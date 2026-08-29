import { AppShell } from "./app/AppShell";
import { AppErrorBoundary } from "./app/components/AppErrorBoundary";
import { WorkspaceProvider } from "./app/state/WorkspaceContext";
import type { IpcClient } from "./ipc/client";
import "./styles/index.css";

interface AppProps {
  /** Tests inject a strict IPC fake; production always uses the Tauri client. */
  readonly client?: IpcClient;
}

/** Renders the CLI Master desktop application. */
export function App({ client }: AppProps) {
  return (
    <AppErrorBoundary>
      <WorkspaceProvider client={client}>
        <AppShell />
      </WorkspaceProvider>
    </AppErrorBoundary>
  );
}
