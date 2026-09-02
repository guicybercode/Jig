import { describe, expect, it } from "vitest";

import {
  appendBrowserUrlToNote,
  browserUrlForTerminal,
} from "./browser-handoff";

describe("browser handoff", () => {
  it("creates terminal input without an Enter keystroke", () => {
    const payload = browserUrlForTerminal("example.com/review");

    expect(payload).toBe("https://example.com/review");
    expect(payload).not.toMatch(/[\r\n]/);
  });

  it("appends a plain browser URL to an existing note", () => {
    expect(
      appendBrowserUrlToNote("Release context", "https://example.com/docs"),
    ).toBe("Release context\n\nhttps://example.com/docs");
  });

  it("does not hand off invalid or control-character addresses", () => {
    expect(browserUrlForTerminal("javascript:alert(1)")).toBeNull();
    expect(browserUrlForTerminal("https://example.com/\nsubmit")).toBeNull();
    expect(
      appendBrowserUrlToNote("Keep this", "file:///etc/passwd"),
    ).toBe("Keep this");
  });

  it("removes callback secrets before handing an address to another node", () => {
    expect(
      browserUrlForTerminal(
        "https://example.com/callback?tab=build&code=secret#token",
      ),
    ).toBe("https://example.com/callback?tab=build");
  });
});
