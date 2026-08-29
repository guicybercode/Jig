import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { AgentApiProvider } from "../../../ipc/AgentApiProvider";
import { BUILTIN_AGENT_IDS, isUuidV7 } from "../../../ipc/uuid";
import { createMemoryAgentApi } from "../../../ipc/memoryAgentApi";
import { AgentsView } from "./AgentsView";
import { CreateSessionDialog } from "./CreateSessionDialog";

function renderAgents(
  api = createMemoryAgentApi({
    installs: {
      claude: { path: "/home/user/.local/bin/claude", version: "claude 1.2.3" },
    },
  }),
) {
  return render(
    <AgentApiProvider api={api}>
      <AgentsView hasProject={false} />
    </AgentApiProvider>,
  );
}

describe("AgentsView", () => {
  it("lists built-ins with UUIDv7 ids instead of adapter keys", async () => {
    renderAgents();
    expect(screen.getByRole("status")).toHaveTextContent("Loading agents");
    const codex = await screen.findByRole("heading", { name: "Codex" });
    const card = codex.closest("article");
    expect(card).not.toBeNull();
    expect(within(card as HTMLElement).getByText(BUILTIN_AGENT_IDS.codex)).toBeVisible();
    expect(isUuidV7(BUILTIN_AGENT_IDS.codex)).toBe(true);
    expect(card).not.toHaveAttribute("data-agent-id", "codex");
    expect(within(card as HTMLElement).getByText("Missing")).toBeVisible();
    expect(within(card as HTMLElement).getByText("Enabled")).toBeVisible();
  });

  it("detects an installed CLI and copies diagnostics without secrets", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });

    renderAgents();
    const claude = await screen.findByRole("heading", { name: "Claude Code" });
    const card = claude.closest("article") as HTMLElement;
    await user.click(within(card).getByRole("button", { name: "Detect" }));
    expect(await within(card).findByText("Installed")).toBeVisible();
    expect(within(card).getByText("claude 1.2.3")).toBeVisible();
    expect(within(card).getByText("/home/user/.local/bin/claude")).toBeVisible();

    await user.click(within(card).getByRole("button", { name: "Diagnostics" }));
    const dialog = screen.getByRole("dialog", { name: "Claude Code diagnostics" });
    expect(dialog).toHaveTextContent(`id: ${BUILTIN_AGENT_IDS.claude}`);
    expect(dialog).not.toHaveTextContent("codex");
    expect(dialog.textContent).not.toMatch(/sk-|token|secret/i);
    await user.click(screen.getByRole("button", { name: "Copy diagnostics" }));
    expect(writeText).toHaveBeenCalled();
    const copied = writeText.mock.calls[0]?.[0] as string;
    expect(copied).toContain(BUILTIN_AGENT_IDS.claude);
    expect(copied).not.toMatch(/sk-|super-secret/);
  });

  it("creates, validates, and removes a custom agent without putting secrets in the DOM", async () => {
    const user = userEvent.setup();
    renderAgents();
    await screen.findByRole("heading", { name: "Codex" });
    await user.click(screen.getByRole("button", { name: "Add custom agent" }));

    expect(screen.getByRole("dialog", { name: "Add custom agent" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Create agent" }));
    expect(screen.getByText("Name is required.")).toBeVisible();

    await user.type(screen.getByLabelText("Name"), "Internal Agent");
    await user.type(screen.getByLabelText("Executable"), "tools/agent");
    await user.click(screen.getByRole("button", { name: "Create agent" }));
    expect(
      screen.getByText(/absolute path, a ~\/ path, a placeholder, or a bare command name/i),
    ).toBeVisible();

    await user.clear(screen.getByLabelText("Executable"));
    await user.type(screen.getByLabelText("Executable"), "/opt/internal-agent");
    await user.type(screen.getByLabelText("Argument 1"), "--workspace");
    await user.click(screen.getByRole("button", { name: "Add argument" }));
    await user.type(screen.getByLabelText("Argument 2"), "${PROJECT_PATH}");
    await user.type(screen.getByLabelText("Environment name 1"), "ACCESS_TOKEN");
    await user.type(screen.getByLabelText("Environment value 1"), "super-secret");
    await user.click(screen.getByRole("button", { name: "Create agent" }));

    expect(await screen.findByRole("heading", { name: "Internal Agent" })).toBeVisible();
    expect(screen.queryByRole("dialog", { name: "Add custom agent" })).not.toBeInTheDocument();
    expect(document.body.textContent).not.toContain("super-secret");
    expect(document.body.innerHTML).not.toContain("super-secret");

    const custom = screen.getByRole("heading", { name: "Internal Agent" }).closest("article") as HTMLElement;
    expect(within(custom).getByText("ACCESS_TOKEN")).toBeVisible();
    const publicId = within(custom).getByText(/^[0-9a-f-]{36}$/i).textContent ?? "";
    expect(isUuidV7(publicId)).toBe(true);
    expect(publicId).not.toBe("internal-agent");

    await user.click(within(custom).getByRole("button", { name: "Remove" }));
    expect(
      screen.getByText(/executable on disk is not deleted/i),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Remove agent" }));
    expect(screen.queryByRole("heading", { name: "Internal Agent" })).not.toBeInTheDocument();
  });

  it("disables a built-in without renaming its public id", async () => {
    const user = userEvent.setup();
    renderAgents();
    const gemini = await screen.findByRole("heading", { name: "Gemini CLI" });
    const card = gemini.closest("article") as HTMLElement;
    await user.click(within(card).getByRole("button", { name: "Disable" }));
    expect(await within(card).findByText("Disabled")).toBeVisible();
    expect(within(card).getByText(BUILTIN_AGENT_IDS.gemini)).toBeVisible();
  });

  it("shows an error state with retry", async () => {
    render(
      <AgentApiProvider api={createMemoryAgentApi({ failList: true })}>
        <AgentsView hasProject={false} />
      </AgentApiProvider>,
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The agent catalog could not be loaded.",
    );
    expect(screen.getByRole("button", { name: "Retry" })).toBeVisible();
  });

  it("shows an empty catalog state", async () => {
    render(
      <AgentApiProvider api={createMemoryAgentApi({ empty: true })}>
        <AgentsView hasProject={false} />
      </AgentApiProvider>,
    );
    expect(
      await screen.findByRole("heading", { name: "No agents in the catalog" }),
    ).toBeVisible();
  });
});

describe("CreateSessionDialog", () => {
  it("submits a UUIDv7 agent id rather than an adapter key", async () => {
    const user = userEvent.setup();
    const onCreate = vi.fn();
    const api = createMemoryAgentApi({
      installs: {
        codex: { path: "/usr/bin/codex", version: "0.1" },
      },
    });
    const { agents } = await api.list();
    render(
      <CreateSessionDialog
        agents={agents}
        hasProject
        onClose={() => undefined}
        onCreate={onCreate}
      />,
    );
    await user.type(screen.getByLabelText("Session name"), "Implement auth");
    await user.click(screen.getByRole("button", { name: "Create session" }));
    expect(onCreate).toHaveBeenCalledWith({
      name: "Implement auth",
      agentId: BUILTIN_AGENT_IDS.codex,
    });
    expect(onCreate.mock.calls[0][0].agentId).not.toBe("codex");
  });
});
