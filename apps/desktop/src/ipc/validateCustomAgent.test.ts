import { describe, expect, it } from "vitest";

import { hasFieldErrors, validateCustomAgent } from "./validateCustomAgent";
import type { CustomAgentInput } from "./agentTypes";

function input(overrides: Partial<CustomAgentInput> = {}): CustomAgentInput {
  return {
    displayName: "Internal",
    executable: "/opt/agent",
    args: ["--workspace"],
    env: [{ key: "ACCESS_TOKEN", value: "super-secret" }],
    defaultCwd: "",
    requiresPty: true,
    ...overrides,
  };
}

describe("validateCustomAgent", () => {
  it("accepts structured fields without invoking a shell", () => {
    expect(hasFieldErrors(validateCustomAgent(input()))).toBe(false);
  });

  it("rejects a relative path that is not a bare command name", () => {
    const errors = validateCustomAgent(input({ executable: "tools/agent" }));
    expect(errors.executable).toMatch(/absolute path, a ~\/ path, a placeholder, or a bare command name/);
  });

  it("rejects empty names and = in environment keys", () => {
    const errors = validateCustomAgent(
      input({
        displayName: "  ",
        env: [{ key: "FOO=BAR", value: "1" }],
      }),
    );
    expect(errors.displayName).toBe("Name is required.");
    expect(errors.env).toMatch(/must not contain '='/);
  });
});
