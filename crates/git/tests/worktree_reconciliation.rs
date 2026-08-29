mod support;

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use cli_master_git::{Git, GitErrorKind, WorktreeUse};
use support::{
    RepositoryFixture, branch_exists, install_post_checkout_hook, shell_quote, write_executable,
};

#[cfg(unix)]
fn post_create_failure_git(fixture: &RepositoryFixture, fail_remove: bool) -> Git {
    let real_git = Git::discover().expect("real Git should be discovered");
    let suffix = if fail_remove { "orphan" } else { "rollback" };
    let wrapper = fixture.temp.path().join(format!("git-{suffix}"));
    let added = fixture.temp.path().join(format!("{suffix}-added"));
    let served = fixture.temp.path().join(format!("{suffix}-served"));
    let script = format!(
        "#!/bin/sh\n\
         REAL_GIT={}\n\
         ADDED={}\n\
         SERVED={}\n\
         FAIL_REMOVE={}\n\
         if [ \"$1\" = \"worktree\" ] && [ \"$2\" = \"add\" ]; then\n\
           \"$REAL_GIT\" \"$@\"\n\
           result=$?\n\
           if [ \"$result\" -eq 0 ]; then : > \"$ADDED\"; fi\n\
           exit \"$result\"\n\
         fi\n\
         if [ \"$1\" = \"worktree\" ] && [ \"$2\" = \"list\" ] && [ -f \"$ADDED\" ] && [ ! -f \"$SERVED\" ]; then\n\
           : > \"$SERVED\"\n\
           printf 'malformed\\0'\n\
           exit 0\n\
         fi\n\
         if [ \"$1\" = \"worktree\" ] && [ \"$2\" = \"remove\" ]; then\n\
           for argument in \"$@\"; do\n\
             if [ \"$argument\" = \"--force\" ]; then exit 97; fi\n\
           done\n\
         fi\n\
         if [ \"$FAIL_REMOVE\" = \"1\" ] && [ \"$1\" = \"worktree\" ] && [ \"$2\" = \"remove\" ]; then\n\
           echo 'injected conservative cleanup refusal' >&2\n\
           exit 73\n\
         fi\n\
         exec \"$REAL_GIT\" \"$@\"\n",
        shell_quote(real_git.executable()),
        shell_quote(&added),
        shell_quote(&served),
        if fail_remove { "1" } else { "0" },
    );
    write_executable(&wrapper, &script);
    Git::with_executable(wrapper).expect("Git wrapper should validate")
}

#[cfg(unix)]
fn removal_auditing_git(fixture: &RepositoryFixture) -> (Git, PathBuf, PathBuf) {
    let real_git = Git::discover().unwrap();
    let wrapper = fixture.temp.path().join("git-removal-audit");
    let remove_seen = fixture.temp.path().join("remove-seen");
    let force_seen = fixture.temp.path().join("force-seen");
    let script = format!(
        "#!/bin/sh\n\
         REAL_GIT={}\n\
         REMOVE_SEEN={}\n\
         FORCE_SEEN={}\n\
         if [ \"$1\" = \"worktree\" ] && [ \"$2\" = \"remove\" ]; then\n\
           : > \"$REMOVE_SEEN\"\n\
           for argument in \"$@\"; do\n\
             if [ \"$argument\" = \"--force\" ]; then\n\
               : > \"$FORCE_SEEN\"\n\
               exit 97\n\
             fi\n\
           done\n\
         fi\n\
         exec \"$REAL_GIT\" \"$@\"\n",
        shell_quote(real_git.executable()),
        shell_quote(&remove_seen),
        shell_quote(&force_seen),
    );
    write_executable(&wrapper, &script);
    (
        Git::with_executable(wrapper).unwrap(),
        remove_seen,
        force_seen,
    )
}

#[cfg(unix)]
fn pre_effect_failure_git(fixture: &RepositoryFixture) -> Git {
    let real_git = Git::discover().unwrap();
    let wrapper = fixture.temp.path().join("git-pre-effect-failure");
    let script = format!(
        "#!/bin/sh\n\
         REAL_GIT={}\n\
         if [ \"$1\" = \"worktree\" ] && [ \"$2\" = \"add\" ]; then\n\
           echo 'injected failure before side effects' >&2\n\
           exit 71\n\
         fi\n\
         exec \"$REAL_GIT\" \"$@\"\n",
        shell_quote(real_git.executable()),
    );
    write_executable(&wrapper, &script);
    Git::with_executable(wrapper).unwrap()
}

#[cfg(unix)]
#[test]
fn post_create_confirmation_failure_is_compensated() {
    let fixture = RepositoryFixture::new();
    let git = post_create_failure_git(&fixture, false);
    let plan = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Compensated Creation",
            "c0a0e001",
        )
        .unwrap();

    let error = git.create_worktree_from_plan(&plan).unwrap_err();

    assert_eq!(error.kind(), GitErrorKind::InvalidOutput);
    assert!(error.message().contains("safely rolled back"));
    assert!(!plan.destination().exists());
    assert!(branch_exists(&fixture.repository, plan.branch()));
}

