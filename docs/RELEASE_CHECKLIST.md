# Beta v0.1 release checklist

Run automated gates first, then perform this checklist against the exact
package intended for distribution. Record the candidate commit, package
checksum, operating system, and tester with the results.

## Manual package acceptance

Every item below starts unchecked. A Vite/Playwright smoke run or a Rust
acceptance test is useful evidence, but neither substitutes for exercising the
real packaged GUI and bundled daemon.

- [ ] Install or mount the actual release-candidate package, launch CLI Master,
      and confirm the packaged desktop connects to its bundled daemon.
- [ ] Add an existing Git repository through **Repository path** and confirm
      that its path and current branch are displayed correctly.
- [ ] Create two sessions for that repository and confirm that each uses its
      own managed worktree and appears as a separate live terminal.
- [ ] Type in both terminals, resize a terminal, then stop one session and
      confirm that the other terminal remains interactive.
- [ ] Disconnect and reconnect the desktop while the daemon remains running;
      confirm that both sessions remain listed and retained output is replayed.
- [ ] Make a worktree dirty, attempt to remove it, and confirm removal is
      blocked without deleting or modifying the user's file.
- [ ] Record all failures and unresolved platform limitations in the release
      notes before attaching or publishing the package.

See [KNOWN_ISSUES.md](KNOWN_ISSUES.md) for limitations that are expected in
the Beta and must not be reported as successful manual acceptance.
