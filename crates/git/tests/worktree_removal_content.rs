mod support;

use std::{
    fs,
    path::{Path, PathBuf},
};

use cli_master_git::{Git, GitErrorKind, RemovalBlocker, WorktreeUse};
use support::{RepositoryFixture, branch_exists, command};

#[test]
fn clean_remove_preserves_branch() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Implement Auth",
            "01234567",
        )
        .unwrap();
    let branch = created.branch.clone().unwrap();

    git.remove_worktree(
        &fixture.repository,
        &fixture.managed,
        &created.path,
        WorktreeUse::default,
    )
    .expect("clean worktree should be removed");

    assert!(!created.path.exists());
    assert!(branch_exists(&fixture.repository, &branch));
}

#[test]
fn dirty_tracked_and_untracked_worktree_is_never_removed() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Dirty Worktree",
            "deadbeef",
        )
        .unwrap();
    fs::write(created.path.join("tracked.txt"), "dirty\n").unwrap();
    fs::write(created.path.join("untracked.txt"), "dirty\n").unwrap();

    let preparation = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .unwrap();
    assert!(preparation.status.has_tracked_changes);
    assert!(preparation.status.has_untracked);
    let error = git
        .remove_worktree(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default,
        )
        .unwrap_err();
    assert_eq!(error.kind(), GitErrorKind::DirtyWorktree);
    assert!(created.path.exists());
}

#[test]
fn ignored_only_worktree_is_preserved() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Ignored Worktree",
            "19a0be5",
        )
        .unwrap();
    fs::write(
        fixture.repository.join(".git/info/exclude"),
        "ignored-output.log\n",
    )
    .unwrap();
    fs::write(created.path.join("ignored-output.log"), "valuable output\n").unwrap();

    let preparation = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .unwrap();
    assert_eq!(
        preparation.ignored_paths,
        [PathBuf::from("ignored-output.log")]
    );
    assert!(preparation.blockers.contains(&RemovalBlocker::IgnoredFiles));
    let error = git
        .remove_worktree(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default,
        )
        .unwrap_err();
    assert_eq!(error.kind(), GitErrorKind::DirtyWorktree);
    assert!(created.path.join("ignored-output.log").exists());
}

#[test]
fn assume_unchanged_modified_worktree_is_preserved() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Assume Unchanged",
            "a55a0e0",
        )
        .unwrap();
    command(
        &created.path,
        ["update-index", "--assume-unchanged", "tracked.txt"],
    );
    fs::write(created.path.join("tracked.txt"), "hidden modification\n").unwrap();

    let preparation = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .unwrap();
    assert_eq!(
        preparation.assume_unchanged_paths,
        [Path::new("tracked.txt")]
    );
    assert!(
        preparation
            .blockers
            .contains(&RemovalBlocker::AssumeUnchanged)
    );
    let error = git
        .remove_worktree(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default,
        )
        .unwrap_err();
    assert_eq!(error.kind(), GitErrorKind::DirtyWorktree);
    assert!(created.path.exists());
}

#[test]
fn skip_worktree_modified_worktree_is_preserved() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Skip Worktree",
            "5c1f1e5",
        )
        .unwrap();
    command(
        &created.path,
        ["update-index", "--skip-worktree", "tracked.txt"],
    );
    fs::write(created.path.join("tracked.txt"), "hidden modification\n").unwrap();

    let preparation = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .unwrap();
    assert_eq!(preparation.skip_worktree_paths, [Path::new("tracked.txt")]);
    assert!(preparation.blockers.contains(&RemovalBlocker::SkipWorktree));
    let error = git
        .remove_worktree(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default,
        )
        .unwrap_err();
    assert_eq!(error.kind(), GitErrorKind::DirtyWorktree);
    assert!(created.path.exists());
}

#[test]
fn removal_rejects_registered_worktree_outside_managed_root() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let other_root = fixture.temp.path().join("other-managed-root");
    let created = git
        .create_worktree(
            &fixture.repository,
            &other_root,
            "Outside Worktree",
            "cab005e",
        )
        .unwrap();
    fs::create_dir_all(&fixture.managed).unwrap();

    let error = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default(),
        )
        .unwrap_err();

    assert_eq!(error.kind(), GitErrorKind::UnsafePath);
    assert!(created.path.exists());
}
