mod support;

use std::{fs, path::Path};

use cli_master_git::{Git, GitErrorKind};
use support::{RepositoryFixture, branch_exists, command, configure_identity};
#[cfg(target_os = "linux")]
use tempfile::TempDir;

#[test]
fn worktree_plan_has_no_filesystem_or_git_side_effects() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    assert!(!fixture.managed.exists());

    let plan = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Pure Planning",
            "a11ce001",
        )
        .expect("worktree should be planned");

    let canonical_temp = fixture.temp.path().canonicalize().unwrap();
    assert_eq!(
        plan.repository_root(),
        fixture.repository.canonicalize().unwrap()
    );
    assert_eq!(
        plan.git_common_dir(),
        fixture.repository.join(".git").canonicalize().unwrap()
    );
    assert!(matches!(plan.initial_oid().len(), 40 | 64));
    assert_eq!(plan.managed_root(), canonical_temp.join("managed"));
    assert_eq!(plan.destination().parent(), Some(plan.managed_root()));
    assert!(plan.branch().starts_with("agent/pure-planning-a11ce001"));
    assert!(!fixture.managed.exists());
    assert!(!plan.destination().exists());
    assert!(!branch_exists(&fixture.repository, plan.branch()));
    assert_eq!(git.list_worktrees(&fixture.repository).unwrap().len(), 1);
}

#[test]
fn creates_an_exact_precomputed_worktree_plan() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let plan = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Durable Saga",
            "d00ab1e5",
        )
        .expect("worktree should be planned");

    let created = git
        .create_worktree_from_plan(&plan)
        .expect("planned worktree should be created");

    assert_eq!(created.path, plan.destination());
    assert_eq!(created.branch.as_deref(), Some(plan.branch()));
    assert_eq!(created.head.as_deref(), Some(plan.initial_oid()));
    assert!(branch_exists(&fixture.repository, plan.branch()));
}

#[test]
fn plan_creates_from_its_oid_even_when_repository_head_advances() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let plan = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Pinned Base",
            "01dba5e0",
        )
        .expect("worktree should be planned");
    fs::write(fixture.repository.join("tracked.txt"), "advanced\n").unwrap();
    command(&fixture.repository, ["add", "tracked.txt"]);
    command(&fixture.repository, ["commit", "-m", "advance main"]);

    let created = git
        .create_worktree_from_plan(&plan)
        .expect("OID-bound plan should remain deterministic");
    let current_head = support::command_line(&fixture.repository, ["rev-parse", "HEAD"]);

    assert_eq!(created.head.as_deref(), Some(plan.initial_oid()));
    assert_ne!(created.head.as_deref(), Some(current_head.as_str()));
}

#[test]
fn plan_is_rejected_when_its_initial_commit_is_no_longer_available() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let plan = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Pruned Base",
            "57a1e000",
        )
        .unwrap();
    let tree = support::command_line(&fixture.repository, ["rev-parse", "HEAD^{tree}"]);
    let replacement = support::command_line(
        &fixture.repository,
        ["commit-tree", tree.as_str(), "-m", "replacement root"],
    );
    command(
        &fixture.repository,
        ["update-ref", "refs/heads/main", replacement.as_str()],
    );
    command(
        &fixture.repository,
        ["reflog", "expire", "--expire=now", "--all"],
    );
    command(&fixture.repository, ["gc", "--prune=now"]);

    let error = git.create_worktree_from_plan(&plan).unwrap_err();

    assert_eq!(error.kind(), GitErrorKind::InvalidInput);
    assert!(error.message().contains("no longer available"));
    assert!(!plan.destination().exists());
    assert!(!branch_exists(&fixture.repository, plan.branch()));
}

#[test]
fn recreated_repository_at_same_path_invalidates_plan_identity() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let plan = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Recreated Repository",
            "1de0717a",
        )
        .expect("worktree should be planned");
    let displaced = fixture.temp.path().join("original-repository");
    fs::rename(&fixture.repository, &displaced).expect("original repository should move");
    fs::create_dir(&fixture.repository).expect("replacement repository should be created");
    command(&fixture.repository, ["init", "-b", "main"]);
    configure_identity(&fixture.repository);
    fs::write(fixture.repository.join("replacement.txt"), "replacement\n").unwrap();
    command(&fixture.repository, ["add", "."]);
    command(&fixture.repository, ["commit", "-m", "replacement"]);

    let error = git
        .create_worktree_from_plan(&plan)
        .expect_err("recreated repository must invalidate physical identity");

    assert_eq!(error.kind(), GitErrorKind::UnsafePath);
    assert!(error.message().contains("common directory identity"));
    assert!(!plan.destination().exists());
    assert!(!branch_exists(&fixture.repository, plan.branch()));
}

