import { describe, expect, it } from "vitest";

import fixture from "../../../../protocol/fixtures/organization.json";
import type { OrganizationGetRequest, OrganizationSaveRequest } from "./domain";
import { decodeOrganizationEntry, decodeOrganizationGetResponse, decodeOrganizationSaveResponse, validateOrganizationGet, validateOrganizationSave } from "./organization-schema";

const request: OrganizationGetRequest = { targets: fixture.response.entries.map((entry) => decodeOrganizationEntry(entry).target) };

describe("organization Rust/TypeScript contract", () => {
  it("decodes the shared defaults in requested order and preserves saved fields", () => {
    expect(request).toEqual(fixture.request);
    expect(decodeOrganizationGetResponse(fixture.response, request)).toEqual(fixture.response);
    expect(decodeOrganizationEntry(fixture.saved)).toEqual(fixture.saved);
  });

  it("requires exact default flags, workflow and explicit null timestamps at revision zero", () => {
    for (const patch of [{ pinned: true }, { archived: true }, { workflow: "done" }, { updatedAtMs: 0 }, { updatedAtMs: undefined }, { revision: -1 }]) {
      expect(() => decodeOrganizationEntry({ ...fixture.response.entries[0], ...patch })).toThrow();
    }
    expect(() => decodeOrganizationEntry({ ...fixture.response.entries[1], workflow: "backlog" })).toThrow();
    expect(() => decodeOrganizationEntry({ ...fixture.response.entries[0], workflow: null })).toThrow();
    expect(() => decodeOrganizationEntry({ ...fixture.response.entries[1], workflow: undefined })).toThrow();
  });

  it("rejects unsafe saved numbers and unrecognized variants without echoing payload values", () => {
    for (const patch of [{ revision: Number.MAX_SAFE_INTEGER + 1 }, { updatedAtMs: null }, { updatedAtMs: -1 }, { updatedAtMs: 0.5 }, { updatedAtMs: Number.MAX_SAFE_INTEGER + 1 }, { pinned: "yes" }, { workflow: "private text" }]) {
      expect(() => decodeOrganizationEntry({ ...fixture.saved, ...patch })).toThrow();
    }
    expect(() => decodeOrganizationEntry({ ...fixture.saved, target: { kind: "private text", id: fixture.saved.target.id } })).toThrow("Invalid organization target kind");
  });

  it("rejects empty, oversized and duplicate batches, including UUID case aliases", () => {
    const owner = request.targets[0];
    expect(() => validateOrganizationGet({ targets: [] })).toThrow();
    expect(() => validateOrganizationGet({ targets: Array.from({ length: 101 }, () => owner) })).toThrow();
    expect(() => validateOrganizationGet({ targets: [owner, owner] })).toThrow("Duplicate organization targets");
    expect(() => validateOrganizationGet({ targets: [owner, { ...owner, id: owner.id.toUpperCase() }] })).toThrow("Duplicate organization targets");
  });

  it("rejects missing or reordered response targets instead of attaching metadata to another entity", () => {
    expect(() => decodeOrganizationGetResponse({ entries: [] }, request)).toThrow("Organization response does not match requested targets");
    expect(() => decodeOrganizationGetResponse({ entries: [...fixture.response.entries].reverse() }, request)).toThrow("Organization response does not match requested targets");
  });

  it("correlates a saved response with all desired flags, the target and the next revision", () => {
    const saved = decodeOrganizationEntry(fixture.saved);
    const input: OrganizationSaveRequest = { target: saved.target, expectedRevision: 1, pinned: saved.pinned, archived: saved.archived, workflow: saved.workflow };
    expect(decodeOrganizationSaveResponse(fixture.saved, input)).toEqual(fixture.saved);
    for (const patch of [{ revision: 3 }, { pinned: false }, { archived: false }, { workflow: "done" }]) expect(() => decodeOrganizationSaveResponse({ ...fixture.saved, ...patch }, input)).toThrow();
    expect(() => validateOrganizationSave({ ...input, target: request.targets[1] })).toThrow();
  });
});
