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
    let mut executable = executable;
    let mut args = args;

    // Each known wrapper consumes at least one argument, bounding traversal by
    // the caller-provided argument count without recursive stack growth.
    loop {
        if executable_has_name(executable, "env") {
            let invocation = parse_env_invocation(args);
            if invocation.uses_split_string {
                return Err(CommandSpecError::ShellCommandString);
            }
            let Some(index) = invocation.command_index else {
                return Ok(());
            };
            executable = args[index].as_str();
            args = &args[index + 1..];
            continue;
        }

        if executable_has_name(executable, "busybox") {
            let Some((nested, nested_args)) = args.split_first() else {
                return Ok(());
            };
            executable = nested.as_str();
            args = nested_args;
            continue;
        }

        let uses_command_string = match classify_shell_executable(executable) {
            Some(ShellKind::Posix) => posix_shell_uses_command_string(executable, args),
            Some(ShellKind::PowerShell) => powershell_uses_command_string(args),
            Some(ShellKind::Cmd) => cmd_uses_command_string(args),
            None => false,
        };
        return if uses_command_string {
            Err(CommandSpecError::ShellCommandString)
        } else {
            Ok(())
        };
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

fn posix_shell_uses_command_string(executable: &str, args: &[String]) -> bool {
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" || argument == "-" {
            return false;
        }
        if let Some(long) = argument.strip_prefix("--") {
            let (name, has_attached_value) = long
                .split_once('=')
                .map_or((long, false), |(name, _)| (name, true));
            let name = name.to_ascii_lowercase();
            if matches!(name.as_str(), "command" | "commands" | "init-command")
                || (executable_has_name(executable, "nu") && name == "execute")
            {
                return true;
            }
            index += if posix_long_option_consumes_value(executable, &name) && !has_attached_value {
                2
            } else {
                1
            };
            continue;
        }

        let (sets_options, flags) = argument
            .strip_prefix('-')
            .map(|flags| (true, flags))
            .or_else(|| argument.strip_prefix('+').map(|flags| (false, flags)))
            .unwrap_or((false, ""));
        if flags.is_empty() {
            return false;
        }
        match parse_posix_short_options(executable, flags, sets_options) {
            PosixShortOptions::CommandString => return true,
            PosixShortOptions::ConsumesNext => index += 2,
            PosixShortOptions::Complete => index += 1,
        }
    }
    false
}

