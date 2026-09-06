import type {
  FileEntry,
  FileListRequest,
  FileListResponse,
  FileReadRequest,
  FileReadResponse,
  FileWriteRequest,
  FileWriteResponse,
} from "./domain";
import {
  IpcContractError,
  requireArray,
  requireRecord,
  requireString,
} from "./schema";

const MAX_PATH_BYTES = 4_096;
const MAX_TEXT_BYTES = 128 * 1_024;
const MAX_PAGE_BYTES = 512 * 1_024;
const MAX_PAGE_ENTRIES = 200;
const encoder = new TextEncoder();

/** Decode exact Unix bytes as a binary string, never as a display filename. */
function pathBytes(value: unknown, allowRoot = false, singleName = false): string {
  const encoded = requireString(value, "file path identifier");
  if (encoded.length > Math.ceil(MAX_PATH_BYTES / 3) * 4) {
    throw new IpcContractError("File path identifier exceeds its byte limit");
  }
  let bytes: string;
  try {
    bytes = globalThis.atob(encoded);
  } catch {
    throw new IpcContractError("File path identifier is not canonical base64");
  }
  if (globalThis.btoa(bytes) !== encoded || bytes.length > MAX_PATH_BYTES) {
    throw new IpcContractError("File path identifier is not canonical base64");
  }
  if (allowRoot && bytes === "") return bytes;
  if (
    bytes.includes("\0") ||
    (singleName && bytes.includes("/")) ||
    bytes.split("/").some((component) =>
      component === "" || component === "." || component === ".."
    )
  ) {
    throw new IpcContractError("File path identifier must name a relative child");
  }
  return bytes;
}

/** Reject values that cannot be represented exactly by the JavaScript client. */
function integer(value: unknown, label: string, minimum = 0, maximum = Number.MAX_SAFE_INTEGER): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new IpcContractError(`${label} must be a bounded safe integer`);
  }
  return value;
}

/** File modification times may precede 1970; absent times stay omitted. */
function modifiedTime(row: Record<string, unknown>): { readonly modifiedAtMs?: number } {
  return row.modifiedAtMs === undefined ? {} : {
    modifiedAtMs: integer(row.modifiedAtMs, "file modification timestamp", Number.MIN_SAFE_INTEGER),
  };
}

/** Keep Rust UTF-8 strings exact rather than repairing lone UTF-16 surrogates. */
function isUnicodeScalarText(text: string): boolean {
  for (let index = 0; index < text.length; index += 1) {
    const unit = text.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = text.charCodeAt(index + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return false;
      index += 1;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      return false;
    }
  }
  return true;
}

/** Validate text size and encoding without ever echoing the file contents. */
function textBytes(value: unknown): { readonly text: string; readonly sizeBytes: number } {
  const text = requireString(value, "file text");
  if (text.length > MAX_TEXT_BYTES || text.includes("\0") || !isUnicodeScalarText(text)) {
    throw new IpcContractError("File text exceeds its limit or is not valid UTF-8 text");
  }
  const sizeBytes = encoder.encode(text).byteLength;
  if (sizeBytes > MAX_TEXT_BYTES) {
    throw new IpcContractError("File text exceeds its UTF-8 byte limit");
  }
  return { text, sizeBytes };
}

/** Revisions are opaque; the frontend validates their versioned wire shape. */
function revision(value: unknown): string {
  const revision = requireString(value, "file revision");
  if (!/^v1:[0-9a-f]{64}$/.test(revision)) {
    throw new IpcContractError("File revision has an invalid format");
  }
  return revision;
}

/** Decode an observed entry without inferring access rights from its kind. */
function entry(value: unknown): FileEntry {
  const row = requireRecord(value, "file entry");
  const pathBase64 = requireString(row.pathBase64, "file entry path");
  pathBytes(pathBase64);
  const displayName = requireString(row.displayName, "file display name");
  if (
    displayName.length === 0 || displayName.length > MAX_PATH_BYTES * 4 ||
    /\p{Cc}/u.test(displayName) || !isUnicodeScalarText(displayName) ||
    encoder.encode(displayName).byteLength > MAX_PATH_BYTES * 4
  ) {
    throw new IpcContractError("File display name is not bounded escaped text");
  }
  const observedKind = requireString(row.kind, "file entry kind");
  const kind = observedKind === "file" || observedKind === "directory" || observedKind === "symlink"
    ? observedKind : "other";
  return {
    pathBase64,
    displayName,
    kind,
    ...(row.sizeBytes === undefined ? {} : { sizeBytes: integer(row.sizeBytes, "file entry size") }),
    ...modifiedTime(row),
  };
}

