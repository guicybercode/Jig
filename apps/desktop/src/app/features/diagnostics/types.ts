import type { DiagnosticsResponse } from "../../../ipc";

export type { DiagnosticsResponse };

/** Loads the typed `diagnostics.get` response through the project IPC client. */
export type DiagnosticsLoader = () => Promise<DiagnosticsResponse>;
