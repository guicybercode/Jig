import type { ApiError } from "../../ipc";
import { disconnectedError, incompatibleProtocolError } from "../../ipc";
import type { ConnectionPhase, ConnectionState } from "./types";

/** Label shown in the status bar for the current daemon phase. */
export function connectionStatusLabel(connection: ConnectionState): string {
  switch (connection.phase) {
    case "loading":
      return "Connecting…";
    case "disconnected":
      return "Daemon unavailable";
    case "incompatible":
      return "Incompatible protocol";
    case "error":
      return connection.message ?? "Daemon error";
    case "ready":
      return "Daemon connected";
  }
}

/** Returns whether mutations and live queries may run. */
export function isDaemonReady(phase: ConnectionPhase): boolean {
  return phase === "ready";
}

/** Maps a failed hello/snapshot onto a shell phase. */
export function classifyConnectFailure(error: ApiError): ConnectionPhase {
  if (error.code === disconnectedError.code) {
    return "disconnected";
  }
  if (error.code === incompatibleProtocolError.code) {
    return "incompatible";
  }
  return "error";
}