/** Validate byte ordering, direct children, continuation and the encoded page limit. */
export function decodeFileListResponse(value: unknown, input: FileListRequest): FileListResponse {
  const row = requireRecord(value, "file list response");
  const rawEntries = requireArray(row.entries, "file entries");
  const limit = integer(input.limit ?? 100, "file page limit", 1, MAX_PAGE_ENTRIES);
  if (rawEntries.length > limit) throw new IpcContractError("File page exceeds its entry limit");
  const directory = pathBytes(input.pathBase64, true);
  let previous = input.afterNameBase64 === undefined ? undefined : pathBytes(input.afterNameBase64, false, true);
  const entries = rawEntries.map((value) => {
    const decoded = entry(value);
    const bytes = pathBytes(decoded.pathBase64);
    const slash = bytes.lastIndexOf("/");
    const parent = slash < 0 ? "" : bytes.slice(0, slash);
    const name = bytes.slice(slash + 1);
    if (parent !== directory || (previous !== undefined && name <= previous)) {
      throw new IpcContractError("File page contains an unrelated, duplicate or unordered entry");
    }
    previous = name;
    return decoded;
  });
  const next = row.nextAfterNameBase64;
  if (next !== undefined && (entries.length === 0 || pathBytes(next, false, true) !== previous)) {
    throw new IpcContractError("File page continuation does not identify its final entry");
  }
  const response: FileListResponse = {
    entries,
    ...(next === undefined ? {} : { nextAfterNameBase64: requireString(next, "file continuation") }),
    observedAtMs: integer(row.observedAtMs, "file observation timestamp"),
  };
  if (encoder.encode(JSON.stringify(response)).byteLength > MAX_PAGE_BYTES) {
    throw new IpcContractError("File page exceeds its encoded response limit");
  }
  return response;
}

/** Validate the exact content and identity returned for the requested file. */
export function decodeFileReadResponse(value: unknown, input: FileReadRequest): FileReadResponse {
  const row = requireRecord(value, "file read response");
  const pathBase64 = requireString(row.pathBase64, "file read path");
  pathBytes(pathBase64);
  if (pathBase64 !== input.pathBase64) throw new IpcContractError("File read returned another identifier");
  const content = textBytes(row.text);
  const sizeBytes = integer(row.sizeBytes, "file text size", 0, MAX_TEXT_BYTES);
  if (sizeBytes !== content.sizeBytes) throw new IpcContractError("File size does not match its UTF-8 text");
  return {
    pathBase64,
    text: content.text,
    revision: revision(row.revision),
    sizeBytes,
    ...modifiedTime(row),
    observedAtMs: integer(row.observedAtMs, "file observation timestamp"),
  };
}

/** A save acknowledgement must identify the exact path and submitted byte count. */
export function decodeFileWriteResponse(value: unknown, input: FileWriteRequest): FileWriteResponse {
  const row = requireRecord(value, "file write response");
  const pathBase64 = requireString(row.pathBase64, "file write path");
  pathBytes(pathBase64);
  if (pathBase64 !== input.pathBase64) throw new IpcContractError("File save returned another identifier");
  const sizeBytes = integer(row.sizeBytes, "saved file size", 0, MAX_TEXT_BYTES);
  if (sizeBytes !== textBytes(input.text).sizeBytes) throw new IpcContractError("File save size differs from the submitted text");
  return {
    pathBase64,
    revision: revision(row.revision),
    sizeBytes,
    ...modifiedTime(row),
    writtenAtMs: integer(row.writtenAtMs, "file publication timestamp"),
  };
}
