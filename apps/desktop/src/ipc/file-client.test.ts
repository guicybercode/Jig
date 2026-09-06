import { beforeEach, describe, expect, it, vi } from "vitest";

const transport = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: transport.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn() }));

import { createMockIpcClient } from "../test/mockIpc";
import { createTauriIpcClient, IpcError } from "./client";
import type { FileListRequest, FileReadRequest, FileWriteRequest } from "./domain";
import type { RequestEnvelope } from "./types";

const REVISION = `v1:${"a".repeat(64)}`;
const NEXT_REVISION = `v1:${"b".repeat(64)}`;
const BYTES = "name-\xff\\file:part.txt";
const PATH = globalThis.btoa(BYTES);
const TEXT = "\u{feff}olá 🦀\r\nlast line without newline";
const NOW = 1_787_941_200_000;
const READ: FileReadRequest = {
  target: { kind: "session", sessionId: "0198f000-0000-7000-8000-000000000001" },
  pathBase64: PATH,
};
const WRITE: FileWriteRequest = {
  target: { kind: "worktree", worktreeId: "0198f000-0000-7000-8000-000000000002" },
  pathBase64: PATH,
  text: TEXT,
  expectedRevision: REVISION,
};
const LIST: FileListRequest = {
  target: { kind: "project", projectId: "0198f000-0000-7000-8000-000000000003" },
  pathBase64: "",
};
const READ_RESPONSE = {
  pathBase64: PATH,
  text: TEXT,
  revision: REVISION,
  sizeBytes: new TextEncoder().encode(TEXT).byteLength,
  observedAtMs: NOW,
};
const WRITE_RESPONSE = {
  pathBase64: PATH,
  revision: NEXT_REVISION,
  sizeBytes: READ_RESPONSE.sizeBytes,
  writtenAtMs: NOW,
};
const ENTRY = {
  pathBase64: PATH,
  displayName: "name-\\xff\\file:part.txt",
  kind: "file",
};

/** Use the production envelope code and mock only the native transport boundary. */
function respond(data: unknown): void {
  transport.invoke.mockImplementation(async (command: string, args: { request: RequestEnvelope<unknown> }) => {
    expect(command).toBe("daemon_request");
    return {
      kind: "response", version: 1, requestId: args.request.requestId,
      status: "success", data,
    };
  });
}

