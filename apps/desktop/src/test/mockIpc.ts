import { vi, type Mock } from "vitest";

import type {
  AppPlatform,
  IpcClient,
  IpcEventHandler,
  Unsubscribe,
} from "../ipc/client";
import type { BootstrapResult, EventEnvelope } from "../ipc/types";

type IpcHandlerName = Exclude<
  keyof IpcClient,
  "platform" | "initialize" | "subscribe"
>;

export type MockIpcHandlers = Partial<Pick<IpcClient, IpcHandlerName>>;

export interface MockIpcClientOptions {
  readonly platform?: AppPlatform;
  readonly bootstrap?: BootstrapResult | Promise<BootstrapResult>;
  readonly initialize?: IpcClient["initialize"];
  readonly handlers?: MockIpcHandlers;
}

/** An injected IPC fake whose unconfigured application calls fail loudly. */
export interface MockIpcClient extends IpcClient {
  readonly listFiles: Mock<IpcClient["listFiles"]>;
  readonly readFile: Mock<IpcClient["readFile"]>;
  readonly writeFile: Mock<IpcClient["writeFile"]>;
  readonly listKnowledge: Mock<IpcClient["listKnowledge"]>;
  readonly saveKnowledge: Mock<IpcClient["saveKnowledge"]>;
  readonly deleteKnowledge: Mock<IpcClient["deleteKnowledge"]>;
  readonly initialize: Mock<IpcClient["initialize"]>;
  readonly subscribe: Mock<IpcClient["subscribe"]>;
  readonly subscribeTerminal: Mock<IpcClient["subscribeTerminal"]>;
  readonly writeTerminal: Mock<IpcClient["writeTerminal"]>;
  readonly resizeTerminal: Mock<IpcClient["resizeTerminal"]>;
  readonly addProject: Mock<IpcClient["addProject"]>;
  readonly renameProject: Mock<IpcClient["renameProject"]>;
  readonly removeProject: Mock<IpcClient["removeProject"]>;
  readonly createCustomAgent: Mock<IpcClient["createCustomAgent"]>;
  readonly createSession: Mock<IpcClient["createSession"]>;
  readonly startSession: Mock<IpcClient["startSession"]>;
  readonly stopSession: Mock<IpcClient["stopSession"]>;
  readonly restartSession: Mock<IpcClient["restartSession"]>;
  readonly renameSession: Mock<IpcClient["renameSession"]>;
  readonly deleteSession: Mock<IpcClient["deleteSession"]>;
  readonly getGitStatus: Mock<IpcClient["getGitStatus"]>;
  readonly prepareWorktreeRemoval: Mock<
    IpcClient["prepareWorktreeRemoval"]
  >;
  readonly removeWorktree: Mock<IpcClient["removeWorktree"]>;
  readonly listWorktrees: Mock<IpcClient["listWorktrees"]>;
  readonly getDiagnostics: Mock<IpcClient["getDiagnostics"]>;
  readonly openPath: Mock<IpcClient["openPath"]>;
  emit(event: string, payload?: unknown, sequence?: number): void;
  listenerCount(): number;
}

export const EMPTY_BOOTSTRAP: BootstrapResult = {
  hello: {
    protocolVersion: 1,
    daemonVersion: "0.1.0-test",
    instanceId: "daemon-test",
  },
  snapshot: {
    schemaVersion: 1,
    projects: [],
    agents: [],
    sessions: [],
    worktrees: [],
  },
  agentDetections: [],
};

