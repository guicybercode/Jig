import { describe, expect, it } from "vitest";

import { encodePromptInput, encodePromptTerminalKey, getPromptInputError, MAX_PROMPT_INPUT_BYTES } from "./prompt-input";

const plain = { bracketedPasteMode: false, applicationCursorKeysMode: false };
const application = { bracketedPasteMode: true, applicationCursorKeysMode: true };
const decode = (value: Uint8Array) => new TextDecoder().decode(value);

describe("prompt input encoding", () => {
  it("uses live paste mode and one trailing Return without shell interpolation", () => {
    const prompt = "printf '$HOME'\r\nreview `file`\n$(literal) — ação";
    const normalized = "printf '$HOME'\rreview `file`\r$(literal) — ação";
    expect(decode(encodePromptInput(prompt, plain))).toBe(`${normalized}\r`);
    expect(decode(encodePromptInput(prompt, application))).toBe(`\u001b[200~${normalized}\u001b[201~\r`);
  });

  it.each(["\u001b[201~run", "stop\u0003", "null\u0000", "delete\u007f", "csi\u009b"])(
    "rejects embedded terminal control sequences without echoing user input",
    (text) => {
      expect(() => encodePromptInput(text, application)).toThrow("Remove terminal control characters");
      expect(getPromptInputError(text)).not.toContain(text);
    },
  );

  it("preserves tabs and whitespace but rejects an empty submission", () => {
    expect(decode(encodePromptInput("  line\t  ", plain))).toBe("  line\t  \r");
    expect(() => encodePromptInput(" \n\t", plain)).toThrow("Write a prompt");
  });

  it("bounds UTF-8 input including framing without truncating the prompt", () => {
    expect(encodePromptInput("x".repeat(MAX_PROMPT_INPUT_BYTES - 13), application)).toHaveLength(MAX_PROMPT_INPUT_BYTES);
    expect(() => encodePromptInput("x".repeat(MAX_PROMPT_INPUT_BYTES - 12), application)).toThrow("64 KiB");
    expect(encodePromptInput("x".repeat(MAX_PROMPT_INPUT_BYTES - 1), plain)).toHaveLength(MAX_PROMPT_INPUT_BYTES);
    expect(() => encodePromptInput("🦉".repeat(16_384), application)).toThrow("64 KiB");
  });

  it("uses normal or application cursor sequences for the same logical key", () => {
    for (const [key, suffix] of [["ArrowUp", "A"], ["ArrowDown", "B"], ["ArrowRight", "C"], ["ArrowLeft", "D"]] as const) {
      expect(decode(encodePromptTerminalKey(key, plain))).toBe(`\u001b[${suffix}`);
      expect(decode(encodePromptTerminalKey(key, application))).toBe(`\u001bO${suffix}`);
    }
    expect(decode(encodePromptTerminalKey("Enter", application))).toBe("\r");
    expect(decode(encodePromptTerminalKey("Tab", plain))).toBe("\t");
  });
});
