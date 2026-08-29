import { test } from "@playwright/test";

const tauriHarness = process.env.CLI_MASTER_TAURI_E2E === "1";

test.describe("Beta UI acceptance", () => {
  test.skip(
    !tauriHarness,
    "The desktop shell on this branch has no session grid yet. Runtime acceptance lives in crates/e2e and drives SessionManager, Git, storage, and daemon bind without adding production test hooks. Set CLI_MASTER_TAURI_E2E=1 when a real Tauri window can host two live tiles.",
  );

  test("adds a local repository", async () => {
    test.info().annotations.push({
      type: "covered-by",
      description: "crates/e2e/tests/acceptance.rs::adds_a_local_repository_and_runs_two_grid_sessions",
    });
  });

  test("creates two sessions on different worktrees and branches", async () => {
    test.info().annotations.push({
      type: "covered-by",
      description: "crates/e2e/tests/acceptance.rs::adds_a_local_repository_and_runs_two_grid_sessions",
    });
  });

  test("opens both sessions in a grid and talks to both terminals", async () => {
    test.info().annotations.push({
      type: "covered-by",
      description: "crates/e2e/tests/acceptance.rs::adds_a_local_repository_and_runs_two_grid_sessions",
    });
  });

  test("resizes one tile without mixing output", async () => {
    test.info().annotations.push({
      type: "covered-by",
      description: "crates/e2e/tests/acceptance.rs::adds_a_local_repository_and_runs_two_grid_sessions",
    });
  });

  test("stops one session without affecting the other", async () => {
    test.info().annotations.push({
      type: "covered-by",
      description: "crates/e2e/tests/acceptance.rs::adds_a_local_repository_and_runs_two_grid_sessions",
    });
  });

  test("closes and reopens the window, then reconnects output", async () => {
    test.info().annotations.push({
      type: "covered-by",
      description: "crates/e2e/tests/acceptance.rs::adds_a_local_repository_and_runs_two_grid_sessions",
    });
  });

  test("refuses to remove a dirty worktree", async () => {
    test.info().annotations.push({
      type: "covered-by",
      description: "crates/e2e/tests/acceptance.rs::dirty_worktree_cannot_be_removed",
    });
  });

  test("daemon restart marks stale live sessions unknown", async () => {
    test.info().annotations.push({
      type: "covered-by",
      description:
        "crates/e2e/tests/acceptance.rs::daemon_restart_converts_stale_live_sessions_to_unknown_without_killing_the_pid",
    });
  });
});
