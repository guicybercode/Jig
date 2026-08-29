use std::fs;

use tempfile::TempDir;

use cli_master_git::{GitError, GitService, WorktreeCreate};

fn git() -> GitService {
    GitService::from_path_env().expect("system git should be installed")
}

fn fixture_repo() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().expect("temporary directory");
    let repo = temp.path().join("repo");
    git()
        .init_with_commit(&repo, "initial")
        .expect("repository should initialize");
    (temp, repo)
}

#[test]
fn discovers_canonical_repository_root() {
    let (_temp, repo) = fixture_repo();
    let nested = repo.join("src");
    fs::create_dir_all(&nested).expect("nested dir");
    let root = git()
        .discover_repository(&nested)
        .expect("nested path should resolve");
    assert_eq!(
        root.canonicalize().expect("root"),
        repo.canonicalize().expect("repo")
    );
}

#[test]
fn status_detects_clean_and_dirty_worktrees() {
    let (_temp, repo) = fixture_repo();
    let status = git().status(&repo).expect("status");
    let branch = status.branch.as_deref();
    assert!(
        branch == Some("master") || branch == Some("main"),
        "unexpected branch {branch:?}"
    );
    assert!(!status.is_dirty);

    fs::write(repo.join("dirty.txt"), "change").expect("dirty file");
    let dirty = git().status(&repo).expect("dirty status");
    assert!(dirty.is_dirty);
    assert!(dirty.changed_paths >= 1);
}

#[test]
fn create_list_and_remove_clean_worktree() {
    let (temp, repo) = fixture_repo();
    let managed = temp.path().join("managed");
    let created = git()
        .create_worktree(&WorktreeCreate {
            repository: &repo,
            managed_root: &managed,
            project_id: "project-1",
            task_label: "implement authentication",
            branch: Some("agent/implement-authentication-test".to_owned()),
            path: Some(managed.join("worktrees/project-1/implement-authentication-test")),
        })
        .expect("worktree should be created");

    let listed = git().list_worktrees(&repo).expect("worktrees should list");
    assert!(listed.iter().any(|info| info.path == created.path));
    assert!(listed.iter().any(|info| info.is_primary));

    let plan = git()
        .prepare_remove(&repo, &created.path)
        .expect("prepare should succeed");
    assert!(!plan.is_dirty);
    git()
        .remove_worktree(&plan, false)
        .expect("clean worktree should remove");
    assert!(!created.path.exists());
}

#[test]
fn refuses_dirty_worktree_removal_until_state_is_clean() {
    let (temp, repo) = fixture_repo();
    let managed = temp.path().join("managed");
    let created = git()
        .create_worktree(&WorktreeCreate {
            repository: &repo,
            managed_root: &managed,
            project_id: "project-1",
            task_label: "dirty check",
            branch: Some("agent/dirty-check".to_owned()),
            path: Some(managed.join("wt-dirty")),
        })
        .expect("worktree should be created");

    fs::write(created.path.join("uncommitted.txt"), "nope").expect("dirty file");
    let plan = git()
        .prepare_remove(&repo, &created.path)
        .expect("prepare should observe dirty state");
    assert!(plan.is_dirty);

    let error = git()
        .remove_worktree(&plan, false)
        .expect_err("dirty removal must be refused");
    assert!(matches!(error, GitError::DirtyWorktree { .. }));
    assert!(created.path.exists());

    fs::remove_file(created.path.join("uncommitted.txt")).expect("restore clean tree");
    let clean = git()
        .prepare_remove(&repo, &created.path)
        .expect("prepare after cleanup");
    assert!(!clean.is_dirty);
    git()
        .remove_worktree(&clean, false)
        .expect("clean worktree should remove after protection was proven");
}

#[test]
fn rejects_worktree_path_outside_managed_root() {
    let (temp, repo) = fixture_repo();
    let managed = temp.path().join("managed");
    let outside = temp.path().join("outside-wt");
    let error = git()
        .create_worktree(&WorktreeCreate {
            repository: &repo,
            managed_root: &managed,
            project_id: "project-1",
            task_label: "escape",
            branch: Some("agent/escape".to_owned()),
            path: Some(outside),
        })
        .expect_err("escaped path must be rejected");
    assert!(matches!(error, GitError::PathOutsideManagedRoot { .. }));
}

#[test]
fn branch_suggestions_use_slug_and_unique_suffix() {
    let first = GitService::suggest_branch("Implement Authentication");
    let second = GitService::suggest_branch("Implement Authentication");
    assert!(first.starts_with("agent/implement-authentication-"));
    assert!(second.starts_with("agent/implement-authentication-"));
    assert_ne!(first, second);
}

#[test]
fn missing_git_is_reported() {
    let error = GitService::new("").expect_err("empty executable");
    assert!(matches!(error, GitError::GitNotFound));
}