fn posix_long_option_consumes_value(executable: &str, name: &str) -> bool {
    if matches!(name, "init-file" | "rcfile") {
        return true;
    }
    if executable_has_name(executable, "fish")
        && matches!(
            name,
            "debug" | "debug-output" | "features" | "profile" | "profile-startup"
        )
    {
        return true;
    }
    executable_has_name(executable, "nu")
        && matches!(
            name,
            "config"
                | "config-home"
                | "env-config"
                | "error-style"
                | "experimental-options"
                | "ide-check"
                | "ide-complete"
                | "ide-goto-def"
                | "ide-hover"
                | "include-path"
                | "log-exclude"
                | "log-file"
                | "log-include"
                | "log-level"
                | "log-target"
                | "mcp-port"
                | "mcp-transport"
                | "plugin-config"
                | "plugins"
                | "table-mode"
        )
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PosixShortOptions {
    CommandString,
    ConsumesNext,
    Complete,
}

fn parse_posix_short_options(
    executable: &str,
    flags: &str,
    sets_options: bool,
) -> PosixShortOptions {
    let mut consumes_next = false;
    for (offset, flag) in flags.char_indices() {
        let command_string = sets_options
            && (flag == 'c'
                || (flag == 'C' && executable_has_name(executable, "fish"))
                || (flag == 'e' && executable_has_name(executable, "nu")));
        if command_string {
            return PosixShortOptions::CommandString;
        }
        if (flag == 'o' && bourne_shell_o_is_clustered(executable))
            || (flag == 'O' && executable_has_name(executable, "bash"))
        {
            consumes_next = true;
            continue;
        }
        let consumes_value = flag == 'o'
            || (executable_has_name(executable, "fish") && matches!(flag, 'd' | 'f' | 'p'))
            || (executable_has_name(executable, "nu") && matches!(flag, 'I' | 'm'));
        if consumes_value {
            return if offset + flag.len_utf8() == flags.len() {
                PosixShortOptions::ConsumesNext
            } else {
                PosixShortOptions::Complete
            };
        }
    }
    if consumes_next {
        PosixShortOptions::ConsumesNext
    } else {
        PosixShortOptions::Complete
    }
}

fn bourne_shell_o_is_clustered(executable: &str) -> bool {
    executable_name(executable).is_some_and(|name| {
        matches!(
            name.trim_end_matches(".exe").to_ascii_lowercase().as_str(),
            "sh" | "ash" | "bash" | "dash" | "ksh" | "mksh" | "yash" | "posh" | "hush"
        )
    })
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PowerShellOption {
    CommandString,
    File,
    Value,
    Switch,
    Terminal,
}

const POWERSHELL_OPTIONS: &[(&str, PowerShellOption)] = &[
    ("command", PowerShellOption::CommandString),
    ("commandwithargs", PowerShellOption::CommandString),
    ("encodedcommand", PowerShellOption::CommandString),
    ("file", PowerShellOption::File),
    ("configurationfile", PowerShellOption::Value),
    ("configurationname", PowerShellOption::Value),
    ("custompipename", PowerShellOption::Value),
    ("encodedarguments", PowerShellOption::Value),
    ("executionpolicy", PowerShellOption::Value),
    ("inputformat", PowerShellOption::Value),
    ("outputformat", PowerShellOption::Value),
    ("psconsolefile", PowerShellOption::Value),
    ("settingsfile", PowerShellOption::Value),
    ("token", PowerShellOption::Value),
    ("utctimestamp", PowerShellOption::Value),
    ("windowstyle", PowerShellOption::Value),
    ("workingdirectory", PowerShellOption::Value),
    ("interactive", PowerShellOption::Switch),
    ("login", PowerShellOption::Switch),
    ("mta", PowerShellOption::Switch),
    ("noexit", PowerShellOption::Switch),
    ("nologo", PowerShellOption::Switch),
    ("noninteractive", PowerShellOption::Switch),
    ("noprofile", PowerShellOption::Switch),
    ("noprofileloadtime", PowerShellOption::Switch),
    ("namedpipeservermode", PowerShellOption::Switch),
    (
        "removeworkingdirectorytrailingcharacter",
        PowerShellOption::Switch,
    ),
    ("servermode", PowerShellOption::Switch),
    ("socketservermode", PowerShellOption::Switch),
    ("sshservermode", PowerShellOption::Switch),
    ("sta", PowerShellOption::Switch),
    ("v2socketservermode", PowerShellOption::Switch),
    ("help", PowerShellOption::Terminal),
    ("version", PowerShellOption::Terminal),
];

const POWERSHELL_OPTION_ALIASES: &[(&str, PowerShellOption)] = &[
    ("c", PowerShellOption::CommandString),
    ("cwa", PowerShellOption::CommandString),
    ("e", PowerShellOption::CommandString),
    ("ec", PowerShellOption::CommandString),
    ("enc", PowerShellOption::CommandString),
    ("f", PowerShellOption::File),
    ("config", PowerShellOption::Value),
    ("ea", PowerShellOption::Value),
    ("ep", PowerShellOption::Value),
    ("ex", PowerShellOption::Value),
    ("if", PowerShellOption::Value),
    ("inp", PowerShellOption::Value),
    ("o", PowerShellOption::Value),
    ("of", PowerShellOption::Value),
    ("settings", PowerShellOption::Value),
    ("w", PowerShellOption::Value),
    ("wd", PowerShellOption::Value),
    ("wo", PowerShellOption::Value),
    ("i", PowerShellOption::Switch),
    ("l", PowerShellOption::Switch),
    ("noe", PowerShellOption::Switch),
    ("nam", PowerShellOption::Switch),
    ("nol", PowerShellOption::Switch),
    ("noni", PowerShellOption::Switch),
    ("nop", PowerShellOption::Switch),
    ("sshs", PowerShellOption::Switch),
    ("s", PowerShellOption::Switch),
    ("so", PowerShellOption::Switch),
    ("to", PowerShellOption::Value),
    ("utc", PowerShellOption::Value),
    ("v2so", PowerShellOption::Switch),
    ("?", PowerShellOption::Terminal),
    ("h", PowerShellOption::Terminal),
    ("v", PowerShellOption::Terminal),
];

fn classify_powershell_option(name: &str) -> Option<PowerShellOption> {
    if let Some((_, option)) = POWERSHELL_OPTION_ALIASES
        .iter()
        .find(|(alias, _)| *alias == name)
    {
        return Some(*option);
    }
    if let Some((_, option)) = POWERSHELL_OPTIONS
        .iter()
        .find(|(candidate, _)| *candidate == name)
    {
        return Some(*option);
    }

    let mut matches = POWERSHELL_OPTIONS
        .iter()
        .filter(|(candidate, _)| candidate.starts_with(name))
        .map(|(_, option)| *option);
    let option = matches.next()?;
    matches.next().is_none().then_some(option)
}

fn powershell_uses_command_string(args: &[String]) -> bool {
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            return false;
        }
        let Some(option) = argument
            .strip_prefix("--")
            .or_else(|| argument.strip_prefix('-'))
            .or_else(|| argument.strip_prefix('/'))
        else {
            return false;
        };
        let (name, has_attached_value) = option
            .split_once([':', '='])
            .map_or((option, false), |(name, _)| (name, true));
        let Some(option) = classify_powershell_option(&name.to_ascii_lowercase()) else {
            return false;
        };
        match option {
            PowerShellOption::CommandString => return true,
            PowerShellOption::File | PowerShellOption::Terminal => return false,
            PowerShellOption::Value => index += if has_attached_value { 1 } else { 2 },
            PowerShellOption::Switch => index += 1,
        }
    }
    false
}

