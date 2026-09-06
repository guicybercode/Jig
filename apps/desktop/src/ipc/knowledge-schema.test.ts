import { describe, expect, it } from "vitest";
import catalog from "../../../../protocol/catalog.json";
import fixture from "../../../../protocol/fixtures/knowledge-page.json";
import { IPC_METHODS } from "./methods";
import { decodeKnowledgeEntry, decodeKnowledgePage } from "./knowledge-schema";

describe("knowledge Rust/TypeScript contract", () => {
  it("decodes the same fixture as the Rust contract, including explicit nulls", () => {
    expect(decodeKnowledgePage(fixture)).toEqual(fixture);
    expect(fixture.nextCursor).toBeNull();
    expect(fixture.entries[0].projectId).toBeNull();
  });

  it("mirrors every public method", () => {
    expect(IPC_METHODS).toEqual(catalog.methods);
  });

  it("rejects unknown variants without echoing text", () => {
    expect(() => decodeKnowledgeEntry({ ...fixture.entries[0], kind: "private content" })).toThrow("Invalid knowledge kind");
  });

  it("rejects unsafe revisions, text bounds and timestamps", () => {
    for (const change of [
      { revision: 0 }, { revision: Number.MAX_SAFE_INTEGER + 1 },
      { updatedAtMs: -1 }, { body: "\0" }, { title: "é".repeat(129) },
      { body: "a".repeat(65_537) },
    ]) expect(() => decodeKnowledgeEntry({ ...fixture.entries[0], ...change })).toThrow();
  });

  it("shares Rust Unicode whitespace validation", () => {
    expect(decodeKnowledgeEntry({ ...fixture.entries[0], body: "\uFEFF" }).body).toBe("\uFEFF");
    expect(() => decodeKnowledgeEntry({ ...fixture.entries[0], body: "\u0085" })).toThrow();
  });

  it("rejects a cursor that cannot advance and repeated rows", () => {
    expect(() => decodeKnowledgePage({ entries: [], nextCursor: fixture.entries[0].id })).toThrow();
    expect(() => decodeKnowledgePage({ entries: [fixture.entries[0], fixture.entries[0]], nextCursor: null })).toThrow();
  });
});
