import { useState } from "react";

import { AppHeader } from "./components/AppHeader";
import { DiagnosticsDialog } from "./features/diagnostics/DiagnosticsDialog";
import { loadNativeDiagnostics } from "./features/diagnostics/loadDiagnostics";
import { ProjectSidebar } from "./features/navigation/ProjectSidebar";
import { LocalStatusBar } from "./features/status/LocalStatusBar";
import { WorkspaceEmptyState } from "./features/workspace/WorkspaceEmptyState";

/** Composes the persistent desktop application regions. */
export function AppShell() {
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);

  return (
    <div className="app-shell">
      <a className="skip-link" href="#workspace">
        Skip to workspace
      </a>
      <AppHeader />
      <div className="app-shell__body">
        <ProjectSidebar />
        <WorkspaceEmptyState />
      </div>
      <LocalStatusBar
        onOpenDiagnostics={() => {
          setDiagnosticsOpen(true);
        }}
      />
      {diagnosticsOpen ? (
        <DiagnosticsDialog
          load={loadNativeDiagnostics}
          onClose={() => {
            setDiagnosticsOpen(false);
          }}
        />
      ) : null}
    </div>
  );
}
