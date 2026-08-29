mod support;

use std::{cell::Cell, ffi::OsStr, fs, path::PathBuf};

use cli_master_git::{Git, GitErrorKind, RemovalBlocker, WorktreeUse};
use support::{RepositoryFixture, command, removal_snapshot};

#[test]
fn running_worktree_is_never_removed() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Running Worktree",
            "feedface",
        )
        .unwrap();
    let usage = WorktreeUse {
        running: true,
        in_use: true,
    };
    let preparation = git
        .prepare_remove(&fixture.repository, &fixture.managed, &created.path, usage)
        .unwrap();
    assert!(!preparation.can_remove);
    let error = git
        .remove_worktree(&fixture.repository, &fixture.managed, &created.path, || {
            usage
        })
        .unwrap_err();
    assert_eq!(error.kind(), GitErrorKind::WorktreeInUse);
}

#[test]
fn removal_rereads_usage_between_safety_snapshots() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Usage Transition",
            "u5a9e001",
        )
        .unwrap();
    let reads = Cell::new(0_u8);

    let error = git
        .remove_worktree(&fixture.repository, &fixture.managed, &created.path, || {
            let current = reads.get();
            reads.set(current + 1);
            WorktreeUse {
                running: current > 0,
                in_use: false,
            }
        })
        .expect_err("new runtime use between snapshots must block removal");

    assert_eq!(reads.get(), 2);
    assert_eq!(error.kind(), GitErrorKind::WorktreeInUse);
    assert!(created.path.exists());
}

#[test]
fn locked_worktree_is_never_removable() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Locked Worktree",
            "10c4ed0",
        )
        .unwrap();
    command(
        &fixture.repository,
        [
            OsStr::new("worktree"),
            OsStr::new("lock"),
            created.path.as_os_str(),
        ],
    );

    let preparation = removal_snapshot(&git, &fixture, &created.path);
    assert!(preparation.blockers.contains(&RemovalBlocker::Locked));
    let error = git
        .remove_worktree(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default,
        )
        .unwrap_err();
    assert_eq!(error.kind(), GitErrorKind::WorktreeInUse);
    assert!(created.path.exists());
}

#[test]
fn removal_snapshot_tracks_identity_content_and_protection_changes() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Snapshot Binding",
            "5a0b1d00",
        )
        .unwrap();
    let snapshot = removal_snapshot(&git, &fixture, &created.path);
    assert_eq!(
        snapshot.repository_root,
        fixture.repository.canonicalize().unwrap()
    );
    assert_eq!(
        snapshot.managed_root,
        fixture.managed.canonicalize().unwrap()
    );
    assert_eq!(snapshot.worktree, created);
    assert!(snapshot.can_remove);

    fs::write(created.path.join("tracked.txt"), "dirty snapshot\n").unwrap();
    let dirty = removal_snapshot(&git, &fixture, &created.path);
    assert_ne!(snapshot, dirty);
    assert!(dirty.blockers.contains(&RemovalBlocker::TrackedChanges));
    command(&created.path, ["checkout", "--", "tracked.txt"]);

    command(
        &created.path,
        ["update-index", "--assume-unchanged", "tracked.txt"],
    );
    let protected = removal_snapshot(&git, &fixture, &created.path);
    assert!(
        protected
            .blockers
            .contains(&RemovalBlocker::AssumeUnchanged)
    );
    command(
        &created.path,
        ["update-index", "--no-assume-unchanged", "tracked.txt"],
    );

    fs::write(created.path.join("tracked.txt"), "committed snapshot\n").unwrap();
    command(&created.path, ["add", "tracked.txt"]);
    command(&created.path, ["commit", "-m", "advance snapshot head"]);
    let advanced_head = removal_snapshot(&git, &fixture, &created.path);
    assert_ne!(snapshot.worktree.head, advanced_head.worktree.head);

    command(&created.path, ["switch", "-c", "snapshot-alternate"]);
    let alternate_branch = removal_snapshot(&git, &fixture, &created.path);
    assert_eq!(
        alternate_branch.worktree.branch.as_deref(),
        Some("snapshot-alternate")
    );

    command(
        &fixture.repository,
        [
            OsStr::new("worktree"),
            OsStr::new("lock"),
            created.path.as_os_str(),
        ],
    );
    let locked = removal_snapshot(&git, &fixture, &created.path);
    assert_ne!(alternate_branch, locked);
    assert!(locked.worktree.locked);
}

#[test]
fn staged_untracked_and_in_use_are_independent_blockers() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let staged = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Staged Only",
            "57a6ed0",
        )
        .unwrap();
    fs::write(staged.path.join("staged.txt"), "staged\n").unwrap();
    command(&staged.path, ["add", "staged.txt"]);
    assert_eq!(
        removal_snapshot(&git, &fixture, &staged.path).blockers,
        [RemovalBlocker::StagedChanges]
    );

    let untracked = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "Untracked Only",
            "a17ac0d",
        )
        .unwrap();
    fs::write(untracked.path.join("untracked.txt"), "untracked\n").unwrap();
    assert_eq!(
        removal_snapshot(&git, &fixture, &untracked.path).blockers,
        [RemovalBlocker::UntrackedFiles]
    );

    let in_use = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "In Use Only",
            "10a5e00",
        )
        .unwrap();
    let preparation = git
        .prepare_remove(
            &fixture.repository,
            &fixture.managed,
            &in_use.path,
            WorktreeUse {
                running: false,
                in_use: true,
            },
        )
        .unwrap();
    assert_eq!(preparation.blockers, [RemovalBlocker::InUse]);
    assert_eq!(preparation.assume_unchanged_paths, Vec::<PathBuf>::new());
}