#[cfg(unix)]
#[test]
fn nonzero_add_with_proven_absence_returns_original_error() {
    let fixture = RepositoryFixture::new();
    let git = pre_effect_failure_git(&fixture);
    let plan = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Absent Failure",
            "ab5e1700",
        )
        .unwrap();

    let error = git.create_worktree_from_plan(&plan).unwrap_err();

    assert_eq!(error.kind(), GitErrorKind::CommandFailed);
    assert!(
        error
            .message()
            .contains("injected failure before side effects")
    );
    assert!(!error.message().contains("rolled back"));
    assert!(!plan.destination().exists());
    assert!(!branch_exists(&fixture.repository, plan.branch()));
    assert_no_worktree_registration(&git, &fixture, plan.destination());
}

#[cfg(unix)]
#[test]
fn unproven_compensation_preserves_partial_path_and_reports_orphan() {
    let fixture = RepositoryFixture::new();
    let git = post_create_failure_git(&fixture, true);
    let plan = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Preserved Orphan",
            "0a9a0001",
        )
        .unwrap();

    let error = git.create_worktree_from_plan(&plan).unwrap_err();

    assert_eq!(error.kind(), GitErrorKind::PartialWorktree);
    assert_eq!(error.path(), Some(plan.destination()));
    assert!(error.message().contains("cleanup could not be proven"));
    assert!(plan.destination().is_dir());
    assert!(branch_exists(&fixture.repository, plan.branch()));
}

#[cfg(unix)]
#[test]
fn nonzero_post_checkout_after_effect_is_reconciled_and_compensated() {
    let fixture = RepositoryFixture::new();
    install_post_checkout_hook(&fixture, "exit 37");
    let git = Git::discover().unwrap();
    let plan = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Hook Failure",
            "e017c0de",
        )
        .unwrap();

    let error = git.create_worktree_from_plan(&plan).unwrap_err();

    assert_eq!(error.kind(), GitErrorKind::CommandFailed);
    assert!(error.message().contains("safely rolled back"));
    assert!(!plan.destination().exists());
    assert!(branch_exists(&fixture.repository, plan.branch()));
    assert_no_worktree_registration(&git, &fixture, plan.destination());
}

#[cfg(unix)]
#[test]
fn timeout_after_post_checkout_effect_is_reconciled_and_compensated() {
    let fixture = RepositoryFixture::new();
    install_post_checkout_hook(&fixture, "sleep 30");
    let git = Git::discover()
        .unwrap()
        .with_timeout(Duration::from_millis(750))
        .unwrap();
    let plan = git
        .plan_worktree(
            &fixture.repository,
            &fixture.managed,
            "Hook Timeout",
            "71ae0a70",
        )
        .unwrap();
    let started = Instant::now();

    let error = git.create_worktree_from_plan(&plan).unwrap_err();

    assert_eq!(error.kind(), GitErrorKind::Timeout);
    assert!(error.message().contains("safely rolled back"));
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(!plan.destination().exists());
    assert!(branch_exists(&fixture.repository, plan.branch()));
    assert_no_worktree_registration(&git, &fixture, plan.destination());
}

#[cfg(unix)]
#[test]
fn worktree_remove_never_receives_force() {
    let fixture = RepositoryFixture::new();
    let (git, remove_seen, force_seen) = removal_auditing_git(&fixture);
    let created = git
        .create_worktree(
            &fixture.repository,
            &fixture.managed,
            "No Force",
            "f0ace000",
        )
        .unwrap();

    git.remove_worktree(
        &fixture.repository,
        &fixture.managed,
        &created.path,
        WorktreeUse::default,
    )
    .expect("non-forced removal should succeed");

    assert!(remove_seen.exists());
    assert!(!force_seen.exists());
}

#[test]
fn repeated_plan_create_remove_stress_preserves_registry_and_branches() {
    let fixture = RepositoryFixture::new();
    let git = Git::discover().unwrap();
    for index in 0..24 {
        let plan = git
            .plan_worktree(
                &fixture.repository,
                &fixture.managed,
                &format!("Stress {index}"),
                &format!("5a9a{index:04}"),
            )
            .unwrap();
        let created = git.create_worktree_from_plan(&plan).unwrap();
        assert_eq!(created.head.as_deref(), Some(plan.initial_oid()));
        git.remove_worktree(
            &fixture.repository,
            &fixture.managed,
            &created.path,
            WorktreeUse::default,
        )
        .unwrap();
        assert!(branch_exists(&fixture.repository, plan.branch()));
    }
    assert_eq!(git.list_worktrees(&fixture.repository).unwrap().len(), 1);
}

fn assert_no_worktree_registration(git: &Git, fixture: &RepositoryFixture, destination: &Path) {
    assert!(
        git.list_worktrees(&fixture.repository)
            .unwrap()
            .iter()
            .all(|worktree| worktree.path != destination)
    );
}