/** Builds a strict spy-backed client and an in-memory daemon event stream. */
export function createMockIpcClient(
  options: MockIpcClientOptions = {},
): MockIpcClient {
  const listeners = new Set<IpcEventHandler>();
  const handlers = options.handlers ?? {};
  let eventSequence = 0;

  const initialize = vi.fn<IpcClient["initialize"]>(
    options.initialize ??
      (() => Promise.resolve(options.bootstrap ?? EMPTY_BOOTSTRAP)),
  );
  const subscribe = vi.fn<IpcClient["subscribe"]>(
    async (handler, _onError): Promise<Unsubscribe> => {
      listeners.add(handler);
      return () => listeners.delete(handler);
    },
  );

  return {
    platform: options.platform ?? "linux",
    listFiles: vi.fn<IpcClient["listFiles"]>(handlers.listFiles ?? (() => rejectUnhandled("listFiles"))),
    readFile: vi.fn<IpcClient["readFile"]>(handlers.readFile ?? (() => rejectUnhandled("readFile"))),
    writeFile: vi.fn<IpcClient["writeFile"]>(handlers.writeFile ?? (() => rejectUnhandled("writeFile"))),
    initialize,
    subscribe,
    subscribeTerminal: vi.fn<IpcClient["subscribeTerminal"]>(
      handlers.subscribeTerminal ??
        (async (_input, handler): Promise<Unsubscribe> => {
          listeners.add(handler);
          return () => listeners.delete(handler);
        }),
    ),
    writeTerminal: vi.fn<IpcClient["writeTerminal"]>(
      handlers.writeTerminal ?? (() => rejectUnhandled("writeTerminal")),
    ),
    resizeTerminal: vi.fn<IpcClient["resizeTerminal"]>(
      handlers.resizeTerminal ?? (() => rejectUnhandled("resizeTerminal")),
    ),
    addProject: vi.fn<IpcClient["addProject"]>(
      handlers.addProject ?? (() => rejectUnhandled("addProject")),
    ),
    renameProject: vi.fn<IpcClient["renameProject"]>(
      handlers.renameProject ?? (() => rejectUnhandled("renameProject")),
    ),
    removeProject: vi.fn<IpcClient["removeProject"]>(
      handlers.removeProject ?? (() => rejectUnhandled("removeProject")),
    ),
    createCustomAgent: vi.fn<IpcClient["createCustomAgent"]>(
      handlers.createCustomAgent ??
        (() => rejectUnhandled("createCustomAgent")),
    ),
    createSession: vi.fn<IpcClient["createSession"]>(
      handlers.createSession ?? (() => rejectUnhandled("createSession")),
    ),
    startSession: vi.fn<IpcClient["startSession"]>(
      handlers.startSession ?? (() => rejectUnhandled("startSession")),
    ),
    stopSession: vi.fn<IpcClient["stopSession"]>(
      handlers.stopSession ?? (() => rejectUnhandled("stopSession")),
    ),
    restartSession: vi.fn<IpcClient["restartSession"]>(
      handlers.restartSession ?? (() => rejectUnhandled("restartSession")),
    ),
    renameSession: vi.fn<IpcClient["renameSession"]>(
      handlers.renameSession ?? (() => rejectUnhandled("renameSession")),
    ),
    deleteSession: vi.fn<IpcClient["deleteSession"]>(
      handlers.deleteSession ?? (() => rejectUnhandled("deleteSession")),
    ),
    getGitStatus: vi.fn<IpcClient["getGitStatus"]>(
      handlers.getGitStatus ?? (() => rejectUnhandled("getGitStatus")),
    ),
    prepareWorktreeRemoval: vi.fn<IpcClient["prepareWorktreeRemoval"]>(
      handlers.prepareWorktreeRemoval ??
        (() => rejectUnhandled("prepareWorktreeRemoval")),
    ),
    listWorktrees: vi.fn<IpcClient["listWorktrees"]>(handlers.listWorktrees ?? (() => rejectUnhandled("listWorktrees"))),
    removeWorktree: vi.fn<IpcClient["removeWorktree"]>(
      handlers.removeWorktree ?? (() => rejectUnhandled("removeWorktree")),
    ),
    getDiagnostics: vi.fn<IpcClient["getDiagnostics"]>(
      handlers.getDiagnostics ?? (() => rejectUnhandled("getDiagnostics")),
    ),
    listKnowledge: vi.fn<IpcClient["listKnowledge"]>(handlers.listKnowledge ?? (() => rejectUnhandled("listKnowledge"))),
    saveKnowledge: vi.fn<IpcClient["saveKnowledge"]>(handlers.saveKnowledge ?? (() => rejectUnhandled("saveKnowledge"))),
    deleteKnowledge: vi.fn<IpcClient["deleteKnowledge"]>(handlers.deleteKnowledge ?? (() => rejectUnhandled("deleteKnowledge"))),
    openPath: vi.fn<IpcClient["openPath"]>(
      handlers.openPath ?? (() => rejectUnhandled("openPath")),
    ),
    emit(event, payload = {}, sequence) {
      const resolvedSequence = sequence ?? eventSequence + 1;
      eventSequence = Math.max(eventSequence, resolvedSequence);
      const envelope: EventEnvelope = {
        kind: "event",
        version: 1,
        event,
        sequence: resolvedSequence,
        payload,
      };
      for (const listener of [...listeners]) {
        listener(envelope);
      }
    },
    listenerCount: () => listeners.size,
  };
}

function rejectUnhandled(method: string): Promise<never> {
  return Promise.reject(
    new Error(`Unexpected IPC call in test: ${method}. Configure a handler.`),
  );
}