#[cfg(target_os = "linux")]
#[test]
fn plan_and_create_preserve_non_utf8_repository_paths() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let temp = TempDir::new().unwrap();
    let repository = temp
        .path()
        .join(OsString::from_vec(b"repository-\xff".to_vec()));
    let managed = temp
        .path()
        .join(OsString::from_vec(b"managed-\xfe".to_vec()));
    fs::create_dir(&repository).unwrap();
    command(&repository, ["init", "-b", "main"]);
    configure_identity(&repository);
    fs::write(repository.join("tracked.txt"), "initial\n").unwrap();
    command(&repository, ["add", "."]);
    command(&repository, ["commit", "-m", "initial"]);
    let git = Git::discover().unwrap();

    let plan = git
        .plan_worktree(&repository, &managed, "Non UTF8", "b17e5000")
        .expect("non-UTF-8 paths should be planned losslessly");
    let created = git
        .create_worktree_from_plan(&plan)
        .expect("non-UTF-8 worktree root should be created");

    assert_eq!(plan.repository_root(), repository.canonicalize().unwrap());
    assert_eq!(created.path, plan.destination());
    assert!(created.path.starts_with(managed.canonicalize().unwrap()));
}

#[test]
fn stale_plan_rejects_destination_and_branch_races() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().expect("Git should be discovered");
    let destination_race = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Destination Race",
            "c0111de0",
        )
        .unwrap();
    fs::create_dir_all(destination_race.destination()).unwrap();
    let destination_error = git
        .create_worktree_from_plan(&destination_race)
        .expect_err("occupied destination must be rejected");
    assert_eq!(destination_error.kind(), GitErrorKind::InvalidInput);

    let branch_race = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Branch Race",
            "b2a4c001",
        )
        .unwrap();
    command(&fixture.repository, ["branch", branch_race.branch()]);
    let branch_error = git
        .create_worktree_from_plan(&branch_race)
        .expect_err("occupied branch must be rejected");
    assert_eq!(branch_error.kind(), GitErrorKind::InvalidInput);
    assert!(!branch_race.destination().exists());
}

#[cfg(unix)]
#[test]
fn stale_plan_rejects_managed_root_symlink_retargeting() {
    use std::os::unix::fs::symlink;

    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let plan = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Symlink Race",
            "5afe1d00",
        )
        .unwrap();
    let outside = fixture.temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    symlink(&outside, &fixture.managed).unwrap();

    let error = git.create_worktree_from_plan(&plan).unwrap_err();

    assert_eq!(error.kind(), GitErrorKind::UnsafePath);
    assert!(
        !outside
            .join(plan.destination().file_name().unwrap())
            .exists()
    );
    assert!(!branch_exists(&fixture.repository, plan.branch()));
}

#[cfg(unix)]
#[test]
fn stale_plan_rejects_a_dangling_destination_symlink() {
    use std::os::unix::fs::symlink;

    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    let plan = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Dangling Destination",
            "da091e00",
        )
        .unwrap();
    fs::create_dir_all(plan.managed_root()).unwrap();
    let dangling_target = fixture.temp.path().join("does-not-exist");
    symlink(&dangling_target, plan.destination()).unwrap();

    let error = git.create_worktree_from_plan(&plan).unwrap_err();

    assert_eq!(error.kind(), GitErrorKind::InvalidInput);
    assert!(fs::symlink_metadata(plan.destination()).is_ok());
    assert!(!dangling_target.exists());
}

#[cfg(target_os = "macos")]
#[test]
fn macos_var_alias_is_canonicalized_before_prefix_checks() {
    let fixture = RepositoryFixture::new();
    let canonical_temp = fixture.temp.path().canonicalize().unwrap();
    let Ok(relative) = canonical_temp.strip_prefix("/private/var") else {
        return;
    };
    let logical_temp = Path::new("/var").join(relative);
    let git = Git::discover().unwrap();
    let plan = git
        .plan_worktree(
            logical_temp.join("repository"),
            logical_temp.join("managed-through-var"),
            "macOS Var Prefix",
            "a11a5000",
        )
        .unwrap();

    assert!(plan.repository_root().starts_with("/private/var"));
    assert!(plan.managed_root().starts_with("/private/var"));
    let created = git.create_worktree_from_plan(&plan).unwrap();
    assert!(created.path.starts_with(plan.managed_root()));
}
