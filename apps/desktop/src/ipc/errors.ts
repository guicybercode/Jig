import type { ApiError } from "./domain";

/** Raised when a typed IPC request fails with a daemon `ApiError`. */
export class IpcRequestError extends Error {
  readonly error: ApiError;

  constructor(error: ApiError) {
    super(error.action ? `${error.message} ${error.action}` : error.message);
    this.name = "IpcRequestError";
    this.error = error;
  }
}

/** The daemon socket is not available from this process. */
export const disconnectedError: ApiError = {
  code: "DAEMON_UNAVAILABLE",
  message: "The local daemon is not connected.",
  action: "Open CLI Master on this machine so cli-masterd can start.",
};

/** Handshake reported a protocol major version this UI does not speak. */
export const incompatibleProtocolError: ApiError = {
  code: "PROTOCOL_INCOMPATIBLE",
  message: "This app cannot speak the daemon protocol version.",
  action: "Update CLI Master so the desktop and daemon versions match.",
};

/** Returns whether `value` is a daemon API error without inspecting SQL. */
export function isApiError(value: unknown): value is ApiError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    typeof (value as ApiError).code === "string" &&
    typeof (value as ApiError).message === "string"
  );
}

/** Formats an error for the status bar or a dialog. Never interpolates SQL. */
export function formatApiError(error: unknown): string {
  if (error instanceof IpcRequestError) {
    return error.message;
  }
  if (isApiError(error)) {
    return error.action ? `${error.message} ${error.action}` : error.message;
  }
  if (error instanceof Error && error.message.length > 0) {
    return error.message;
  }
  return "The local daemon request failed.";
}

/** Converts unknown failures into a safe `ApiError`. */
export function toApiError(error: unknown): ApiError {
  if (error instanceof IpcRequestError) {
    return error.error;
  }
  if (isApiError(error)) {
    return error;
  }
  return {
    code: "DAEMON_REQUEST_FAILED",
    message: formatApiError(error),
  };
}
