use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// Marker substituted for secret-like values in logs and IPC details.
pub const REDACTED: &str = "[redacted]";

const SENSITIVE_PARTS: &[&str] = &[
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSPHRASE",
    "COOKIE",
    "CREDENTIAL",
    "AUTHORIZATION",
    "AUTHENTICATION",
];

/// Returns whether an environment or field name should be redacted.
#[must_use]
pub fn is_sensitive_name(name: &str) -> bool {
    let normalized = normalize_name(name);
    if normalized.contains("API_KEY") || normalized.contains("APIKEY") {
        return true;
    }
    normalized.split('_').any(is_sensitive_part)
}

/// Replaces a secret-like value with [`REDACTED`].
#[must_use]
pub fn redact_value(name: &str, value: &str) -> String {
    if is_sensitive_name(name) {
        REDACTED.to_owned()
    } else {
        value.to_owned()
    }
}

/// Redacts values whose keys look secret-like.
#[must_use]
pub fn redact_map(values: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| (key.clone(), redact_value(key, value)))
        .collect()
}

/// Redacts a JSON value associated with `key`.
#[must_use]
pub fn redact_json_value(key: &str, value: Value) -> Value {
    if is_sensitive_name(key) {
        return Value::String(REDACTED.to_owned());
    }
    match value {
        Value::Object(map) => Value::Object(redact_json_map(map)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| redact_json_value(key, item))
                .collect(),
        ),
        Value::String(text) => Value::String(redact_text(&text)),
        other => other,
    }
}

/// Redacts assignment-style secrets and Bearer tokens in free-form text.
#[must_use]
pub fn redact_text(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut pending_bearer = false;

    for token in tokenize_preserving_whitespace(text) {
        if token.chars().all(char::is_whitespace) {
            output.push_str(&token);
            continue;
        }

        if pending_bearer {
            output.push_str(REDACTED);
            pending_bearer = false;
            continue;
        }

        if token.eq_ignore_ascii_case("bearer") {
            output.push_str(&token);
            pending_bearer = true;
            continue;
        }

        output.push_str(&redact_assignment_token(&token));
    }

    if pending_bearer {
        output.push_str(REDACTED);
    }

    output
}

fn redact_json_map(map: Map<String, Value>) -> Map<String, Value> {
    map.into_iter()
        .map(|(key, value)| {
            let redacted = redact_json_value(&key, value);
            (key, redacted)
        })
        .collect()
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character == '-' || character == '.' || character == '/' {
                '_'
            } else {
                character.to_ascii_uppercase()
            }
        })
        .collect()
}

fn is_sensitive_part(part: &str) -> bool {
    if part == "AUTH" || SENSITIVE_PARTS.contains(&part) {
        return true;
    }
    SENSITIVE_PARTS.iter().any(|needle| part.contains(needle))
}

fn redact_assignment_token(token: &str) -> String {
    let separator = if token.contains('=') {
        Some('=')
    } else if let Some(stripped) = token.strip_prefix("--") {
        if stripped.contains('=') {
            Some('=')
        } else {
            None
        }
    } else {
        None
    };

    let Some(separator) = separator else {
        return token.to_owned();
    };

    let Some((raw_key, _value)) = token.split_once(separator) else {
        return token.to_owned();
    };
    let key = raw_key.trim_start_matches('-');
    if is_sensitive_name(key) {
        format!("{raw_key}{separator}{REDACTED}")
    } else {
        token.to_owned()
    }
}

fn tokenize_preserving_whitespace(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_whitespace = text.chars().next().is_some_and(char::is_whitespace);

    for character in text.chars() {
        if character.is_whitespace() != in_whitespace && !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
            in_whitespace = character.is_whitespace();
        }
        current.push(character);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_sensitive_environment_names() {
        for name in [
            "TOKEN",
            "GH_TOKEN",
            "access-token",
            "SECRET",
            "AWS_SECRET_ACCESS_KEY",
            "PASSWORD",
            "DB_PASSWORD",
            "API_KEY",
            "openai-api-key",
            "AUTH",
            "AUTHORIZATION",
            "COOKIE",
            "HTTP_COOKIE",
            "CREDENTIAL",
            "credentials_file",
        ] {
            assert!(is_sensitive_name(name), "{name} should be sensitive");
        }
    }

    #[test]
    fn does_not_treat_author_as_auth() {
        assert!(!is_sensitive_name("AUTHOR"));
        assert!(!is_sensitive_name("AUTHOR_NAME"));
        assert!(!is_sensitive_name("HOME"));
        assert!(!is_sensitive_name("PATH"));
    }

    #[test]
    fn redacts_assignment_and_bearer_text() {
        let text = "TOKEN=abc123 --password=hunter2 AUTHORIZATION=leaked Bearer abc PATH=/usr/bin";
        let redacted = redact_text(text);
        assert!(redacted.contains("TOKEN=[redacted]"));
        assert!(redacted.contains("--password=[redacted]"));
        assert!(redacted.contains("AUTHORIZATION=[redacted]"));
        assert!(redacted.contains("Bearer [redacted]"));
        assert!(redacted.contains("PATH=/usr/bin"));
        assert!(!redacted.contains("abc123"));
        assert!(!redacted.contains("hunter2"));
        assert!(!redacted.contains("leaked"));
        assert!(!redacted.contains("Bearer abc"));
    }

    #[test]
    fn redacts_map_values_but_keeps_safe_keys() {
        let values = BTreeMap::from([
            ("TOKEN".to_owned(), "secret".to_owned()),
            ("MODE".to_owned(), "interactive".to_owned()),
        ]);
        let redacted = redact_map(&values);
        assert_eq!(redacted["TOKEN"], REDACTED);
        assert_eq!(redacted["MODE"], "interactive");
    }
}
