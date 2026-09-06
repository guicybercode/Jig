import type { TerminalInputModes } from "../terminal/terminal-runtime";

/** Mirrors core wire MAX_PTY_INPUT_BYTES, including paste framing and Return. */
export const MAX_PROMPT_INPUT_BYTES = 64 * 1024;

export type PromptInputKey = "Enter" | "Tab" | "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight";

const BRACKETED_PASTE_START = "\u001b[200~";
const BRACKETED_PASTE_END = "\u001b[201~";
const encoder = new TextEncoder();

/** Matches xterm's paste newline normalization without interpreting shell syntax. */
function promptPayload(text: string, bracketedPasteMode: boolean): string {
  const normalized = text.replace(/\r\n|\n/g, "\r");
  return bracketedPasteMode
    ? `${BRACKETED_PASTE_START}${normalized}${BRACKETED_PASTE_END}\r`
    : `${normalized}\r`;
}

/** Static errors never include the prompt, paths, or other user input. */
export function getPromptInputError(text: string, modes?: TerminalInputModes): string | undefined {
  if (!text.trim()) return "Write a prompt before sending.";
  if (/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/.test(text)) {
    return "Remove terminal control characters from the prompt before sending.";
  }
  // Before a terminal is available, reserve the largest supported paste framing.
  const payload = promptPayload(text, modes?.bracketedPasteMode ?? true);
  if (encoder.encode(payload).byteLength > MAX_PROMPT_INPUT_BYTES) {
    return "This prompt exceeds the terminal's 64 KiB input limit. Shorten it before sending.";
  }
  return undefined;
}

/** Encodes one explicitly submitted prompt using the live terminal's modes. */
export function encodePromptInput(text: string, modes: TerminalInputModes): Uint8Array {
  const error = getPromptInputError(text, modes);
  if (error) throw new Error(error);
  return encoder.encode(promptPayload(text, modes.bracketedPasteMode));
}

/** Empty-composer navigation honors the program's application-cursor mode. */
export function encodePromptTerminalKey(key: PromptInputKey, modes: TerminalInputModes): Uint8Array {
  if (key === "Enter") return encoder.encode("\r");
  if (key === "Tab") return encoder.encode("\t");
  const suffix = { ArrowUp: "A", ArrowDown: "B", ArrowRight: "C", ArrowLeft: "D" }[key];
  return encoder.encode(`\u001b${modes.applicationCursorKeysMode ? "O" : "["}${suffix}`);
}
