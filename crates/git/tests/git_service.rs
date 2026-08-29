use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

use cli_master_core::{ProjectId, SessionId};
use cli_master_git::{
    CreateWorktreeRequest, DiffOptions, ExistingBranchBehavior, GitError, GitService,
    InspectOptions, PrepareRemoveRequest, RemoveScope, RemoveWorktreeRequest, RepositoryKind, code,
};
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    repo: PathBuf,
    service: GitService,
    project_id: ProjectId,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().expect("temporary directory");
        let repo = temp.path().join("repo");
        fs::create_dir(&repo).expect("repo directory");
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.name", "CLI Master Test"]);
        git(&repo, &["config", "user.email", "test@cli-master.local"]);
        git(&repo, &["config", "commit.gpgsign", "false"]);
        let managed = temp.path().join("managed");
        let service = GitService::new(&managed).expect("git service");
        Self {
            _temp: temp,
            repo,
            service,
            project_id: ProjectId::new(),
        }
    }

    fn with_spaces() -> Self {
        let temp = TempDir::new().expect("temporary directory");
        let repo = temp.path().join("my repo");
        fs::create_dir(&repo).expect("repo directory");
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.name", "CLI Master Test"]);
        git(&repo, &["config", "user.email", "test@cli-master.local"]);
        git(&repo, &["config", "commit.gpgsign", "false"]);
        let managed = temp.path().join("managed trees");
        let service = GitService::new(&managed).expect("git service");
        Self {
            _temp: temp,
            repo,
            service,
            project_id: ProjectId::new(),
        }
    }

    fn commit_readme(&self) {
        fs::write(self.repo.join("README.md"), "hello\n").expect("readme");
        git(&self.repo, &["add", "README.md"]);
        git(&self.repo, &["commit", "--no-gpg-sign", "-m", "init"]);
    }

    fn create(&self, session_name: &str) -> cli_master_git::CreatedWorktree {
        self.service
            .create_worktree(CreateWorktreeRequest {
                repository: self.repo.clone(),
                session_name: session_name.to_owned(),
                session_id: SessionId::new(),
                project_id: self.project_id,
                base_ref: None,
                existing_branch: ExistingBranchBehavior::AllocateUnique,
            })
            .expect("worktree should be created")
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "CLI Master Test")
        .env("GIT_AUTHOR_EMAIL", "test@cli-master.local")
        .env("GIT_COMMITTER_NAME", "CLI Master Test")
        .env("GIT_COMMITTER_EMAIL", "test@cli-master.local")
        .output()
        .expect("git should spawn");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn error_code<T: std::fmt::Debug>(result: Result<T, GitError>) -> &'static str {
    result.expect_err("operation should fail").code()
}

#[test]
fn detects_valid_repository_and_subdirectory() {
    let fixture = Fixture::new();
    fixture.commit_readme();
    let nested = fixture.repo.join("src");
    fs::create_dir(&nested).expect("src");

    let root = fixture
        .service
        .detect_repository(&fixture.repo)
        .expect("root");
    assert_eq!(root.kind, RepositoryKind::Root);
    assert!(!root.unborn);

    let nested_info = fixture.service.detect_repository(&nested).expect("nested");
    assert_eq!(nested_info.kind, RepositoryKind::Subdirectory);
    assert_eq!(nested_info.root, root.root);
    assert_eq!(
        fixture
            .service
            .get_repository_root(&nested)
            .expect("toplevel"),
        root.root
    );
}

#[test]
fn rejects_missing_path_and_non_git_directory() {
    let fixture = Fixture::new();
    let missing = fixture.repo.join("does-not-exist");
    assert_eq!(
        error_code(fixture.service.detect_repository(&missing)),
        code::PATH_NOT_FOUND
    );

    let empty = fixture.repo.parent().expect("parent").join("not-git");
    fs::create_dir(&empty).expect("empty dir");
    assert_eq!(
        error_code(fixture.service.detect_repository(&empty)),
        code::NOT_A_REPOSITORY
    );
}

