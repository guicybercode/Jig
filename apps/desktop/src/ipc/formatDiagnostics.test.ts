import { describe, expect, it } from "vitest";

import { formatAgentDiagnostics } from "./formatDiagnostics";
import { BUILTIN_AGENT_IDS } from "./uuid";
import type { AgentRecord } from "./agentTypes";

const claude: AgentRecord = {
  id: BUILTIN_AGENT_IDS.claude,
  adapterKey: "claude",
  displayName: "Claude Code",
  source: "built_in",
  enabled: true,
  installed: true,
  executable: "claude",
  defaultArgs: [],
  envKeys: ["ACCESS_TOKEN"],
  requiresPty: true,
  resolvedPath: "/home/user/.local/bin/claude",
  version: "claude 1.2.3",
};

describe("formatAgentDiagnostics", () => {
  it("uses the public UUIDv7 and omits adapter keys and secret-like values", () => {
    const text = formatAgentDiagnostics(claude, {
      agentId: claude.id,
      displayName: claude.displayName,
      installed: true,
      launchTest: { status: "success" },
      searchedPaths: ["/home/user/.local/bin"],
      path: claude.resolvedPath,
      version: claude.version,
    });

    expect(text).toContain(`id: ${BUILTIN_AGENT_IDS.claude}`);
    expect(text).not.toContain("codex");
    expect(text).not.toMatch(/\bsk-/);
    expect(text).not.toContain("super-secret");
    expect(text).toContain("envKeys: ACCESS_TOKEN");
    expect(text).not.toContain("adapterKey");
  });

  it("redacts token-shaped fragments in warnings and failed launch details", () => {
    const text = formatAgentDiagnostics(claude, {
      agentId: claude.id,
      displayName: claude.displayName,
      installed: false,
      launchTest: { status: "failed", message: "probe failed sk-live-example" },
      searchedPaths: [],
      warning: "ignored sk-live-example",
    });

    expect(text).not.toContain("sk-live-example");
    expect(text).toContain("<redacted>");
  });
});