fn cmd_uses_command_string(args: &[String]) -> bool {
    for argument in args {
        let argument = argument.to_ascii_lowercase();
        if argument.starts_with("/c") || argument.starts_with("/k") {
            return true;
        }
        if !matches!(
            argument.as_str(),
            "/a" | "/u"
                | "/q"
                | "/d"
                | "/s"
                | "/e:on"
                | "/e:off"
                | "/f:on"
                | "/f:off"
                | "/v:on"
                | "/v:off"
        ) && !argument
            .strip_prefix("/t:")
            .is_some_and(|colors| matches!(colors.len(), 1 | 2))
        {
            return false;
        }
    }
    false
}

#[derive(Clone, Copy, Default)]
struct EnvInvocation {
    uses_split_string: bool,
    command_index: Option<usize>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum EnvOption {
    Flag,
    RequiredValue,
    OptionalAttachedValue,
    SplitString,
    Invalid,
}

const ENV_LONG_OPTIONS: &[(&str, EnvOption)] = &[
    ("ignore-environment", EnvOption::Flag),
    ("null", EnvOption::Flag),
    ("argv0", EnvOption::RequiredValue),
    ("unset", EnvOption::RequiredValue),
    ("chdir", EnvOption::RequiredValue),
    ("split-string", EnvOption::SplitString),
    ("block-signal", EnvOption::OptionalAttachedValue),
    ("default-signal", EnvOption::OptionalAttachedValue),
    ("ignore-signal", EnvOption::OptionalAttachedValue),
    ("list-signal-handling", EnvOption::Flag),
    ("debug", EnvOption::Flag),
    ("help", EnvOption::Flag),
    ("version", EnvOption::Flag),
];

fn parse_env_invocation(args: &[String]) -> EnvInvocation {
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            index += 1;
            break;
        }
        if argument == "-" {
            index += 1;
            continue;
        }
        if let Some(long) = argument.strip_prefix("--") {
            let (option, has_attached_value) = parse_env_long_option(long);
            match option {
                EnvOption::SplitString => {
                    return EnvInvocation {
                        uses_split_string: true,
                        command_index: None,
                    };
                }
                EnvOption::RequiredValue if !has_attached_value => {
                    if args.get(index + 1).is_none() {
                        return EnvInvocation::default();
                    }
                    index += 2;
                }
                EnvOption::Flag if has_attached_value => {
                    return EnvInvocation::default();
                }
                EnvOption::Invalid => {
                    return EnvInvocation::default();
                }
                EnvOption::Flag | EnvOption::RequiredValue | EnvOption::OptionalAttachedValue => {
                    index += 1;
                }
            }
            continue;
        }
        let Some(cluster) = argument.strip_prefix('-') else {
            break;
        };
        match parse_env_short_options(cluster) {
            EnvOption::SplitString => {
                return EnvInvocation {
                    uses_split_string: true,
                    command_index: None,
                };
            }
            EnvOption::RequiredValue => {
                if args.get(index + 1).is_none() {
                    return EnvInvocation::default();
                }
                index += 2;
            }
            EnvOption::Flag | EnvOption::OptionalAttachedValue => index += 1,
            EnvOption::Invalid => return EnvInvocation::default(),
        }
    }

    while args.get(index).is_some_and(|argument| {
        argument
            .split_once('=')
            .is_some_and(|(name, _)| !name.is_empty())
    }) {
        index += 1;
    }
    EnvInvocation {
        uses_split_string: false,
        command_index: (index < args.len()).then_some(index),
    }
}

