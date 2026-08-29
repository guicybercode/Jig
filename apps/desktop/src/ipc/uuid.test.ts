import { describe, expect, it } from "vitest";

import { BUILTIN_AGENT_IDS, createUuidV7, isUuidV7 } from "./uuid";

describe("agent identifiers", () => {
  it("keeps built-in public ids as UUIDv7 values, not adapter keys", () => {
    expect(BUILTIN_AGENT_IDS.codex).toBe("01936a10-0000-7000-8000-000000000001");
    expect(BUILTIN_AGENT_IDS.claude).toBe("01936a10-0000-7000-8000-000000000002");
    expect(BUILTIN_AGENT_IDS.gemini).toBe("01936a10-0000-7000-8000-000000000003");
    expect(BUILTIN_AGENT_IDS.opencode).toBe("01936a10-0000-7000-8000-000000000004");

    for (const [key, id] of Object.entries(BUILTIN_AGENT_IDS)) {
      expect(isUuidV7(id)).toBe(true);
      expect(id).not.toBe(key);
      expect(id).not.toContain(key);
    }
  });

  it("generates UUIDv7 identifiers for custom agents", () => {
    const id = createUuidV7(1_700_000_000_000);
    expect(isUuidV7(id)).toBe(true);
    expect(id).not.toBe("custom");
  });
});
