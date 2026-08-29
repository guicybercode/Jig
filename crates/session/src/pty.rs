use cli_master_core::{CommandSpec, redact_text};

use crate::error::SessionError;

/// Requested PTY grid size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PtySize {
    /// Number of columns.
    pub cols: u16,
    /// Number of rows.
    pub rows: u16,
}

impl PtySize {
    /// Creates a size, rejecting a zero dimension.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::InvalidSize`] when either dimension is zero.
    pub fn new(cols: u16, rows: u16) -> Result<Self, SessionError> {
        if cols == 0 || rows == 0 {
            Err(SessionError::InvalidSize)
        } else {
            Ok(Self { cols, rows })
        }
    }
}

pub(crate) struct SpawnedPty {
    pub pid: Option<u32>,
    pub pgid: Option<i32>,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    pub master: Box<dyn portable_pty::MasterPty + Send>,
    pub reader: Box<dyn std::io::Read + Send>,
    pub writer: Box<dyn std::io::Write + Send>,
}

pub(crate) trait PtyBackend: Send + Sync {
    fn spawn(&self, spec: &CommandSpec, size: PtySize) -> Result<SpawnedPty, SessionError>;
}

pub(crate) struct NativePtyBackend;

impl PtyBackend for NativePtyBackend {
    fn spawn(&self, spec: &CommandSpec, size: PtySize) -> Result<SpawnedPty, SessionError> {
        spawn_native(spec, size)
    }
}

fn spawn_native(spec: &CommandSpec, size: PtySize) -> Result<SpawnedPty, SessionError> {
    if !spec.cwd().is_dir() {
        return Err(SessionError::InvalidWorkingDirectory(spec.cwd().clone()));
    }

    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system
        .openpty(portable_pty::PtySize {
            rows: size.rows,
            cols: size.cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| SessionError::Pty(error.to_string()))?;

    let command = build_command(spec);

    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| SessionError::Spawn(redact_spawn_error(&error.to_string())))?;
    drop(pair.slave);

    let child_pid = child.process_id();
    let group_id = pair
        .master
        .process_group_leader()
        .or_else(|| child_pid.and_then(|value| i32::try_from(value).ok()));
    let group_id = crate::unix::sanitize_pgid(group_id);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| SessionError::Pty(error.to_string()))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| SessionError::Pty(error.to_string()))?;

    Ok(SpawnedPty {
        pid: child_pid,
        pgid: group_id,
        child,
        master: pair.master,
        reader,
        writer,
    })
}

fn build_command(spec: &CommandSpec) -> portable_pty::CommandBuilder {
    let mut command = portable_pty::CommandBuilder::new(spec.executable());
    for argument in spec.args() {
        command.arg(argument);
    }
    command.cwd(spec.cwd());
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    for (key, value) in spec.env() {
        command.env(key, value);
    }
    for key in spec.env_removals() {
        command.env_remove(key);
    }
    command
}

fn redact_spawn_error(message: &str) -> String {
    const KEEP: usize = 180;
    let bounded = if message.len() <= KEEP {
        message.to_owned()
    } else {
        let mut end = KEEP;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &message[..end])
    };
    redact_text(&bounded)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use cli_master_core::CommandSpec;

    use super::{build_command, redact_spawn_error};

    #[test]
    fn applies_environment_removals_after_additions() {
        let spec = CommandSpec::try_from_parts(
            "agent",
            Vec::<String>::new(),
            "/tmp/project",
            BTreeMap::from([(
                "CLI_MASTER_STALE_TOKEN".to_owned(),
                "must-not-reach-child".to_owned(),
            )]),
        )
        .expect("command fixture should be valid")
        .with_env_removals(["CLI_MASTER_STALE_TOKEN"])
        .expect("environment removal should be valid");

        let command = build_command(&spec);

        assert!(command.get_env("CLI_MASTER_STALE_TOKEN").is_none());
    }

    #[test]
    fn redacts_long_spawn_errors_at_a_utf8_boundary() {
        let message = format!("{}é{}", "a".repeat(179), "b".repeat(8));
        assert!(!message.is_char_boundary(180));

        let redacted = redact_spawn_error(&message);

        assert_eq!(redacted, format!("{}…", "a".repeat(179)));
    }

    #[test]
    fn preserves_spawn_errors_within_the_limit() {
        let message = "falha ao iniciar: caminho inválido";
        assert_eq!(redact_spawn_error(message), message);
    }

    #[test]
    fn redacts_secrets_from_spawn_errors() {
        let redacted = redact_spawn_error("could not start with TOKEN=spawn-secret");
        assert!(!redacted.contains("spawn-secret"));
        assert!(redacted.contains("[redacted]"));
    }
}
