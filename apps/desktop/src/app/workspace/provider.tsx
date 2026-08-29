import { useEffect, useMemo, type ReactNode } from "react";

import {
  createDisconnectedTransport,
  createIpcClient,
  createTauriTransport,
  isTauriRuntime,
  type IpcClient,
} from "../../ipc";
import { WorkspaceRuntimeContext } from "./context";
import { createWorkspaceController } from "./controller";
import { createWorkspaceStore } from "./store";
import { createTerminalRegistry } from "./terminal-registry";
import { createInitialWorkspaceState } from "./types";

interface WorkspaceProviderProps {
  readonly children: ReactNode;
  readonly client?: IpcClient;
}

/**
 * Owns the workspace store and the single project IPC client.
 *
 * Tests inject a mock client. Production uses the Tauri transport when present.
 * Browser-only renders stay disconnected so the empty shell does not flash.
 */
export function WorkspaceProvider({
  children,
  client,
}: WorkspaceProviderProps) {
  const resolvedClient = useMemo(() => {
    if (client) {
      return client;
    }
    if (isTauriRuntime()) {
      return createIpcClient(createTauriTransport());
    }
    return null;
  }, [client]);

  const store = useMemo(
    () =>
      createWorkspaceStore(
        createInitialWorkspaceState(resolvedClient ? "loading" : "disconnected"),
      ),
    [resolvedClient],
  );
  const terminals = useMemo(() => createTerminalRegistry(), []);
  const actions = useMemo(() => {
    if (!resolvedClient) {
      return createWorkspaceController({
        client: createIpcClient(createDisconnectedTransport()),
        store,
        terminals,
      });
    }
    return createWorkspaceController({
      client: resolvedClient,
      store,
      terminals,
    });
  }, [resolvedClient, store, terminals]);

  useEffect(() => {
    if (!resolvedClient) {
      return undefined;
    }
    return actions.start();
  }, [actions, resolvedClient]);

  const runtime = useMemo(
    () => ({ store, actions, terminals }),
    [actions, store, terminals],
  );

  return (
    <WorkspaceRuntimeContext.Provider value={runtime}>
      {children}
    </WorkspaceRuntimeContext.Provider>
  );
}
