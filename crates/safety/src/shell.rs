use std::ffi::OsString;
use std::path::{Path, PathBuf};

use cli_master_core::{ApplicationError, ErrorCode, LOGIN_SHELL_PATH_TIMEOUT};

use crate::process::SpawnRequest;

/// Constant command passed to a login shell when importing PATH.
///
/// The command string is never concatenated with user input. The only
/// caller-controlled value is the absolute shell executable.
pub const LOGIN_SHELL_PATH_COMMAND: &str = r#"printf '%s\n' "$PATH""#;

/// Imports PATH from a login shell using a constant, non-interpolated command.
///
/// This is the only supported `sh -c` / `zsh -lc` exception. Custom agent
/// definitions cannot use it.
///
/// # Errors
///
/// Returns an error when the shell is not an absolute executable, the command
/// times out, or PATH cannot be parsed.
pub fn import_login_path(shell: impl AsRef<Path>) -> Result<OsString, ApplicationError> {
    let shell = shell.as_ref();
    if !shell.is_absolute() {
        return Err(ApplicationError::new(
            ErrorCode::InvalidPath,
            "Login shell path must be absolute.",
        )
        .with_action("Configure an absolute shell such as /bin/zsh or /bin/bash.")
        .with_context("shell", shell.display().to_string()));
    }

    let file_name = shell
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let args: &[&str] = match file_name {
        "zsh" | "bash" => &["-lc", LOGIN_SHELL_PATH_COMMAND],
        "sh" | "dash" => &["-c", LOGIN_SHELL_PATH_COMMAND],
        _ => {
            return Err(ApplicationError::new(
                ErrorCode::ShellInvocationRefused,
                "Only bash, zsh, or sh may be used to import PATH.",
            )
            .with_action("Use /bin/zsh, /bin/bash, or /bin/sh."));
        }
    };

    // This is a deliberate, documented exception to assert_structured_command.
    let output = run_login_shell(shell, args)?;
    if !output.success() {
        return Err(ApplicationError::new(
            ErrorCode::PtySpawnFailed,
            "The login shell did not print PATH.",
        )
        .with_action("Inspect the shell startup files, then retry PATH import."));
    }
    Ok(OsString::from(output.stdout_text()))
}

fn run_login_shell(
    shell: &Path,
    args: &[&str],
) -> Result<crate::process::ProcessOutput, ApplicationError> {
    let mut request = SpawnRequest::new(PathBuf::from(shell)).timeout(LOGIN_SHELL_PATH_TIMEOUT);
    for argument in args {
        request = request.arg(*argument);
    }
    crate::process::run_command_unchecked(&request.allow_login_shell())
}
