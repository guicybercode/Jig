use std::{collections::BTreeMap, error::Error, fmt, path::PathBuf};

use serde::{Deserialize, Deserializer, Serialize, de};

/// A process launch specification that never relies on shell interpolation.
///
/// Arguments and environment entries remain separate from the executable, so
/// callers can pass them directly to a platform process API. Environment values
/// values and argument contents are intentionally redacted from the `Debug`
/// representation.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct CommandSpec {
    executable: String,
    args: Vec<String>,
    cwd: PathBuf,
    env: BTreeMap<String, String>,
}

impl CommandSpec {
    /// Creates a command with no arguments or environment overrides.
    ///
    /// # Errors
    ///
    /// Returns an error if the executable is empty, any string contains a NUL
    /// byte, the working directory is empty, or an environment key is invalid.
    pub fn new(
        executable: impl Into<String>,
        cwd: impl Into<PathBuf>,
    ) -> Result<Self, CommandSpecError> {
        Self::try_from_parts(executable, Vec::<String>::new(), cwd, BTreeMap::new())
    }

    /// Creates a validated command from all of its structured components.
    ///
    /// # Errors
    ///
    /// Returns an error if the executable is empty, any string contains a NUL
    /// byte, the working directory is empty, or an environment key is invalid.
    pub fn try_from_parts<E, A, S, P>(
        executable: E,
        args: A,
        cwd: P,
        env: BTreeMap<String, String>,
    ) -> Result<Self, CommandSpecError>
    where
        E: Into<String>,
        A: IntoIterator<Item = S>,
        S: Into<String>,
        P: Into<PathBuf>,
    {
        let command = Self {
            executable: executable.into(),
            args: args.into_iter().map(Into::into).collect(),
            cwd: cwd.into(),
            env,
        };
        command.validate()?;
        Ok(command)
    }

    /// Returns the executable name or path.
    #[must_use]
    pub fn executable(&self) -> &str {
        &self.executable
    }

    /// Returns the ordered argument list.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Returns the requested process working directory.
    #[must_use]
    pub fn cwd(&self) -> &PathBuf {
        &self.cwd
    }

    /// Returns environment variables that override the inherited environment.
    #[must_use]
    pub const fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    fn validate(&self) -> Result<(), CommandSpecError> {
        validate_nonempty_string("executable", &self.executable)?;
        if self.cwd.as_os_str().is_empty() {
            return Err(CommandSpecError::EmptyWorkingDirectory);
        }
        for argument in &self.args {
            validate_no_nul("argument", argument)?;
        }
        for (key, value) in &self.env {
            if key.is_empty() || key.contains('=') {
                return Err(CommandSpecError::InvalidEnvironmentKey(key.clone()));
            }
            validate_no_nul("environment key", key)?;
            validate_no_nul("environment value", value)?;
        }
        Ok(())
    }
}

impl fmt::Debug for CommandSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandSpec")
            .field("executable", &self.executable)
            .field("args_count", &self.args.len())
            .field("cwd", &self.cwd)
            .field("env_keys", &self.env.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl<'de> Deserialize<'de> for CommandSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireCommandSpec {
            executable: String,
            args: Vec<String>,
            cwd: PathBuf,
            #[serde(default)]
            env: BTreeMap<String, String>,
        }

        let wire = WireCommandSpec::deserialize(deserializer)?;
        Self::try_from_parts(wire.executable, wire.args, wire.cwd, wire.env)
            .map_err(de::Error::custom)
    }
}

/// Validation failure for a [`CommandSpec`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandSpecError {
    /// The executable was empty.
    EmptyExecutable,
    /// The working directory was empty.
    EmptyWorkingDirectory,
    /// An environment variable name was empty or contained `=`.
    InvalidEnvironmentKey(String),
    /// A value contained a NUL byte and cannot be passed to an OS process API.
    ContainsNul(&'static str),
}

impl fmt::Display for CommandSpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyExecutable => formatter.write_str("executable must not be empty"),
            Self::EmptyWorkingDirectory => {
                formatter.write_str("working directory must not be empty")
            }
            Self::InvalidEnvironmentKey(key) => {
                write!(formatter, "invalid environment variable name: {key:?}")
            }
            Self::ContainsNul(field) => write!(formatter, "{field} must not contain a NUL byte"),
        }
    }
}

impl Error for CommandSpecError {}

fn validate_nonempty_string(field: &'static str, value: &str) -> Result<(), CommandSpecError> {
    if value.is_empty() {
        return match field {
            "executable" => Err(CommandSpecError::EmptyExecutable),
            _ => Err(CommandSpecError::ContainsNul(field)),
        };
    }
    validate_no_nul(field, value)
}

fn validate_no_nul(field: &'static str, value: &str) -> Result<(), CommandSpecError> {
    if value.contains('\0') {
        Err(CommandSpecError::ContainsNul(field))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command() -> CommandSpec {
        CommandSpec::try_from_parts(
            "codex",
            ["exec", "--token=argument-secret"],
            "/tmp/project",
            BTreeMap::from([("TOKEN".to_owned(), "secret".to_owned())]),
        )
        .expect("fixture command should be valid")
    }

    #[test]
    fn command_round_trips_without_flattening_arguments() {
        let command = command();
        let json = serde_json::to_string(&command).expect("command should serialize");
        let decoded: CommandSpec = serde_json::from_str(&json).expect("command should deserialize");

        assert_eq!(decoded, command);
        assert_eq!(decoded.args(), ["exec", "--token=argument-secret"]);
    }

    #[test]
    fn debug_redacts_environment_values_and_argument_contents() {
        let debug = format!("{:?}", command());

        assert!(debug.contains("TOKEN"));
        assert!(debug.contains("args_count: 2"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn empty_executable_and_nul_bytes_are_rejected() {
        assert_eq!(
            CommandSpec::new("", "/tmp").expect_err("empty executable"),
            CommandSpecError::EmptyExecutable
        );
        assert_eq!(
            CommandSpec::new("codex\0", "/tmp").expect_err("nul executable"),
            CommandSpecError::ContainsNul("executable")
        );
        assert_eq!(
            CommandSpec::new("codex", "").expect_err("empty cwd"),
            CommandSpecError::EmptyWorkingDirectory
        );
        let error =
            CommandSpec::try_from_parts("codex", ["ok", "bad\0arg"], "/tmp", BTreeMap::new())
                .expect_err("nul argument");
        assert_eq!(error, CommandSpecError::ContainsNul("argument"));
    }

    #[test]
    fn deserialization_rejects_invalid_environment_keys() {
        let json = r#"{
            "executable":"codex",
            "args":[],
            "cwd":"/tmp/project",
            "env":{"BAD=KEY":"value"}
        }"#;

        let error = serde_json::from_str::<CommandSpec>(json)
            .expect_err("invalid environment key should be rejected");
        assert!(
            error
                .to_string()
                .contains("invalid environment variable name")
        );
    }
}
