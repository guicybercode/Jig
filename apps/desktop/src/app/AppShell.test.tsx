import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { AppShell } from "./AppShell";

describe("AppShell", () => {
  it("explains why session and project actions are unavailable", () => {
    render(<AppShell />);

    expect(
      screen.getByRole("button", { name: "New Session" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "New Session" }),
    ).toHaveAccessibleDescription("Add a project first");
    expect(
      screen.getByRole("button", { name: "Add Project" }),
    ).toHaveAccessibleDescription(
      "Available when the local daemon is connected.",
    );
  });

  it("reports an honest empty local workspace", () => {
    render(<AppShell />);

    expect(
      screen.getByRole("heading", { name: "No project selected", level: 1 }),
    ).toBeVisible();
    expect(
      screen.getByRole("heading", { name: "Add a repository to begin" }),
    ).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent("Daemon unavailable");
    expect(
      screen.getByRole("navigation", { name: "Workspace navigation" }),
    ).toBeVisible();
  });

  it("places the workspace skip link first in keyboard order", async () => {
    const user = userEvent.setup();
    render(<AppShell />);

    await user.tab();

    const skipLink = screen.getByRole("link", { name: "Skip to workspace" });
    expect(skipLink).toHaveFocus();
    expect(skipLink).toHaveAttribute("href", "#workspace");
    expect(screen.getByRole("main")).toHaveAttribute("id", "workspace");
  });

  it("shows connected project metadata from the daemon snapshot", async () => {
    const client = {
      async request(method: string) {
        if (method === "state.snapshot") {
          return {
            daemon: {
              protocolVersion: 1,
              appVersion: "0.1.0-beta.1",
              instanceId: "dev",
              platform: "linux",
            },
            projects: [
              {
                id: "11111111-1111-1111-1111-111111111111",
                name: "Demo",
                path: "/tmp/demo",
                currentBranch: "main",
              },
            ],
            agents: [
              {
                id: "codex",
                displayName: "Codex",
                source: "built_in",
                enabled: true,
                detected: false,
                executable: "codex",
                args: [],
              },
            ],
            sessions: [],
            worktrees: [],
          };
        }
        throw new Error(`unexpected method ${method}`);
      },
    };

    render(<AppShell client={client} />);

    expect(
      await screen.findByRole("heading", { name: "Demo", level: 1 }),
    ).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent("Daemon connected");
    expect(screen.getByRole("button", { name: "New Session" })).toBeEnabled();
    expect(screen.getByText("/tmp/demo · main")).toBeVisible();
  });
});
