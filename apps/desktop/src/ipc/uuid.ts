/** Well-known UUIDv7 identifiers for built-in agents. Never use adapter keys as IDs. */
export const BUILTIN_AGENT_IDS = {
  codex: "01936a10-0000-7000-8000-000000000001",
  claude: "01936a10-0000-7000-8000-000000000002",
  gemini: "01936a10-0000-7000-8000-000000000003",
  opencode: "01936a10-0000-7000-8000-000000000004",
} as const;

const UUID_V7 =
  /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

/** Returns whether `value` is a UUIDv7 string. */
export function isUuidV7(value: string): boolean {
  return UUID_V7.test(value);
}

/** Creates a UUIDv7 identifier. */
export function createUuidV7(now = Date.now()): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  const time = BigInt(now);
  bytes[0] = Number((time >> 40n) & 0xffn);
  bytes[1] = Number((time >> 32n) & 0xffn);
  bytes[2] = Number((time >> 24n) & 0xffn);
  bytes[3] = Number((time >> 16n) & 0xffn);
  bytes[4] = Number((time >> 8n) & 0xffn);
  bytes[5] = Number(time & 0xffn);
  bytes[6] = (bytes[6] & 0x0f) | 0x70;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