fn parse_env_short_options(cluster: &str) -> EnvOption {
    let mut options = cluster.char_indices().peekable();
    while let Some((_, option)) = options.next() {
        match option {
            '0' | 'i' | 'v' => {}
            'S' => return EnvOption::SplitString,
            'a' | 'u' | 'C' | 'P' => {
                return if options.peek().is_some() {
                    EnvOption::Flag
                } else {
                    EnvOption::RequiredValue
                };
            }
            _ => return EnvOption::Invalid,
        }
    }
    EnvOption::Flag
}

fn parse_env_long_option(option: &str) -> (EnvOption, bool) {
    let (name, attached_value) = option
        .split_once('=')
        .map_or((option, false), |(name, _)| (name, true));
    if name.is_empty() {
        return (EnvOption::Invalid, attached_value);
    }
    let exact = ENV_LONG_OPTIONS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, kind)| *kind);
    let kind = exact.unwrap_or_else(|| {
        let mut matches = ENV_LONG_OPTIONS
            .iter()
            .filter(|(candidate, _)| candidate.starts_with(name))
            .map(|(_, kind)| *kind);
        let first = matches.next().unwrap_or(EnvOption::Invalid);
        if matches.next().is_some() {
            EnvOption::Invalid
        } else {
            first
        }
    });
    (kind, attached_value)
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
            ("/usr/bin/env", vec!["-iS", "sh -c 'echo unsafe'"]),
            ("env", vec!["-viS", "sh -c 'echo unsafe'"]),
            ("env", vec!["-0ivSsh -c 'echo unsafe'"]),
            (
                "env",
                vec!["-a", "custom-argv0", "-S", "sh -c 'echo unsafe'"],
            ),
            ("env", vec!["-iacustom-argv0", "-Ssh -c 'echo unsafe'"]),
            (
                "env",
                vec![
                    "--argv0",
                    "custom-argv0",
                    "--split-string",
                    "sh -c 'echo unsafe'",
                ],
            ),
            ("env", vec!["--split-string", "bash -c 'echo unsafe'"]),
            ("env", vec!["--split-string=bash -c 'echo unsafe'"]),
            ("env", vec!["--spl=bash -c 'echo unsafe'"]),
            ("env", vec!["-uS", "/bin/sh", "-c", "echo unsafe"]),
            ("env", vec!["-C/tmp", "/bin/sh", "-c", "echo unsafe"]),
            (
                "env",
                vec!["--", "SAFE=value", "/bin/sh", "-c", "echo unsafe"],
            ),
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
    fn env_option_parser_keeps_values_and_post_terminator_args_structured() {
        for args in [
            vec!["-uS", "codex", "-c", "plain argument"],
            vec!["-iuS", "codex", "-c", "plain argument"],
            vec!["-u", "sh", "codex", "-c", "plain argument"],
            vec!["-C/bin/sh", "codex", "-c", "plain argument"],
            vec!["-C", "/bin/sh", "codex", "-c", "plain argument"],
            vec!["-P/bin/sh", "codex", "-c", "plain argument"],
            vec!["-aS", "codex", "-c", "plain argument"],
            vec!["-iaS", "codex", "-c", "plain argument"],
            vec!["--argv0=S", "codex", "-c", "plain argument"],
            vec!["--unset=sh", "codex", "-c", "plain argument"],
            vec!["--chdir", "/bin/sh", "codex", "-c", "plain argument"],
            vec!["--", "-S", "sh -c 'not interpreted'"],
            vec!["--", "--split-string=sh -c 'not interpreted'"],
            vec!["-xS", "sh -c 'invalid env option, not interpreted'"],
        ] {
            let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert_eq!(
                validate_structured_invocation("/usr/bin/env", &args),
                Ok(()),
                "rejected structured env arguments: {args:?}"
            );
        }
    }

    #[test]
    fn rejects_shell_command_strings_through_nested_known_wrappers() {
        for (executable, args) in [
            (
                "/usr/bin/env",
                vec!["/usr/bin/env", "/bin/sh", "-c", "echo unsafe"],
            ),
            ("env", vec!["busybox", "sh", "-c", "echo unsafe"]),
            ("busybox", vec!["env", "-S", "sh -c 'echo unsafe'"]),
            ("/bin/busybox", vec!["busybox", "ash", "-c", "echo unsafe"]),
        ] {
            let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert_eq!(
                validate_structured_invocation(executable, &args),
                Err(CommandSpecError::ShellCommandString),
                "accepted nested wrapper chain {executable} {args:?}"
            );
        }
    }

    #[test]
    fn allows_structured_commands_through_nested_known_wrappers() {
        for (executable, args) in [
            ("/usr/bin/env", vec!["env", "codex", "-c", "plain argument"]),
            ("env", vec!["busybox", "codex", "-c", "plain argument"]),
            (
                "busybox",
                vec!["env", "--", "codex", "-c", "plain argument"],
            ),
            (
                "/bin/busybox",
                vec!["busybox", "codex", "-c", "plain argument"],
            ),
            ("env", vec!["env", "/bin/sh"]),
        ] {
            let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert_eq!(
                validate_structured_invocation(executable, &args),
                Ok(()),
                "rejected structured nested wrapper chain {executable} {args:?}"
            );
        }
    }

    #[test]
    fn shell_option_parsers_respect_script_and_option_boundaries() {
        for (executable, args) in [
            ("/bin/sh", vec!["--", "-c"]),
            ("bash", vec!["script.sh", "-c"]),
            ("bash", vec!["-o", "posix", "script.sh", "-c"]),
            ("bash", vec!["-O", "extglob", "script.sh", "-c"]),
            ("fish", vec!["-p", "/tmp/profile", "script.fish", "-c"]),
            ("fish", vec!["-p/tmp/cache", "script.fish", "-c"]),
            ("fish", vec!["-dcomplete", "script.fish", "-c"]),
            ("nu", vec!["--config", "/tmp/config.nu", "script.nu", "-e"]),
            ("nu", vec!["-Iconfig", "script.nu", "-e"]),
            ("nu", vec!["-mrounded", "script.nu", "-e"]),
            ("pwsh", vec!["-File", "script.ps1", "c"]),
            ("pwsh", vec!["script.ps1", "-Command", "plain argument"]),
            ("pwsh", vec!["--", "-EncodedCommand", "plain argument"]),
            (
                "pwsh",
                vec![
                    "-WorkingDirectory",
                    "/tmp",
                    "-File",
                    "script.ps1",
                    "-Command",
                ],
            ),
            ("cmd.exe", vec!["script.cmd", "/C", "plain argument"]),
        ] {
            let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert_eq!(
                validate_structured_invocation(executable, &args),
                Ok(()),
                "rejected arguments after a shell parsing boundary: {executable} {args:?}"
            );
        }
    }

    #[test]
    fn shell_option_parsers_reject_effective_command_string_options() {
        for (executable, args) in [
            ("bash", vec!["-o", "posix", "-c", "echo unsafe"]),
            ("bash", vec!["-oc", "posix", "echo unsafe"]),
            ("/bin/sh", vec!["-oc", "posix", "echo unsafe"]),
            ("zsh", vec!["-oshwordsplit", "-c", "echo unsafe"]),
            ("bash", vec!["--rcfile", "/tmp/bashrc", "-c", "echo unsafe"]),
            ("nu", vec!["--commands", "echo unsafe"]),
            ("nu", vec!["-e", "echo unsafe"]),
            ("nu", vec!["--execute", "echo unsafe"]),
            (
                "nu",
                vec!["--config", "/tmp/config.nu", "-e", "echo unsafe"],
            ),
            ("fish", vec!["-d", "warning", "-c", "echo unsafe"]),
            ("fish", vec!["-p", "/tmp/profile", "-c", "echo unsafe"]),
            ("fish", vec!["-p/tmp/profile", "-c", "echo unsafe"]),
            ("fish", vec!["-f", "qmark-noglob", "-c", "echo unsafe"]),
            (
                "fish",
                vec!["--debug-output", "/tmp/fish.log", "-c", "echo unsafe"],
            ),
            (
                "pwsh",
                vec!["-WorkingDirectory", "/tmp", "-Command", "echo unsafe"],
            ),
            ("pwsh", vec!["-NoProfile", "-ec", "ZQBjAGgAbwA="]),
            ("pwsh", vec!["-ea", "YQByAGcAcwA=", "-c", "echo unsafe"]),
            ("pwsh", vec!["-so", "-c", "echo unsafe"]),
            ("pwsh", vec!["-to", "token", "-c", "echo unsafe"]),
            ("cmd.exe", vec!["/D", "/C", "echo unsafe"]),
        ] {
            let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert_eq!(
                validate_structured_invocation(executable, &args),
                Err(CommandSpecError::ShellCommandString),
                "accepted an effective shell command-string option: {executable} {args:?}"
            );
        }
    }

    #[test]
    fn deeply_nested_known_wrappers_do_not_use_recursive_validation() {
        let mut args = vec!["env".to_owned(); 4_096];
        args.extend(["sh".to_owned(), "-c".to_owned(), "echo unsafe".to_owned()]);
        assert_eq!(
            validate_structured_invocation("env", &args),
            Err(CommandSpecError::ShellCommandString)
        );
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
