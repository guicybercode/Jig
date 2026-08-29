import type { DiagnosticsLoader, DiagnosticsReport } from "./types";

/** Loads a sanitized diagnostics snapshot from the desktop bridge. */
export const loadNativeDiagnostics: DiagnosticsLoader = async () => {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<DiagnosticsReport>("diagnostics_get");
};

/** Loads a copy-paste diagnostics bundle from the desktop bridge. */
export async function exportNativeDiagnostics(): Promise<string> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string>("diagnostics_export");
}
