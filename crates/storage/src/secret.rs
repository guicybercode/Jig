use std::collections::BTreeMap;

use crate::error::{EntityKind, StorageError, StorageErrorKind};

const MAX_ENV_OVERRIDES: usize = 32;

const SECRET_MARKERS: &[&str] = &[
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "API_KEY",
    "APIKEY",
    "AUTHORIZATION",
    "CREDENTIAL",
    "PRIVATE_KEY",
    "ACCESS_KEY",
    "SESSION_KEY",
    "AUTH_TOKEN",
];

const FULL_ENV_MARKERS: &[&str] = &[
    "HOME",
    "USER",
    "SHELL",
    "PWD",
    "OLDPWD",
    "TERM",
    "SSH_AUTH_SOCK",
];

pub(crate) fn env_key_looks_secret(key: &str) -> bool {
    let normalized = key.to_ascii_uppercase();
    SECRET_MARKERS
        .iter()
        .any(|marker| normalized.contains(marker))
}

pub(crate) fn looks_like_full_environment(env: &BTreeMap<String, String>) -> bool {
    if env.len() > MAX_ENV_OVERRIDES {
        return true;
    }
    let present = FULL_ENV_MARKERS
        .iter()
        .filter(|marker| {
            env.contains_key(**marker) || env.keys().any(|key| key.eq_ignore_ascii_case(marker))
        })
        .count();
    present >= 3
}

pub(crate) fn validate_allowed_env(env: &BTreeMap<String, String>) -> Result<(), StorageError> {
    if looks_like_full_environment(env) {
        return Err(StorageError::new(
            "validate env",
            EntityKind::Agent,
            StorageErrorKind::FullEnvironmentRejected,
            "Store only explicit non-secret overrides. Agents inherit local authentication.",
        ));
    }
    if env.keys().any(|key| env_key_looks_secret(key)) {
        return Err(StorageError::new(
            "validate env",
            EntityKind::Agent,
            StorageErrorKind::SecretRejected,
            "Remove token-like environment keys. Authentication stays with the installed CLI.",
        ));
    }
    Ok(())
}

pub(crate) fn setting_key_is_forbidden(key: &str) -> bool {
    env_key_looks_secret(key)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{env_key_looks_secret, validate_allowed_env};

    #[test]
    fn rejects_token_keys() {
        assert!(env_key_looks_secret("OPENAI_API_KEY"));
        assert!(env_key_looks_secret("auth_token"));
        assert!(!env_key_looks_secret("PATH"));
        assert!(!env_key_looks_secret("CLI_MASTER_SEARCH_PATH"));
    }

    #[test]
    fn rejects_full_process_environment() {
        let env = BTreeMap::from([
            ("HOME".to_owned(), "/home/dev".to_owned()),
            ("USER".to_owned(), "dev".to_owned()),
            ("SHELL".to_owned(), "/bin/zsh".to_owned()),
            ("PATH".to_owned(), "/usr/bin".to_owned()),
        ]);
        let error = validate_allowed_env(&env).expect_err("full env must be rejected");
        assert!(error.to_string().contains("full process environment"));
    }
}
