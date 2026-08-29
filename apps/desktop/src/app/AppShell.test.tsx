import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { createMockIpcClient } from "../ipc";
import { helloFixture, snapshotFixture } from "../test/ipc-fixtures";
import { AppShell } from "./AppShell";

function renderConnectedShell() {
  const client = createMockIpcClient({
    "system.hello": () => helloFixture(),
    "state.snapshot": () => snapshotFixture(),
    "agent.detect": () => ({ detections: [] }),
  });
  render(<AppShell client={client} />);
  return client;
}

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

  it("shows connected project metadata from the official hello and snapshot", async () => {
    renderConnectedShell();

    expect(
      await screen.findByRole("heading", { name: "Demo", level: 1 }),
    ).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent("Daemon connected");
    expect(screen.getByRole("button", { name: "New Session" })).toBeEnabled();
    expect(screen.getByText("/tmp/demo · main")).toBeVisible();
  });

  it("opens the new session dialog from the header and keeps keyboard focus inside it", async () => {
    const user = userEvent.setup();
    renderConnectedShell();
    await screen.findByRole("heading", { name: "Demo", level: 1 });

    const opener = screen.getByRole("button", { name: "New Session" });
    await user.click(opener);

    const dialog = await screen.findByRole("dialog", { name: "New session" });
    expect(dialog).toBeVisible();

    await waitFor(() => {
      expect(dialog.querySelector("#new-session-dialog-form-name")).toHaveFocus();
    });

    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("dialog", { name: "New session" }),
    ).not.toBeInTheDocument();
    expect(opener).toHaveFocus();
  });

  it("opens commands from the header and restores focus on Escape", async () => {
    const user = userEvent.setup();
    render(<AppShell />);

    const opener = screen.getByRole("button", { name: "Commands" });
    await user.click(opener);

    const palette = screen.getByRole("dialog", { name: "Command palette" });
    expect(palette).toBeVisible();
    expect(
      screen.getByRole("searchbox", { name: "Search commands" }),
    ).toHaveFocus();
    expect(
      within(palette).getByRole("button", { name: "New Session" }),
    ).toHaveAccessibleDescription(
      "Connect the daemon and select a project first.",
    );

    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("dialog", { name: "Command palette" }),
    ).not.toBeInTheDocument();
    expect(opener).toHaveFocus();
  });

  it("opens commands globally without stealing text or terminal input", async () => {
    const user = userEvent.setup();
    renderConnectedShell();
    await screen.findByRole("heading", { name: "Demo", level: 1 });

    await user.keyboard("{Control>}k{/Control}");
    expect(
      screen.getByRole("dialog", { name: "Command palette" }),
    ).toBeVisible();
    await user.keyboard("{Escape}");

    const projectPath = screen.getByRole("textbox", {
      name: "Repository path",
    });
    projectPath.focus();
    await user.keyboard("{Control>}k{/Control}");
    expect(
      screen.queryByRole("dialog", { name: "Command palette" }),
    ).not.toBeInTheDocument();
    expect(projectPath).toHaveFocus();

    render(
      <section data-terminal-root="true">
        <textarea aria-label="Terminal input" />
      </section>,
    );
    const terminalInput = screen.getByRole("textbox", {
      name: "Terminal input",
    });
    terminalInput.focus();
    await user.keyboard("{Control>}k{/Control}");
    expect(
      screen.queryByRole("dialog", { name: "Command palette" }),
    ).not.toBeInTheDocument();
    expect(terminalInput).toHaveFocus();
  });

  it("routes a palette command through the current workspace actions", async () => {
    const user = userEvent.setup();
    renderConnectedShell();
    await screen.findByRole("heading", { name: "Demo", level: 1 });

    await user.click(screen.getByRole("button", { name: "Commands" }));
    const palette = screen.getByRole("dialog", { name: "Command palette" });
    await user.click(within(palette).getByRole("button", { name: "New Session" }));

    const dialog = await screen.findByRole("dialog", { name: "New session" });
    expect(dialog).toBeVisible();
    await waitFor(() => {
      expect(dialog.querySelector("#new-session-dialog-form-name")).toHaveFocus();
    });
  });
});