#[test]
fn reports_unborn_head_until_first_commit() {
    let fixture = Fixture::new();
    let info = fixture
        .service
        .detect_repository(&fixture.repo)
        .expect("unborn repo is still a repo");
    assert!(info.unborn);
    assert_eq!(
        error_code(fixture.service.create_worktree(CreateWorktreeRequest {
            repository: fixture.repo.clone(),
            session_name: "First session".to_owned(),
            session_id: SessionId::new(),
            project_id: fixture.project_id,
            base_ref: None,
            existing_branch: ExistingBranchBehavior::AllocateUnique,
        })),
        code::UNBORN_HEAD
    );

    fixture.commit_readme();
    let after = fixture
        .service
        .current_branch(&fixture.repo)
        .expect("branch");
    match after {
        cli_master_git::BranchState::Branch { name } => assert_eq!(name, "main"),
        other => panic!("expected branch after first commit, got {other:?}"),
    }
}

#[test]
fn creates_isolated_worktree_and_detects_it() {
    let fixture = Fixture::new();
    fixture.commit_readme();
    let created = fixture.create("Implement OAuth Refresh");
    assert_eq!(created.branch, "agent/implement-oauth-refresh");
    assert!(
        created
            .path
            .starts_with(fixture.service.managed_worktree_root())
    );
    assert!(created.path.is_dir());

    let listed = fixture.service.list_worktrees(&fixture.repo).expect("list");
    assert!(
        listed
            .iter()
            .any(|worktree| worktree.branch.as_deref() == Some("agent/implement-oauth-refresh"))
    );

    let inspected = fixture
        .service
        .inspect_worktree(
            &created.path,
            InspectOptions {
                include_dirty: true,
            },
        )
        .expect("inspect");
    assert_eq!(
        inspected.branch.as_deref(),
        Some("agent/implement-oauth-refresh")
    );
    assert_eq!(inspected.dirty, Some(false));
    assert!(!inspected.is_primary);
    assert_eq!(
        fixture
            .service
            .detect_repository(&created.path)
            .expect("linked")
            .kind,
        RepositoryKind::Worktree
    );
}

#[test]
fn two_sessions_do_not_share_a_branch() {
    let fixture = Fixture::new();
    fixture.commit_readme();
    let first = fixture.create("Implement OAuth Refresh");
    let second = fixture.create("Implement OAuth Refresh");
    assert_ne!(first.branch, second.branch);
    assert_ne!(first.path, second.path);
    assert!(second.branch.starts_with("agent/implement-oauth-refresh-"));
}

#[test]
fn reject_behavior_fails_on_existing_branch() {
    let fixture = Fixture::new();
    fixture.commit_readme();
    let _first = fixture.create("Implement OAuth Refresh");
    let error = fixture
        .service
        .create_worktree(CreateWorktreeRequest {
            repository: fixture.repo.clone(),
            session_name: "Implement OAuth Refresh".to_owned(),
            session_id: SessionId::new(),
            project_id: fixture.project_id,
            base_ref: None,
            existing_branch: ExistingBranchBehavior::Reject,
        })
        .expect_err("reject should fail");
    assert_eq!(error.code(), code::BRANCH_EXISTS);
}

#[test]
fn reports_clean_and_dirty_status_and_diff() {
    let fixture = Fixture::new();
    fixture.commit_readme();
    let created = fixture.create("Status session");
    let clean = fixture.service.status(&created.path).expect("clean status");
    assert!(!clean.is_dirty());
    assert_eq!(clean.branch.as_deref(), Some("agent/status-session"));

    fs::write(created.path.join("README.md"), "changed\n").expect("edit");
    fs::write(created.path.join("new.txt"), "added\n").expect("untracked");
    let dirty = fixture.service.status(&created.path).expect("dirty status");
    assert!(dirty.is_dirty());
    assert!(dirty.unstaged.iter().any(|path| path == "README.md"));
    assert!(dirty.untracked.iter().any(|path| path == "new.txt"));

    let diff = fixture
        .service
        .diff(&created.path, DiffOptions::unstaged())
        .expect("diff");
    assert!(diff.patch.contains("changed") || diff.patch.contains("README.md"));
    assert!(!diff.truncated);
    assert!(!diff.invalid_output);
}

