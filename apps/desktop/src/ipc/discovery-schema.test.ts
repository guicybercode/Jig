import { describe, expect, it } from "vitest";

import fixture from "../../../../protocol/fixtures/knowledge-discovery.json";
import { decodeKnowledgeDiscoverResponse, decodeKnowledgeReadResponse, decodeKnowledgeSourceEntry } from "./discovery-schema";

describe("discovery Rust/TypeScript contract", () => {
  it("decodes the shared scan and read fixture without changing provenance or content", () => {
    expect(decodeKnowledgeDiscoverResponse(fixture.scan)).toEqual(fixture.scan);
    expect(decodeKnowledgeReadResponse(fixture.read)).toEqual(fixture.read);
  });

  it("rejects unknown variants and malformed capabilities without echoing source values", () => {
    for (const field of ["kind", "provider", "scope", "availability", "entryId"]) {
      expect(() => decodeKnowledgeSourceEntry({ ...fixture.read.entry, [field]: "private document text" }))
        .toThrow(/^Invalid discovery /);
    }
    expect(() => decodeKnowledgeSourceEntry({ ...fixture.read.entry, viaSymlink: "true" })).toThrow();
    expect(() => decodeKnowledgeDiscoverResponse({ ...fixture.scan, scanId: "not-a-capability" })).toThrow("Invalid discovery capability");
  });

  it("rejects repeated entries and inventories beyond source or issue bounds", () => {
    expect(() => decodeKnowledgeDiscoverResponse({ ...fixture.scan, entries: [fixture.read.entry, fixture.read.entry] })).toThrow("Duplicate discovery source capability");
    expect(() => decodeKnowledgeDiscoverResponse({ ...fixture.scan, entries: Array.from({ length: 513 }, () => fixture.read.entry) })).toThrow("Discovery source limit exceeded");
    expect(() => decodeKnowledgeDiscoverResponse({ ...fixture.scan, issues: Array.from({ length: 33 }, () => ({ code: "partial", sourcePath: null, message: "Partial" })) })).toThrow("Discovery issue limit exceeded");
  });

  it("preserves truncation and explicit nullable issue paths", () => {
    const scan = { ...fixture.scan, truncated: true, issues: [{ code: "unsupported_scope", sourcePath: null, message: "Nested scopes are not included." }] };
    expect(decodeKnowledgeDiscoverResponse(scan)).toEqual(scan);
    expect(() => decodeKnowledgeDiscoverResponse({ ...scan, issues: [{ code: "partial", message: "Missing path field" }] })).toThrow();
    expect(() => decodeKnowledgeDiscoverResponse({ ...scan, truncated: "yes" })).toThrow();
  });

  it("keeps empty and markup-like source text literal while enforcing UTF-8 byte and NUL limits", () => {
    for (const content of ["", "<script>alert('text only')</script>", "é".repeat(32_768)]) {
      expect(decodeKnowledgeReadResponse({ ...fixture.read, content }).content).toBe(content);
    }
    expect(() => decodeKnowledgeReadResponse({ ...fixture.read, content: "é".repeat(32_769) })).toThrow("Discovery content limit exceeded");
    expect(() => decodeKnowledgeReadResponse({ ...fixture.read, content: "private text\0" })).toThrow("Invalid discovery source text");
    expect(() => decodeKnowledgeReadResponse({ ...fixture.read, entry: { ...fixture.read.entry, availability: "symlink" } })).toThrow("Unavailable discovery source returned content");
  });
});
