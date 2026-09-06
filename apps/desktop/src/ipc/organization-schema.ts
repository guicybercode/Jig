import type { OrganizationEntry, OrganizationGetRequest, OrganizationGetResponse, OrganizationSaveRequest, OrganizationTarget, OrganizationWorkflow } from "./domain";
import { IpcContractError, requireArray, requireBoolean, requireRecord, requireString } from "./schema";

/** Stable identity key shared by batch correlation and draft caches. */
export function organizationTargetKey(target: OrganizationTarget): string {
  return `${target.kind}:${target.id.toLowerCase()}`;
}

/** Validates catalog IDs without attaching the rejected payload to an error. */
function target(value: unknown): OrganizationTarget {
  const row = requireRecord(value, "organization target");
  if (row.kind !== "project" && row.kind !== "session") throw new IpcContractError("Invalid organization target kind");
  const id = requireString(row.id, "organization target ID");
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id)) throw new IpcContractError("Invalid organization target ID");
  return { kind: row.kind, id };
}

/** Workflow values are user labels rather than native runtime states. */
export function isOrganizationWorkflow(value: unknown): value is OrganizationWorkflow {
  return value === "backlog" || value === "in_progress" || value === "in_review" || value === "blocked" || value === "done";
}

function workflow(value: unknown, owner: OrganizationTarget): OrganizationWorkflow | null {
  if (owner.kind === "project" && value === null) return null;
  if (owner.kind === "session" && isOrganizationWorkflow(value)) return value;
  throw new IpcContractError("Organization workflow does not match its target");
}

function integer(value: unknown): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) throw new IpcContractError("Invalid organization revision or timestamp");
  return value;
}

/** Matches Rust's unwritten defaults and persisted-entry invariants. */
export function decodeOrganizationEntry(value: unknown): OrganizationEntry {
  const row = requireRecord(value, "organization entry");
  const owner = target(row.target);
  const pinned = requireBoolean(row.pinned, "organization pinned flag");
  const archived = requireBoolean(row.archived, "organization archived flag");
  const progress = workflow(row.workflow, owner);
  const revision = integer(row.revision);
  const updatedAtMs = row.updatedAtMs === null ? null : integer(row.updatedAtMs);
  if (revision === 0 ? pinned || archived || updatedAtMs !== null || (owner.kind === "session" && progress !== "backlog") : updatedAtMs === null) {
    throw new IpcContractError("Invalid organization default or saved entry");
  }
  return { target: owner, pinned, archived, workflow: progress, revision, updatedAtMs };
}

/** Validates the whole batch before sending a read to the daemon. */
export function validateOrganizationGet(input: OrganizationGetRequest): void {
  if (input.targets.length < 1 || input.targets.length > 100) throw new IpcContractError("Organization batches require 1 to 100 targets");
  const targets = input.targets.map(target);
  if (new Set(targets.map(organizationTargetKey)).size !== targets.length) throw new IpcContractError("Duplicate organization targets");
}

/** Correlates every response with its requested target in the original order. */
export function decodeOrganizationGetResponse(value: unknown, input: OrganizationGetRequest): OrganizationGetResponse {
  validateOrganizationGet(input);
  const row = requireRecord(value, "organization response");
  const rawEntries = requireArray(row.entries, "organization entries");
  if (rawEntries.length !== input.targets.length) {
    throw new IpcContractError("Organization response does not match requested targets");
  }
  const entries = rawEntries.map(decodeOrganizationEntry);
  if (entries.some((entry, index) => organizationTargetKey(entry.target) !== organizationTargetKey(input.targets[index]))) {
    throw new IpcContractError("Organization response does not match requested targets");
  }
  return { entries };
}

/** Validates a complete optimistic save before transmitting it. */
export function validateOrganizationSave(input: OrganizationSaveRequest): void {
  const owner = target(input.target);
  integer(input.expectedRevision);
  requireBoolean(input.pinned, "organization pinned flag");
  requireBoolean(input.archived, "organization archived flag");
  workflow(input.workflow, owner);
}

/** Prevents another target or different desired flags from being reported as saved. */
export function decodeOrganizationSaveResponse(value: unknown, input: OrganizationSaveRequest): OrganizationEntry {
  validateOrganizationSave(input);
  const entry = decodeOrganizationEntry(value);
  if (organizationTargetKey(entry.target) !== organizationTargetKey(input.target) || entry.revision !== input.expectedRevision + 1 || entry.pinned !== input.pinned || entry.archived !== input.archived || entry.workflow !== input.workflow) {
    throw new IpcContractError("Organization save response does not match the requested change");
  }
  return entry;
}