describe("file IPC transport and decoding", () => {
  beforeEach(() => {
    transport.invoke.mockReset();
  });

  it("preserves registered targets, byte-exact identifiers, revisions and UTF-8 text across the generic transport", async () => {
    const client = createTauriIpcClient();
    const page = { entries: [ENTRY], nextAfterNameBase64: PATH, observedAtMs: NOW };
    respond(page);
    const listInput = { ...LIST, limit: 25, afterNameBase64: globalThis.btoa("earlier") };
    expect(await client.listFiles(listInput)).toEqual(page);

    respond({ ...READ_RESPONSE, modifiedAtMs: -1_000 });
    expect(await client.readFile(READ)).toEqual({ ...READ_RESPONSE, modifiedAtMs: -1_000 });

    respond(WRITE_RESPONSE);
    const written = await client.writeFile(WRITE);
    expect(written).toEqual(WRITE_RESPONSE);
    expect(written).not.toHaveProperty("modifiedAtMs");
    expect(transport.invoke.mock.calls.map(([command, args]) => ({
      command,
      method: args.request.method,
      payload: args.request.payload,
    }))).toEqual([
      { command: "daemon_request", method: "file.list", payload: listInput },
      { command: "daemon_request", method: "file.read", payload: READ },
      { command: "daemon_request", method: "file.write", payload: WRITE },
    ]);
    expect(globalThis.atob(written.pathBase64)).toBe(BYTES);
    expect(transport.invoke.mock.calls[2][1].request.payload.text).toBe(TEXT);
  });

  it("preserves daemon error messages and conflict/durability metadata without adding file contents", async () => {
    const client = createTauriIpcClient();
    for (const code of ["file_conflict", "file_durability_uncertain"]) {
      const error = {
        code,
        message: "Refresh this file before retrying.",
        action: "Read the current revision.",
        details: { currentRevision: NEXT_REVISION, writeApplied: code === "file_durability_uncertain" },
      };
      transport.invoke.mockImplementation(async (_command: string, args: { request: RequestEnvelope<unknown> }) => ({
        kind: "response", version: 1, requestId: args.request.requestId, status: "error", error,
      }));
      try {
        await client.writeFile(WRITE);
        expect.fail("a failed save must not become an acknowledgement");
      } catch (caught) {
        expect(caught).toBeInstanceOf(IpcError);
        expect(caught).toMatchObject(error);
        expect(JSON.stringify(caught)).not.toContain(TEXT);
      }
    }
    expect(transport.invoke).toHaveBeenCalledTimes(2);
  });

  it.each([
    ["noncanonical base64", { pathBase64: "YQ" }],
    ["nonzero padding bits", { pathBase64: "YR==" }],
    ["traversal", { pathBase64: globalThis.btoa("../private") }],
    ["absolute path", { pathBase64: globalThis.btoa("/private") }],
    ["empty path", { pathBase64: "" }],
    ["different file", { pathBase64: globalThis.btoa("another.txt") }],
    ["oversized path", { pathBase64: globalThis.btoa("a".repeat(4_097)) }],
    ["revision", { revision: `v1:${"A".repeat(64)}` }],
    ["NUL text", { text: "private-file-body\0", sizeBytes: 18 }],
    ["unpaired surrogate", { text: "\ud800", sizeBytes: 3 }],
    ["UTF-8 byte bound", { text: "é".repeat(65_537), sizeBytes: 131_074 }],
    ["byte count mismatch", { sizeBytes: 1 }],
    ["fractional byte count", { sizeBytes: 1.5 }],
    ["null modification time", { modifiedAtMs: null }],
    ["unsafe timestamp", { observedAtMs: Number.MAX_SAFE_INTEGER + 1 }],
  ])("rejects a malformed read response: %s", async (_label, change) => {
    respond({ ...READ_RESPONSE, ...change });
    await expect(createTauriIpcClient().readFile(READ)).rejects.toMatchObject({ code: "invalid_ipc_payload" });
  });

  it("contract errors never quote malformed field values or file text", async () => {
    const secret = "private-file-body-not-for-diagnostics";
    respond({ ...READ_RESPONSE, text: `${secret}\0`, sizeBytes: secret.length + 1 });
    try {
      await createTauriIpcClient().readFile(READ);
      expect.fail("NUL text must be rejected");
    } catch (error) {
      expect(error).toBeInstanceOf(IpcError);
      expect(String(error)).not.toContain(secret);
      expect(JSON.stringify(error)).not.toContain(secret);
    }
  });

  it("accepts the exact 128 KiB UTF-8 boundary including worst-case JSON escapes", async () => {
    const text = "\u0001".repeat(128 * 1_024);
    respond({ ...READ_RESPONSE, text, sizeBytes: 128 * 1_024 });
    const response = await createTauriIpcClient().readFile(READ);
    expect(response.text).toBe(text);
    expect(response.sizeBytes).toBe(128 * 1_024);
  });

  it.each([
    ["another path", { pathBase64: globalThis.btoa("different.txt") }],
    ["different byte count", { sizeBytes: WRITE_RESPONSE.sizeBytes + 1 }],
    ["unbounded byte count", { sizeBytes: 131_073 }],
    ["invalid revision", { revision: "invalid" }],
    ["negative publication time", { writtenAtMs: -1 }],
  ])("refuses an inconsistent save acknowledgement: %s", async (_label, change) => {
    respond({ ...WRITE_RESPONSE, ...change });
    await expect(createTauriIpcClient().writeFile(WRITE)).rejects.toMatchObject({ code: "invalid_ipc_payload" });
  });

  it("ignores future entry kinds conservatively and keeps byte ordering independent of display names", async () => {
    const entries = [
      { pathBase64: globalThis.btoa("\x80"), displayName: "z", kind: "future_mount" },
      { pathBase64: globalThis.btoa("\xff"), displayName: "a", kind: "symlink" },
    ];
    respond({ entries, observedAtMs: NOW });
    const result = await createTauriIpcClient().listFiles(LIST);
    expect(result.entries).toEqual([{ ...entries[0], kind: "other" }, entries[1]]);
    expect(result).not.toHaveProperty("nextAfterNameBase64");
    expect(result.entries[0]).not.toHaveProperty("sizeBytes");
  });

  it.each([
    ["duplicate identifier", { entries: [ENTRY, ENTRY], observedAtMs: NOW }],
    ["nonchild path", { entries: [{ ...ENTRY, pathBase64: globalThis.btoa("nested/file.txt") }], observedAtMs: NOW }],
    ["empty page with cursor", { entries: [], nextAfterNameBase64: PATH, observedAtMs: NOW }],
    ["cursor on another name", { entries: [ENTRY], nextAfterNameBase64: globalThis.btoa("other"), observedAtMs: NOW }],
    ["directory cursor", { entries: [ENTRY], nextAfterNameBase64: globalThis.btoa("nested/file"), observedAtMs: NOW }],
    ["null cursor", { entries: [ENTRY], nextAfterNameBase64: null, observedAtMs: NOW }],
    ["unsafe entry size", { entries: [{ ...ENTRY, sizeBytes: Number.MAX_SAFE_INTEGER + 1 }], observedAtMs: NOW }],
    ["raw display control", { entries: [{ ...ENTRY, displayName: "line\nname" }], observedAtMs: NOW }],
    ["unrecognized nonstring kind", { entries: [{ ...ENTRY, kind: null }], observedAtMs: NOW }],
  ])("rejects a malformed file listing: %s", async (_label, page) => {
    respond(page);
    await expect(createTauriIpcClient().listFiles(LIST)).rejects.toMatchObject({ code: "invalid_ipc_payload" });
  });

  it("rejects a page before its exclusive cursor and checks both entry and encoded-byte limits", async () => {
    const client = createTauriIpcClient();
    respond({ entries: [ENTRY], observedAtMs: NOW });
    await expect(client.listFiles({ ...LIST, afterNameBase64: PATH })).rejects.toMatchObject({ code: "invalid_ipc_payload" });

    const entries = Array.from({ length: 201 }, (_, index) => ({
      pathBase64: globalThis.btoa(index.toString().padStart(3, "0")), displayName: "file", kind: "file",
    }));
    respond({ entries, observedAtMs: NOW });
    await expect(client.listFiles({ ...LIST, limit: 200 })).rejects.toMatchObject({ code: "invalid_ipc_payload" });

    const directory = "d".repeat(4_000);
    respond({ entries: entries.slice(0, 100).map((entry) => ({
      ...entry, pathBase64: globalThis.btoa(`${directory}/${globalThis.atob(entry.pathBase64)}`),
    })), observedAtMs: NOW });
    await expect(client.listFiles({ ...LIST, pathBase64: globalThis.btoa(directory) })).rejects.toMatchObject({ code: "invalid_ipc_payload" });
  });

  it("the injected mock rejects unconfigured editor calls and permits explicit handlers", async () => {
    const unhandled = createMockIpcClient();
    await expect(unhandled.listFiles(LIST)).rejects.toThrow("Unexpected IPC call in test: listFiles");
    await expect(unhandled.readFile(READ)).rejects.toThrow("Unexpected IPC call in test: readFile");
    await expect(unhandled.writeFile(WRITE)).rejects.toThrow("Unexpected IPC call in test: writeFile");
    const configured = createMockIpcClient({ handlers: { readFile: async () => READ_RESPONSE } });
    await expect(configured.readFile(READ)).resolves.toEqual(READ_RESPONSE);
    expect(configured.readFile).toHaveBeenCalledWith(READ);
  });
});
