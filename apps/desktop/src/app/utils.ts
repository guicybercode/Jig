import type { ApiErrorData, SessionStatus } from "../ipc/types";
import { IpcError, toIpcError } from "../ipc/client";

/** Formats a timestamp compactly while preserving the exact value in `title`. */
export function formatActivityTime(timestampMs: number): string {
  const elapsedMs = Math.max(0, Date.now() - timestampMs);
  const elapsedMinutes = Math.floor(elapsedMs / 60_000);
  if (elapsedMinutes < 1) {
    return "Just now";
  }
  if (elapsedMinutes < 60) {
    return `${elapsedMinutes}m ago`;
  }
  const elapsedHours = Math.floor(elapsedMinutes / 60);
  if (elapsedHours < 24) {
    return `${elapsedHours}h ago`;
  }
  const elapsedDays = Math.floor(elapsedHours / 24);
  if (elapsedDays < 7) {
    return `${elapsedDays}d ago`;
  }
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    year:
      new Date(timestampMs).getFullYear() === new Date().getFullYear()
        ? undefined
        : "numeric",
  }).format(timestampMs);
}

export function toDateTime(timestampMs: number): string {
  return new Date(timestampMs).toISOString();
}

/** Generates a readable branch suggestion; the daemon remains authoritative. */
export function suggestBranch(sessionName: string): string {
  const slug = sessionName
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);
  return `agent/${slug || "new-session"}`;
}

/** Linux and macOS paths passed to the daemon must be absolute. */
export function isAbsoluteLocalPath(path: string): boolean {
  return path.startsWith("/") && !path.includes("\0");
}

export function isLiveStatus(status: SessionStatus): boolean {
  return status === "starting" || status === "running" || status === "idle";
}

export function errorData(error: unknown): ApiErrorData {
  const normalized = toIpcError(error);
  return {
    code: normalized.code,
    message: normalized.message,
    action: normalized.action,
    details: normalized.details,
  };
}

export async function copyText(value: string): Promise<void> {
  if (!navigator.clipboard?.writeText) {
    throw new IpcError({
      code: "clipboard_unavailable",
      message: "Clipboard access is not available in this desktop view.",
      action: "Select the working-directory path and copy it manually.",
    });
  }
  try {
    await navigator.clipboard.writeText(value);
  } catch (error) {
    throw new IpcError({
      code: "clipboard_write_failed",
      message: "The working-directory path could not be copied.",
      action: "Select the path and copy it manually.",
      details: {
        reason: error instanceof Error ? error.message : String(error),
      },
    });
  }
}
