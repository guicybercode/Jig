import type {
  KnowledgeDiscoverResponse,
  KnowledgeDiscoveryIssue,
  KnowledgeReadResponse,
  KnowledgeSourceEntry,
} from "./domain";
import { KNOWLEDGE_BODY_BYTES } from "./knowledge-schema";
import { IpcContractError, requireArray, requireBoolean, requireRecord, requireString } from "./schema";

/** A discovery capability must be a UUIDv7, without echoing an invalid value. */
function capability(value: unknown): string {
  const identifier = requireString(value, "discovery capability");
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(identifier)) {
    throw new IpcContractError("Invalid discovery capability");
  }
  return identifier;
}

/** Reads display-only metadata and rejects unsupported native-provider variants. */
export function decodeKnowledgeSourceEntry(value: unknown): KnowledgeSourceEntry {
  const entry = requireRecord(value, "discovery source");
  if (entry.kind !== "rule" && entry.kind !== "skill") {
    throw new IpcContractError("Invalid discovery source kind");
  }
  if (entry.provider !== "codex" && entry.provider !== "claude" && entry.provider !== "cursor") {
    throw new IpcContractError("Invalid discovery provider");
  }
  if (entry.scope !== "global" && entry.scope !== "project" && entry.scope !== "admin") {
    throw new IpcContractError("Invalid discovery scope");
  }
  if (
    entry.availability !== "available" && entry.availability !== "too_large" &&
    entry.availability !== "symlink" && entry.availability !== "non_regular" &&
    entry.availability !== "unreadable"
  ) {
    throw new IpcContractError("Invalid discovery availability");
  }
  return {
    entryId: capability(entry.entryId),
    kind: entry.kind,
    provider: entry.provider,
    scope: entry.scope,
    sourcePath: requireString(entry.sourcePath, "discovery source path"),
    name: requireString(entry.name, "discovery source name"),
    scopeDirectory: requireString(entry.scopeDirectory, "discovery scope directory"),
    precedenceHint: requireString(entry.precedenceHint, "discovery precedence hint"),
    viaSymlink: requireBoolean(entry.viaSymlink, "discovery symlink flag"),
    availability: entry.availability,
  };
}

/** Keeps partial-result explanations distinct from successfully read sources. */
function decodeIssue(value: unknown): KnowledgeDiscoveryIssue {
  const issue = requireRecord(value, "discovery issue");
  return {
    code: requireString(issue.code, "discovery issue code"),
    sourcePath: issue.sourcePath === null ? null : requireString(issue.sourcePath, "discovery issue path"),
    message: requireString(issue.message, "discovery issue message"),
  };
}

/** Enforces the bounded, uniquely selectable source inventory. */
export function decodeKnowledgeDiscoverResponse(value: unknown): KnowledgeDiscoverResponse {
  const response = requireRecord(value, "discovery response");
  const entries = requireArray(response.entries, "discovery sources");
  const issues = requireArray(response.issues, "discovery issues");
  if (entries.length > 512) throw new IpcContractError("Discovery source limit exceeded");
  if (issues.length > 32) throw new IpcContractError("Discovery issue limit exceeded");
  const sources = entries.map(decodeKnowledgeSourceEntry);
  if (new Set(sources.map((entry) => entry.entryId)).size !== sources.length) {
    throw new IpcContractError("Duplicate discovery source capability");
  }
  return {
    scanId: capability(response.scanId),
    entries: sources,
    truncated: requireBoolean(response.truncated, "discovery truncation flag"),
    issues: issues.map(decodeIssue),
  };
}

/** Raw source text is allowed to be empty; it is never parsed as HTML or a command. */
export function decodeKnowledgeReadResponse(value: unknown): KnowledgeReadResponse {
  const response = requireRecord(value, "discovery read response");
  const entry = decodeKnowledgeSourceEntry(response.entry);
  const content = requireString(response.content, "discovery source content");
  if (content.includes("\0")) {
    throw new IpcContractError("Invalid discovery source text");
  }
  if (entry.availability !== "available") {
    throw new IpcContractError("Unavailable discovery source returned content");
  }
  if (new TextEncoder().encode(content).byteLength > KNOWLEDGE_BODY_BYTES) {
    throw new IpcContractError("Discovery content limit exceeded");
  }
  return { entry, content };
}
