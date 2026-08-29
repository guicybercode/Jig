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
    if normalized.contains("API_KEY")
        || normalized.contains("APIKEY")
        || normalized.contains("ACCESS_KEY")
        || normalized.contains("PRIVATE_KEY")
        || normalized.contains("SIGNING_KEY")
    {
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
    let text = redact_private_key_blocks(text);
    let mut output = String::with_capacity(text.len());
    let mut pending_bearer = false;
    let mut pending_sensitive_value = false;
    let mut quoted_secret: Option<char> = None;
    let mut redact_line_remainder = false;

    for token in tokenize_preserving_whitespace(&text) {
        if token.chars().all(char::is_whitespace) {
            output.push_str(&token);
            if token.contains(['\n', '\r']) {
                redact_line_remainder = false;
            }
            continue;
        }

        if redact_line_remainder {
            output.push_str(REDACTED);
            continue;
        }

        if let Some(quote) = quoted_secret {
            if token.contains(quote) {
                quoted_secret = None;
            }
            continue;
        }

        if pending_bearer {
            output.push_str(REDACTED);
            quoted_secret = unclosed_quote(&token);
            pending_bearer = false;
            continue;
        }

        if pending_sensitive_value {
            if token == "=" || token == ":" {
                output.push_str(&token);
                continue;
            }
            if is_authorization_scheme(&token) {
                output.push_str(REDACTED);
                pending_bearer = true;
                pending_sensitive_value = false;
                continue;
            }
            output.push_str(REDACTED);
            quoted_secret = unclosed_quote(&token);
            pending_sensitive_value = false;
            continue;
        }

        if is_authorization_scheme(&token) {
            output.push_str(&token);
            pending_bearer = true;
            continue;
        }

        if let Some(redacted) = redact_assignment_token(&token) {
            output.push_str(&redacted.text);
            pending_sensitive_value = redacted.value_was_empty && !redacted.redact_line_remainder;
            quoted_secret = redacted.unclosed_quote;
            redact_line_remainder = redacted.redact_line_remainder;
            continue;
        }

        if is_sensitive_name(
            token
                .trim_matches(['"', '\'', ':', '='])
                .trim_start_matches('-'),
        ) {
            output.push_str(&token);
            pending_sensitive_value = true;
            continue;
        }

        if looks_like_secret_value(&token) {
            output.push_str(REDACTED);
        } else {
            output.push_str(&token);
        }
    }

    if pending_bearer || pending_sensitive_value {
        output.push_str(REDACTED);
    }

    output
}

fn redact_private_key_blocks(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut remaining = text;
    while let Some(begin) = remaining.find("-----BEGIN") {
        let after_begin = &remaining[begin..];
        let Some(header_end) = after_begin.find("PRIVATE KEY-----") else {
            output.push_str(remaining);
            return output;
        };
        output.push_str(&remaining[..begin]);
        output.push_str("[redacted private key]");
        let body = &after_begin[header_end + "PRIVATE KEY-----".len()..];
        let Some(end_begin) = body.find("-----END") else {
            return output;
        };
        let after_end = &body[end_begin..];
        let Some(end) = after_end.find("PRIVATE KEY-----") else {
            return output;
        };
        remaining = &after_end[end + "PRIVATE KEY-----".len()..];
    }
    output.push_str(remaining);
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

struct RedactedAssignment {
    text: String,
    value_was_empty: bool,
    unclosed_quote: Option<char>,
    redact_line_remainder: bool,
}

fn redact_assignment_token(token: &str) -> Option<RedactedAssignment> {
    let (separator_index, separator) = token
        .char_indices()
        .find(|(_, character)| matches!(character, '=' | ':'))?;
    let raw_key = &token[..separator_index];
    let key = raw_key.trim_matches(['"', '\'']).trim_start_matches('-');
    let value = &token[separator_index + separator.len_utf8()..];
    if !is_sensitive_name(key) && !looks_like_secret_value(value) {
        return None;
    }
    Some(RedactedAssignment {
        text: format!("{raw_key}{separator}{REDACTED}"),
        value_was_empty: value.is_empty(),
        unclosed_quote: unclosed_quote(value),
        redact_line_remainder: separator == ':' && is_sensitive_name(key),
    })
}

fn is_authorization_scheme(token: &str) -> bool {
    matches!(
        token
            .trim_matches(|character: char| !character.is_ascii_alphabetic())
            .to_ascii_lowercase()
            .as_str(),
        "bearer" | "basic" | "digest" | "negotiate"
    )
}

fn unclosed_quote(value: &str) -> Option<char> {
    let quote = value
        .chars()
        .next()
        .filter(|character| matches!(character, '"' | '\''))?;
    let closing_count = value
        .chars()
        .filter(|character| *character == quote)
        .count();
    (closing_count % 2 == 1).then_some(quote)
}

fn looks_like_secret_value(token: &str) -> bool {
    let value = token.trim_matches(|character: char| {
        character.is_ascii_punctuation() && !matches!(character, '_' | '-' | '.')
    });
    let lower = value.to_ascii_lowercase();
    if [
        "sk_live_",
        "sk_test_",
        "sk-proj-",
        "sk-ant-",
        "rk_live_",
        "ghp_",
        "gho_",
        "ghs_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "glpat-",
        "npm_",
        "pypi-",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }
    if (value.starts_with("AKIA") || value.starts_with("ASIA"))
        && value.len() >= 16
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return true;
    }
    value.starts_with("eyJ") && value.matches('.').count() == 2 && value.len() >= 24
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
    fn redacts_spaced_quoted_and_pattern_based_secrets() {
        let text = concat!(
            "TOKEN = top-secret ",
            "password=\"first middlepiece last\" ",
            "Authorization: Bearer header-secret ",
            "aws=AKIAIOSFODNN7EXAMPLE ",
            "stripe=sk_live_1234567890 ",
            "jwt=eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.signature"
        );
        let redacted = redact_text(text);
        for secret in [
            "top-secret",
            "first",
            "middlepiece",
            "last",
            "header-secret",
            "AKIAIOSFODNN7EXAMPLE",
            "sk_live_1234567890",
            "eyJhbGciOiJIUzI1NiJ9",
        ] {
            assert!(!redacted.contains(secret), "leaked {secret}: {redacted}");
        }
    }

    #[test]
    fn redacts_private_key_blocks() {
        let text = "before -----BEGIN PRIVATE KEY-----\nvery-secret-key-material\n-----END PRIVATE KEY----- after";
        let redacted = redact_text(text);
        assert_eq!(redacted, "before [redacted private key] after");
        assert!(!redacted.contains("very-secret-key-material"));
    }

    #[test]
    fn redacts_basic_authorization_and_complete_cookie_headers() {
        let text = concat!(
            "Authorization: Basic dXNlcjpwYXNzd29yZA==\n",
            "Cookie: session=first-secret preference=second-secret\n",
            "safe line"
        );
        let redacted = redact_text(text);
        for secret in ["dXNlcjpwYXNzd29yZA==", "first-secret", "second-secret"] {
            assert!(!redacted.contains(secret), "leaked {secret}: {redacted}");
        }
        assert!(redacted.contains("safe line"));
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
