use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Deserializer, Serialize, de};

/// Maximum accepted length for optional PTY startup input.
pub const MAX_STARTUP_INPUT_BYTES: usize = 4096;

/// A process launch specification that never relies on shell interpolation.
///
/// Arguments and environment entries remain separate from the executable, so
/// callers can pass them directly to a platform process API. Environment values
/// and argument contents are intentionally redacted from the `Debug`
/// representation.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct CommandSpec {
    executable: String,
    args: Vec<String>,
    cwd: PathBuf,
    env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    env_removals: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    terminal_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    startup_input: Option<String>,
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

    /// Creates a validated command from its structured executable, arguments,
    /// working directory, and environment additions.
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
            env_removals: Vec::new(),
            terminal_title: None,
            startup_input: None,
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

    /// Returns environment variables added or replaced in the child process.
    #[must_use]
    pub const fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    /// Returns environment variables added or replaced in the child process.
    #[must_use]
    pub const fn env_additions(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    /// Returns environment variable names removed before the child starts.
    #[must_use]
    pub fn env_removals(&self) -> &[String] {
        &self.env_removals
    }

    /// Returns the optional terminal title for the session UI.
    #[must_use]
    pub fn terminal_title(&self) -> Option<&str> {
        self.terminal_title.as_deref()
    }

    /// Returns optional bytes to write after the PTY is attached.
    ///
    /// This is never interpolated by a shell. Callers must treat it as raw
    /// terminal input and must not use it to smuggle command strings.
    #[must_use]
    pub fn startup_input(&self) -> Option<&str> {
        self.startup_input.as_deref()
    }

    /// Replaces environment-removal keys after validating them.
    ///
    /// # Errors
    ///
    /// Returns an error if a key is empty, contains `=`, or contains a NUL.
    pub fn with_env_removals<I, S>(mut self, keys: I) -> Result<Self, CommandSpecError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.env_removals = unique_preserving_order(keys.into_iter().map(Into::into));
        self.validate()?;
        Ok(self)
    }

    /// Sets the terminal title. Empty titles are stored as `None`.
    ///
    /// # Errors
    ///
    /// Returns an error if the title contains a NUL byte.
    pub fn with_terminal_title(
        mut self,
        title: impl Into<String>,
    ) -> Result<Self, CommandSpecError> {
        let title = title.into();
        self.terminal_title = if title.is_empty() { None } else { Some(title) };
        self.validate()?;
        Ok(self)
    }

    /// Sets optional PTY startup input. Empty input is stored as `None`.
    ///
    /// # Errors
    ///
    /// Returns an error if the value contains a NUL byte or exceeds
    /// [`MAX_STARTUP_INPUT_BYTES`].
    pub fn with_startup_input(
        mut self,
        input: impl Into<String>,
    ) -> Result<Self, CommandSpecError> {
        let input = input.into();
        self.startup_input = if input.is_empty() { None } else { Some(input) };
        self.validate()?;
        Ok(self)
    }

    fn validate(&self) -> Result<(), CommandSpecError> {
        validate_nonempty_string("executable", &self.executable)?;
        if self.cwd.as_os_str().is_empty() {
            return Err(CommandSpecError::EmptyWorkingDirectory);
        }
        for argument in &self.args {
            validate_no_nul("argument", argument)?;
        }
        validate_structured_invocation(&self.executable, &self.args)?;
        for (key, value) in &self.env {
            validate_environment_key(key)?;
            validate_no_nul("environment value", value)?;
        }
        for key in &self.env_removals {
            validate_environment_key(key)?;
        }
        if let Some(title) = &self.terminal_title {
            validate_no_nul("terminal title", title)?;
        }
        if let Some(input) = &self.startup_input {
            validate_no_nul("startup input", input)?;
            if input.len() > MAX_STARTUP_INPUT_BYTES {
                return Err(CommandSpecError::StartupInputTooLarge {
                    length: input.len(),
                    max: MAX_STARTUP_INPUT_BYTES,
                });
            }
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
            .field("env_removals", &self.env_removals)
            .field("terminal_title", &self.terminal_title)
            .field(
                "startup_input",
                &self.startup_input.as_ref().map(|_| "[redacted]"),
            )
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
            #[serde(default)]
            env_removals: Vec<String>,
            #[serde(default)]
            terminal_title: Option<String>,
            #[serde(default)]
            startup_input: Option<String>,
        }

        let wire = WireCommandSpec::deserialize(deserializer)?;
        let mut command = Self::try_from_parts(wire.executable, wire.args, wire.cwd, wire.env)
            .map_err(de::Error::custom)?;
        command = command
            .with_env_removals(wire.env_removals)
            .map_err(de::Error::custom)?;
        if let Some(title) = wire.terminal_title {
            command = command
                .with_terminal_title(title)
                .map_err(de::Error::custom)?;
        }
        if let Some(input) = wire.startup_input {
            command = command
                .with_startup_input(input)
                .map_err(de::Error::custom)?;
        }
        Ok(command)
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
    /// A known shell or wrapper was asked to interpret a command string.
    ShellCommandString,
    /// Optional startup input exceeded the accepted size.
    StartupInputTooLarge {
        /// Actual byte length of the rejected input.
        length: usize,
        /// Configured maximum byte length.
        max: usize,
    },
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
            Self::ShellCommandString => formatter.write_str(
                "shell command strings are refused; pass an executable and argument array",
            ),
            Self::StartupInputTooLarge { length, max } => write!(
                formatter,
                "startup input is {length} bytes; maximum is {max}"
            ),
        }
    }
}