#[test]
fn protects_binary_files_and_truncates_large_diffs() {
    let fixture = Fixture::new();
    fixture.commit_readme();
    fs::write(fixture.repo.join("blob.bin"), [0_u8, 1, 2, 0, 3]).expect("binary");
    git(&fixture.repo, &["add", "blob.bin"]);
    git(&fixture.repo, &["commit", "--no-gpg-sign", "-m", "binary"]);
    fs::write(fixture.repo.join("blob.bin"), [0_u8, 9, 9, 0, 9]).expect("binary change");

    let diff = fixture
        .service
        .diff(&fixture.repo, DiffOptions::unstaged())
        .expect("binary diff");
    assert!(
        diff.binary_paths.iter().any(|path| path == "blob.bin"),
        "binary paths: {:?}",
        diff.binary_paths
    );
    assert!(!diff.patch.contains('\0'));

    fs::write(fixture.repo.join("large.txt"), "x".repeat(8_192)).expect("large file");
    git(&fixture.repo, &["add", "large.txt"]);
    git(&fixture.repo, &["commit", "--no-gpg-sign", "-m", "large"]);
    fs::write(fixture.repo.join("large.txt"), "y".repeat(8_192)).expect("large change");
    let truncated = fixture
        .service
        .diff(
            &fixture.repo,
            DiffOptions {
                scope: cli_master_git::DiffScope::Unstaged,
                byte_limit: 64,
            },
        )
        .expect("truncated diff");
    assert!(truncated.truncated);
    assert!(truncated.patch.len() <= 64);
}

#[test]
fn refuses_to_remove_a_dirty_worktree() {
    let fixture = Fixture::new();
    fixture.commit_readme();
    let created = fixture.create("Dirty remove");
    fs::write(created.path.join("scratch.txt"), "nope\n").expect("dirty file");

    let prepared = fixture
        .service
        .prepare_remove_worktree(PrepareRemoveRequest {
            path: created.path.clone(),
            session_is_active: false,
            scope: RemoveScope::Directory,
        })
        .expect("prepare should report findings");
    assert!(prepared.dirty);
    assert!(
        prepared
            .blockers
            .contains(&cli_master_git::RemoveBlocker::Dirty)
    );
    assert!(prepared.confirmation_token.is_none());
    assert!(created.path.exists());

    let error = fixture
        .service
        .remove_worktree(RemoveWorktreeRequest {
            path: created.path.clone(),
            confirmation_token: String::new(),
            session_is_active: false,
            scope: RemoveScope::Directory,
        })
        .expect_err("empty token must fail");
    assert_eq!(error.code(), code::CONFIRMATION_REQUIRED);
    assert!(created.path.exists());
}

#[test]
fn removes_a_clean_worktree_after_explicit_confirmation() {
    let fixture = Fixture::new();
    fixture.commit_readme();
    let created = fixture.create("Clean remove");
    let prepared = fixture
        .service
        .prepare_remove_worktree(PrepareRemoveRequest {
            path: created.path.clone(),
            session_is_active: false,
            scope: RemoveScope::Directory,
        })
        .expect("prepare");
    let token = prepared
        .confirmation_token
        .expect("clean worktree should receive a token");
    fixture
        .service
        .remove_worktree(RemoveWorktreeRequest {
            path: created.path.clone(),
            confirmation_token: token,
            session_is_active: false,
            scope: RemoveScope::Directory,
        })
        .expect("remove");
    assert!(!created.path.exists());
}

#[test]
fn metadata_only_removal_leaves_the_directory() {
    let fixture = Fixture::new();
    fixture.commit_readme();
    let created = fixture.create("Metadata only");
    let prepared = fixture
        .service
        .prepare_remove_worktree(PrepareRemoveRequest {
            path: created.path.clone(),
            session_is_active: false,
            scope: RemoveScope::MetadataOnly,
        })
        .expect("prepare");
    let token = prepared.confirmation_token.expect("token");
    fixture
        .service
        .remove_worktree(RemoveWorktreeRequest {
            path: created.path.clone(),
            confirmation_token: token,
            session_is_active: false,
            scope: RemoveScope::MetadataOnly,
        })
        .expect("metadata remove");
    assert!(created.path.exists());
}

