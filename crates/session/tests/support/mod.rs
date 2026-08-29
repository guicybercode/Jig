#![allow(dead_code)]

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use cli_master_core::{AgentId, AgentSource, Project, ProjectId, wire::SessionIsolation};
use cli_master_git::Git;
use cli_master_session::{CreateSession, FakeSpawner, SessionWorktreeSaga};
use cli_master_storage::{Storage, StoredAgent};
use tempfile::TempDir;

pub const CREATED_AT_MS: i64 = 1_787_941_200_000;
pub const DAEMON_ID: &str = "daemon-saga-test";

pub struct Fixture {
    pub temp: TempDir,
    pub repository: PathBuf,
    pub managed: PathBuf,
    pub database: PathBuf,
    pub project_id: ProjectId,
    pub agent_id: AgentId,
}

impl Fixture {
    pub fn new() -> Self {
        let temp = TempDir::new().expect("temporary directory should be created");
        let repository = temp.path().join("repository");
        let managed = temp.path().join("managed");
        let database = temp.path().join("cli-master.db");
        fs::create_dir(&repository).expect("repository directory should be created");
        git(&repository, ["init", "-b", "main"]);
        git(
            &repository,
            ["config", "user.email", "tests@example.invalid"],
        );
        git(&repository, ["config", "user.name", "CLI Master Tests"]);
        fs::write(repository.join("tracked.txt"), "initial\n").expect("fixture file");
        git(&repository, ["add", "."]);
        git(&repository, ["commit", "-m", "initial"]);

        let project_id = ProjectId::new();
        let agent_id = AgentId::new();
        let mut storage = Storage::open(&database).expect("database should open");
        storage.migrate().expect("database should migrate");
        storage
            .insert_project(&Project {
                id: project_id,
                name: "Saga Project".to_owned(),
                path: repository.clone(),
                repository_root: None,
                current_branch: None,
                created_at_ms: CREATED_AT_MS,
                last_opened_at_ms: CREATED_AT_MS,
            })
            .expect("project should insert");
        storage
            .insert_agent(&StoredAgent {
                id: agent_id,
                source: AgentSource::BuiltIn,
                display_name: "Codex".to_owned(),
                executable: "true".to_owned(),
                args: Vec::new(),
                env: std::collections::BTreeMap::new(),
                enabled: true,
                created_at_ms: CREATED_AT_MS,
                updated_at_ms: CREATED_AT_MS,
            })
            .expect("agent should insert");
        drop(storage);

        Self {
            temp,
            repository,
            managed,
            database,
            project_id,
            agent_id,
        }
    }

    pub fn saga(&self, spawner: FakeSpawner) -> SessionWorktreeSaga<FakeSpawner> {
        let mut storage = Storage::open(&self.database).expect("database should reopen");
        storage.migrate().expect("database should migrate");
        SessionWorktreeSaga::new(
            Git::discover().expect("Git should be discovered"),
            storage,
            spawner,
            DAEMON_ID,
        )
        .expect("saga should construct")
    }

    pub fn request(&self, name: &str, short_id: Option<&str>) -> CreateSession {
        CreateSession {
            project_id: self.project_id,
            agent_id: self.agent_id,
            name: name.to_owned(),
            isolation: SessionIsolation::NewWorktree,
            managed_root: self.managed.clone(),
            short_id: short_id.map(str::to_owned),
        }
    }
}

pub fn git<I, S>(cwd: &Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("Git fixture command should start");
    assert!(
        output.status.success(),
        "Git fixture command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn branch_exists(repository: &Path, branch: &str) -> bool {
    Command::new("git")
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(repository)
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .expect("branch lookup should start")
        .success()
}