impl Error for CommandSpecError {}

/// Refuses command-string flags for known shells and direct shell wrappers.
///
/// Interactive shells without a command-string flag remain valid for PTY use.
/// This check is lexical and performs no filesystem I/O; adapters should resolve
/// executable aliases to canonical paths before constructing the command.
///
/// # Errors
///
/// Returns [`CommandSpecError::ShellCommandString`] for POSIX shells,
/// `PowerShell`, `cmd`, `env`, or `BusyBox` invocations that would interpret a
/// command string.
pub fn validate_structured_invocation(
    executable: &str,
    args: &[String],
) -> Result<(), CommandSpecError> {
    if executable_has_name(executable, "env")
        && args.iter().any(|argument| {
            argument == "-S"
                || argument.starts_with("-S") && argument.len() > 2
                || argument == "--split-string"
                || argument.starts_with("--split-string=")
        })
    {
        return Err(CommandSpecError::ShellCommandString);
    }
    if let Some((nested, nested_args)) = wrapped_shell(executable, args) {
        return validate_structured_invocation(nested, nested_args);
    }

    let shell = classify_shell_executable(executable);
    let uses_command_string = match shell {
        Some(ShellKind::Posix) => args.iter().any(|argument| {
            let argument = argument.to_ascii_lowercase();
            argument == "--command"
                || argument.starts_with("--command=")
                || argument == "--init-command"
                || argument.starts_with("--init-command=")
                || argument
                    .strip_prefix('-')
                    .is_some_and(|flags| !flags.starts_with('-') && flags.contains('c'))
        }),
        Some(ShellKind::PowerShell) => args.iter().any(|argument| {
            let flag = argument.trim_start_matches(['-', '/']).to_ascii_lowercase();
            !flag.is_empty()
                && ("command".starts_with(&flag)
                    || "commandwithargs".starts_with(&flag)
                    || "encodedcommand".starts_with(&flag))
        }),
        Some(ShellKind::Cmd) => args.iter().any(|argument| {
            let argument = argument.to_ascii_lowercase();
            argument.starts_with("/c") || argument.starts_with("/k")
        }),
        None => false,
    };
    if uses_command_string {
        Err(CommandSpecError::ShellCommandString)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum ShellKind {
    Posix,
    PowerShell,
    Cmd,
}

fn executable_name(executable: &str) -> Option<&str> {
    Path::new(executable).file_name()?.to_str()
}

fn classify_shell(name: &str) -> Option<ShellKind> {
    let name = name.to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    match name {
        "sh" | "ash" | "bash" | "zsh" | "dash" | "ksh" | "mksh" | "yash" | "posh" | "hush"
        | "fish" | "csh" | "tcsh" | "nu" | "xonsh" | "elvish" => Some(ShellKind::Posix),
        "powershell" | "pwsh" => Some(ShellKind::PowerShell),
        "cmd" => Some(ShellKind::Cmd),
        _ => None,
    }
}

fn classify_shell_executable(executable: &str) -> Option<ShellKind> {
    executable_name(executable).and_then(classify_shell)
}

fn executable_has_name(executable: &str, expected: &str) -> bool {
    executable_name(executable)
        .is_some_and(|name| name.trim_end_matches(".exe").eq_ignore_ascii_case(expected))
}

fn wrapped_shell<'a>(executable: &str, args: &'a [String]) -> Option<(&'a str, &'a [String])> {
    if executable_has_name(executable, "busybox") {
        let (nested, rest) = args.split_first()?;
        return classify_shell_executable(nested).map(|_| (nested.as_str(), rest));
    }
    if !executable_has_name(executable, "env") {
        return None;
    }
    for (index, argument) in args.iter().enumerate() {
        if classify_shell_executable(argument).is_some() {
            return Some((argument, &args[index + 1..]));
        }
    }
    None
}

fn unique_preserving_order(keys: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut ordered = Vec::new();
    for key in keys {
        if !ordered.contains(&key) {
            ordered.push(key);
        }
    }
    ordered
}

fn validate_environment_key(key: &str) -> Result<(), CommandSpecError> {
    if key.is_empty() || key.contains('=') {
        return Err(CommandSpecError::InvalidEnvironmentKey(key.to_owned()));
    }
    validate_no_nul("environment key", key)
}

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
        .with_env_removals(["STALE_TOKEN"])
        .expect("removal keys should be valid")
        .with_terminal_title("Codex")
        .expect("title should be valid")
        .with_startup_input("hello\n")
        .expect("startup input should be valid")
    }

    #[test]
    fn command_round_trips_without_flattening_arguments() {
        let command = command();
        let json = serde_json::to_string(&command).expect("command should serialize");
        let decoded: CommandSpec = serde_json::from_str(&json).expect("command should deserialize");

        assert_eq!(decoded, command);
        assert_eq!(decoded.args(), ["exec", "--token=argument-secret"]);
        assert_eq!(decoded.env_additions(), command.env());
        assert_eq!(decoded.env_removals(), ["STALE_TOKEN"]);
        assert_eq!(decoded.terminal_title(), Some("Codex"));
        assert_eq!(decoded.startup_input(), Some("hello\n"));
        assert!(json.contains("env_removals"));
        assert!(json.contains("terminal_title"));
        assert!(json.contains("startup_input"));
    }

    #[test]
    fn debug_redacts_environment_values_arguments_and_startup_input() {
        let debug = format!("{:?}", command());

        assert!(debug.contains("TOKEN"));
        assert!(debug.contains("args_count: 2"));
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("hello"));
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

    #[test]
    fn rejects_oversized_startup_input() {
        let input = "a".repeat(MAX_STARTUP_INPUT_BYTES + 1);
        let error = CommandSpec::new("codex", "/tmp/project")
            .expect("command should be valid")
            .with_startup_input(input)
            .expect_err("oversized startup input should be rejected");
        assert!(matches!(
            error,
            CommandSpecError::StartupInputTooLarge { .. }
        ));
    }

    #[test]
    fn rejects_shell_command_strings_and_direct_wrappers() {
        for (executable, args) in [
            ("/bin/bash", vec!["-lc", "echo unsafe"]),
            ("zsh", vec!["--command=echo unsafe"]),
            ("pwsh.exe", vec!["-EncodedCommand", "ZQBjAGgAbwA="]),
            ("cmd.exe", vec!["/C", "echo unsafe"]),
            ("/usr/bin/env", vec!["bash", "-c", "echo unsafe"]),
            ("/usr/bin/env", vec!["/bin/bash", "-c", "echo unsafe"]),
            ("busybox", vec!["sh", "-c", "echo unsafe"]),
            ("busybox", vec!["/bin/ash", "-c", "echo unsafe"]),
            ("env", vec!["-S", "bash -c 'echo unsafe'"]),
            ("env", vec!["-Sbash -c 'echo unsafe'"]),
            ("env", vec!["--split-string=bash -c 'echo unsafe'"]),
        ] {
            let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert_eq!(
                validate_structured_invocation(executable, &args),
                Err(CommandSpecError::ShellCommandString),
                "accepted {executable} {args:?}"
            );
        }
    }

    #[test]
    fn allows_interactive_shells_and_metacharacters_as_plain_arguments() {
        CommandSpec::new("/bin/sh", "/tmp/project")
            .expect("an interactive PTY shell has no command string");
        CommandSpec::try_from_parts(
            "codex",
            ["exec", "; rm -rf -- definitely-not-executed"],
            "/tmp/project",
            BTreeMap::new(),
        )
        .expect("metacharacters remain one argument without a shell");
    }

    #[test]
    fn empty_optional_fields_are_omitted() {
        let command = CommandSpec::new("codex", "/tmp/project").expect("command should be valid");
        let json = serde_json::to_string(&command).expect("command should serialize");
        assert!(!json.contains("env_removals"));
        assert!(!json.contains("terminal_title"));
        assert!(!json.contains("startup_input"));
    }
}
