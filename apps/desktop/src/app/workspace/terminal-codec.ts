import type { TerminalInput } from "../features/terminal";

/** Encodes a byte array as canonical padded base64. */
export function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}

/** Encodes UTF-8 text as base64 for `session.write`. */
export function utf8ToBase64(text: string): string {
  return bytesToBase64(new TextEncoder().encode(text));
}

/** Decodes daemon `session.output` payloads into bytes for xterm. */
export function base64ToBytes(value: string): Uint8Array {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

/** Encodes xterm input using the same base64 contract as `session.write`. */
export function terminalInputToBase64(input: TerminalInput): string {
  if (input.kind === "text") {
    return utf8ToBase64(input.data);
  }
  return btoa(input.data);
}
