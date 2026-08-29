import { AppShell } from "./app/AppShell";
import { AppErrorBoundary } from "./app/components/AppErrorBoundary";
import { WorkspaceProvider } from "./app/state/WorkspaceContext";
import type { WorkspaceView } from "./app/state/WorkspaceContext";
import type { IpcClient } from "./ipc/client";
import "./styles/index.css";

interface AppProps {
  /** Tests inject a strict IPC fake; production always uses the Tauri client. */
  readonly client?: IpcClient;
  readonly initialView?: WorkspaceView;
}

/** Renders the Jig desktop application. */
export function App({ client, initialView }: AppProps) {
  return (
    <AppErrorBoundary>
      <WorkspaceProvider client={client} initialView={initialView}>
        <AppShell />
      </WorkspaceProvider>
    </AppErrorBoundary>
  );
}
