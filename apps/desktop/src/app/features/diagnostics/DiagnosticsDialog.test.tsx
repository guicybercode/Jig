import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { DiagnosticsDialog } from "./DiagnosticsDialog";
import type { DiagnosticsReport } from "./types";

const report: DiagnosticsReport = {
  appVersion: "0.1.0",
  os: "linux",
  arch: "x86_64",
  dataDir: "/tmp/cli-master/data",
  configDir: "/tmp/cli-master/config",
  runtimeDir: "/tmp/cli-master/runtime",
  databasePath: "/tmp/cli-master/data/cli-master.db",
  logDir: "/tmp/cli-master/logs",
  gitVersion: "2.43.0",
  gitAvailable: true,
  daemon: {
    connected: false,
    status: "No daemon is running. Session processes are not attached.",
  },
  sqlite: {
    fileExists: false,
    available: false,
    status: "Database file has not been created yet.",
  },
  agents: [
    {
      key: "codex",
      displayName: "Codex",
      detected: false,
    },
  ],
  executables: [{ name: "git", path: "/usr/bin/git" }],
  sessionCount: 0,
  worktreeCount: 0,
  recentLogs: [
    {
      timestamp: "2026-08-29T00:00:00.000Z",
      level: "info",
      target: "diagnostics",
      operation: "diagnostics.get",
      message: "TOKEN=[redacted]",
    },
  ],
  recentErrors: [
    {
      code: "WORKTREE_DIRTY",
      message: "Worktree has uncommitted changes.",
      action: "Commit or move the changes, or confirm dirty removal explicitly.",
    },
  ],
};

describe("DiagnosticsDialog", () => {
  it("renders a sanitized snapshot and copies it", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });

    render(
      <DiagnosticsDialog
        exportReport={async () => JSON.stringify(report)}
        load={async () => report}
        onClose={() => undefined}
      />,
    );

    expect(
      await screen.findByRole("heading", { name: "Diagnostics" }),
    ).toBeVisible();
    expect(screen.getByText("0.1.0")).toBeVisible();
    expect(
      screen.getByText("No daemon is running. Session processes are not attached."),
    ).toBeVisible();
    expect(screen.queryByText("super-secret")).not.toBeInTheDocument();
    expect(screen.queryByText(/PWD=/)).not.toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: "Copy sanitized diagnostics" }),
    );
    expect(writeText).toHaveBeenCalled();
    const copied = String(writeText.mock.calls[0]?.[0] ?? "");
    expect(copied).toContain("0.1.0");
    expect(copied).not.toContain("super-secret");
    expect(
      screen.getByRole("status").textContent,
    ).toMatch(/Copied/);
  });

  it("never falls back to copying the raw diagnostics response", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    render(
      <DiagnosticsDialog
        exportReport={async () => {
          throw new Error("native export failed");
        }}
        load={async () => ({
          ...report,
          dataDir: "/Users/private-user/.local/share/cli-master",
        })}
        onClose={() => undefined}
      />,
    );
    await screen.findByText("0.1.0");

    await user.click(
      screen.getByRole("button", { name: "Copy sanitized diagnostics" }),
    );

    expect(writeText).not.toHaveBeenCalled();
    expect(screen.getByRole("status")).toHaveTextContent(
      "Diagnostics export failed",
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "do not copy the on-screen paths",
    );
  });

  it("does not render native error details that may contain secrets", async () => {
    render(
      <DiagnosticsDialog
        load={async () => {
          throw new Error("TOKEN=loader-secret");
        }}
        onClose={() => undefined}
      />,
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Diagnostics could not be loaded safely",
    );
    expect(screen.queryByText(/loader-secret/)).not.toBeInTheDocument();
  });
});