#[test]
fn refuses_primary_worktree_and_active_session() {
    let fixture = Fixture::new();
    fixture.commit_readme();
    let primary = fixture
        .service
        .prepare_remove_worktree(PrepareRemoveRequest {
            path: fixture.repo.clone(),
            session_is_active: false,
            scope: RemoveScope::Directory,
        })
        .expect("prepare primary");
    assert!(
        primary
            .blockers
            .contains(&cli_master_git::RemoveBlocker::PrimaryWorktree)
            || primary
                .blockers
                .contains(&cli_master_git::RemoveBlocker::OutsideManagedRoot)
    );
    assert!(primary.confirmation_token.is_none());

    let created = fixture.create("In use");
    let in_use = fixture
        .service
        .prepare_remove_worktree(PrepareRemoveRequest {
            path: created.path,
            session_is_active: true,
            scope: RemoveScope::Directory,
        })
        .expect("prepare in use");
    assert!(
        in_use
            .blockers
            .contains(&cli_master_git::RemoveBlocker::SessionActive)
    );
    assert!(in_use.confirmation_token.is_none());
}

#[test]
fn rolls_back_when_worktree_add_cannot_create_the_directory() {
    let fixture = Fixture::new();
    fixture.commit_readme();
    let project_dir = fixture
        .service
        .managed_worktree_root()
        .join(fixture.project_id.to_string());
    fs::create_dir_all(&project_dir).expect("project dir");
    let mut permissions = fs::metadata(&project_dir).expect("metadata").permissions();
    permissions.set_mode(0o555);
    fs::set_permissions(&project_dir, permissions).expect("lock project dir");

    let error = fixture
        .service
        .create_worktree(CreateWorktreeRequest {
            repository: fixture.repo.clone(),
            session_name: "Implement OAuth Refresh".to_owned(),
            session_id: SessionId::new(),
            project_id: fixture.project_id,
            base_ref: None,
            existing_branch: ExistingBranchBehavior::AllocateUnique,
        })
        .expect_err("read-only directory should block create");
    assert_eq!(error.code(), code::COMMAND_FAILED);

    let mut permissions = fs::metadata(&project_dir).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&project_dir, permissions).expect("unlock project dir");

    let listed = fixture.service.list_worktrees(&fixture.repo).expect("list");
    assert!(
        listed
            .iter()
            .all(|worktree| worktree.branch.as_deref() != Some("agent/implement-oauth-refresh"))
    );
}

#[test]
fn supports_paths_with_spaces_and_unicode_session_names() {
    let fixture = Fixture::with_spaces();
    fixture.commit_readme();
    let created = fixture.create("Implementação OAuth");
    assert_eq!(created.branch, "agent/implementacao-oauth");
    assert!(created.path.exists());
    let status = fixture.service.status(&created.path).expect("status");
    assert!(!status.is_dirty());
}

#[test]
fn supports_detached_head_as_a_base() {
    let fixture = Fixture::new();
    fixture.commit_readme();
    git(&fixture.repo, &["checkout", "--detach", "HEAD"]);
    let branch = fixture
        .service
        .current_branch(&fixture.repo)
        .expect("detached");
    assert!(matches!(
        branch,
        cli_master_git::BranchState::Detached { .. }
    ));
    let created = fixture
        .service
        .create_worktree(CreateWorktreeRequest {
            repository: fixture.repo.clone(),
            session_name: "From detached".to_owned(),
            session_id: SessionId::new(),
            project_id: fixture.project_id,
            base_ref: Some("HEAD".to_owned()),
            existing_branch: ExistingBranchBehavior::AllocateUnique,
        })
        .expect("worktree from detached HEAD");
    assert_eq!(created.branch, "agent/from-detached");
}

#[test]
fn does_not_run_git_through_a_shell() {
    let fixture = Fixture::new();
    fixture.commit_readme();
    let sneaky = fixture.repo.join("name; true");
    assert_eq!(
        error_code(fixture.service.detect_repository(&sneaky)),
        code::PATH_NOT_FOUND
    );
}

#[test]
fn errors_include_recovery_actions() {
    let fixture = Fixture::new();
    let error = fixture
        .service
        .detect_repository(fixture.repo.join("missing"))
        .expect_err("missing");
    let api = error.to_api_error();
    assert_eq!(api.code, code::PATH_NOT_FOUND);
    assert!(api.action.is_some());
    assert!(api.details.contains_key("path"));
}
