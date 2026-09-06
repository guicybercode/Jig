import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import type { Ref } from "react";
import type { IpcClient } from "../../../ipc/client";

import type {
  AgentRecord,
  ApiErrorData,
  CreateCustomAgentInput,
  CreateSessionInput,
  Project,
  Session,
  Worktree,
} from "../../../ipc/types";
import { Icon } from "../../components/Icon";
import { StatusBadge } from "../../components/StatusBadge";
import { BrowserSurface } from "../browser/BrowserSurface";
import {
  defaultBrowserRuntime,
  type BrowserRuntime,
} from "../browser/browser-runtime";
import {
  createCanvasNode,
  createInitialCanvasDocument,
  createSessionTerminalCanvasNode,
  createTerminalCanvasNode,
  duplicateCanvasSelection,
  getCanvasNodeSize,
  normalizeBrowserUrl,
  type BrowserCanvasNode,
  type CanvasTerminalConfiguration,
  type CanvasNode,
  type NoteCanvasNode,
  type TerminalCanvasNode,
} from "./canvas-state";
import {
  appendBrowserUrlToNote,
  browserUrlForTerminal,
} from "./browser-handoff";
import { CanvasConnections } from "./CanvasConnections";
import { CanvasKnowledgePanel } from "./CanvasKnowledgePanel";
import type { KnowledgeInsertion } from "../knowledge/knowledge-types";
import { CanvasElementSearch } from "./CanvasElementSearch";
import {
  CANVAS_ORIGIN_X,
  CANVAS_ORIGIN_Y,
  INITIAL_VIEW_CENTER,
  toStagePoint,
} from "./canvas-geometry";
import { NewCanvasTerminalDialog } from "./NewCanvasTerminalDialog";
import { PromptComposer } from "./PromptComposer";
import type { PromptContextItem, PromptTerminalKey } from "./PromptComposer";
import { encodePromptInput, encodePromptTerminalKey, getPromptInputError } from "./prompt-input";
import { useCanvasState } from "./useCanvasState";
import {
  LiveTerminal,
  type LiveTerminalInputHandle,
  type LiveTerminalTransport,
} from "../terminal/LiveTerminal";
import type { TerminalInputModes } from "../terminal/terminal-runtime";
import { errorData, isLiveStatus } from "../../utils";

export interface CanvasSessionFocusRequest {
  readonly sessionId: string;
  readonly revision: number;
}

interface CanvasWorkspaceProps extends LiveTerminalTransport {
  readonly isCompact?: boolean;
  readonly isConnected: boolean;
  readonly projects: readonly Project[];
  readonly project?: Project;
  readonly agents: readonly AgentRecord[];
  readonly sessions: readonly Session[];
  readonly worktrees: readonly Worktree[];
  readonly selectedSessionId?: string;
  readonly sessionFocusRevision: number;
  readonly onSelectSession: (sessionId: string | null) => void;
  readonly onCreateCustomAgent: (
    input: CreateCustomAgentInput,
  ) => Promise<AgentRecord>;
  readonly onCreateSession: (input: CreateSessionInput) => Promise<Session>;
  readonly onStartSession: (sessionId: string) => Promise<Session>;
  readonly onRestartSession: (sessionId: string) => Promise<Session>;
  readonly onRenameSession: (sessionId: string) => void;
  readonly onStopSession: (sessionId: string) => void;
  readonly onDeleteSession: (sessionId: string) => void;
  readonly onRemoveWorktree: (worktreeId: string) => void;
  readonly onGitStatus: (sessionId: string) => void;
  readonly onOpenPath: (path: string) => Promise<void>;
  readonly browserRuntime?: BrowserRuntime;
  readonly knowledgeClient?: Pick<IpcClient, "listKnowledge" | "saveKnowledge" | "deleteKnowledge" | "discoverKnowledge" | "readKnowledge">;
  readonly knowledgeConnectionKey?: string;
  readonly knowledgeOpenRevision?: number;
}

const ZOOM_STEP = 0.1;
const COMPACT_TERMINAL_GUTTER_PX = 48;
const SCROLL_SETTLE_DELAY_MS = 160;

/** Spatial terminal and notes workspace inspired by the supplied references. */
export function CanvasWorkspace({
  isCompact = false,
  isConnected,
  projects,
  project,
  agents,
  sessions,
  worktrees,
  selectedSessionId,
  sessionFocusRevision,
  onSelectSession,
  onCreateCustomAgent,
  onCreateSession,
  onStartSession,
  onRestartSession,
  onRenameSession,
  onStopSession,
  onDeleteSession,
  onRemoveWorktree,
  onGitStatus,
  onOpenPath,
  browserRuntime = defaultBrowserRuntime,
  knowledgeClient,
  knowledgeConnectionKey,
  knowledgeOpenRevision = 0,
  subscribeTerminal,
  writeTerminal,
  resizeTerminal,
}: CanvasWorkspaceProps) {
  const { state, dispatch, persistenceAvailable } = useCanvasState();
  const viewportRef = useRef<HTMLDivElement>(null);
  const viewportWidth = useElementWidth(viewportRef);
  const viewportInitializedRef = useRef(false);
  const nodeElementsRef = useRef(new Map<string, HTMLElement>());
  const handledFocusRequestRef = useRef<CanvasSessionFocusRequest | null>(null);
  const focusSelectionRef = useRef(false);
  const layersTriggerRef = useRef<HTMLButtonElement>(null);
  const terminalInputsRef = useRef(new Map<string, LiveTerminalInputHandle>());
  const pendingComposerInputRef = useRef(new Set<string>());
  const canvasComposingRef = useRef(false);
  const handledKnowledgeRequestRef = useRef(0);
  const panRef = useRef<{
    readonly pointerId: number;
    readonly clientX: number;
    readonly clientY: number;
    readonly scrollLeft: number;
    readonly scrollTop: number;
  } | null>(null);
  const scrollSettleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const [layersOpen, setLayersOpen] = useState(false);
  const [composerOpen, setComposerOpen] = useState(false);
  const [knowledgeOpen, setKnowledgeOpen] = useState(false);
  const [knowledgeVisited, setKnowledgeVisited] = useState(false);
  const [pendingComposerNodes, setPendingComposerNodes] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const [terminalDialogOpen, setTerminalDialogOpen] = useState(false);
  const [pendingTerminals, setPendingTerminals] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const [terminalErrors, setTerminalErrors] = useState<
    Readonly<Record<string, string>>
  >({});
  const [canvasInteracting, setCanvasInteracting] = useState(false);
  const [canvasScrolling, setCanvasScrolling] = useState(false);
  const [browserHandoffStatus, setBrowserHandoffStatus] = useState<
    string | null
  >(null);
  const terminalSessions = useMemo(
    () => new Map(sessions.map((session) => [session.id, session])),
    [sessions],
  );
  const projectSessions = useMemo(
    () =>
      project
        ? sessions.filter((session) => session.projectId === project.id)
        : [],
    [project, sessions],
  );
  const hiddenSessionIds = useMemo(
    () => new Set(state.hiddenSessionIds),
    [state.hiddenSessionIds],
  );
  const storedVisibleNodes = useMemo(
    () =>
      state.nodes.filter((node) => {
        if (
          node.kind === "terminal" &&
          node.sessionId &&
          hiddenSessionIds.has(node.sessionId)
        ) {
          return false;
        }
        const session =
          node.kind === "terminal" && node.sessionId
            ? terminalSessions.get(node.sessionId)
            : undefined;
        const nodeProjectId = session?.projectId ?? node.projectId;
        return !nodeProjectId || nodeProjectId === project?.id;
      }),
    [hiddenSessionIds, project?.id, state.nodes, terminalSessions],
  );
  const visibleNodes = useMemo(
    () =>
      effectiveCanvasNodes(
        storedVisibleNodes,
        isCompact,
        viewportWidth,
        state.zoom,
      ),
    [isCompact, state.zoom, storedVisibleNodes, viewportWidth],
  );
  const storedVisibleNodesById = useMemo(
    () => new Map(storedVisibleNodes.map((node) => [node.id, node])),
    [storedVisibleNodes],
  );
  const visibleNodeIds = useMemo(
    () => new Set(visibleNodes.map((node) => node.id)),
    [visibleNodes],
  );
  const visibleConnections = useMemo(
    () =>
      state.connections.filter(
        (connection) =>
          visibleNodeIds.has(connection.sourceNodeId) &&
          visibleNodeIds.has(connection.targetNodeId),
      ),
    [state.connections, visibleNodeIds],
  );
  const visibleConnectionSourceId =
    state.connectionSourceId && visibleNodeIds.has(state.connectionSourceId)
      ? state.connectionSourceId
      : null;
  const selectedNode = visibleNodes.find(
    (node) => node.id === state.selectedNodeId,
  );
  const composerNode = composerOpen && selectedNode?.kind === "terminal"
    ? selectedNode
    : undefined;
  const composerSession = composerNode?.sessionId
    ? terminalSessions.get(composerNode.sessionId)
    : undefined;
  const composerDisabledReason = composerNode
    ? !isConnected
      ? "Reconnect to the daemon to send input. You can keep drafting."
      : !composerSession
        ? "Attach a running session to send input. You can keep drafting."
        : !isLiveStatus(composerSession.status)
          ? "This terminal is stopped. Start it to send input."
          : pendingComposerNodes.has(composerNode.id)
            ? "Input is being delivered to this terminal. You can keep drafting."
            : composerNode.promptDraft?.trim()
              ? getPromptInputError(composerNode.promptDraft)
              : undefined
    : undefined;
  const selectedNodeIds = state.selectedNodeIds.filter((id) =>
    visibleNodeIds.has(id),
  );
  const selectedConnections = selectedNode
    ? visibleConnections.flatMap((connection) => {
        const otherNodeId =
          connection.sourceNodeId === selectedNode.id
            ? connection.targetNodeId
            : connection.targetNodeId === selectedNode.id
              ? connection.sourceNodeId
              : null;
        const otherNode = visibleNodes.find((node) => node.id === otherNodeId);
        return otherNode ? [{ connection, otherNode }] : [];
      })
    : [];
  const composerContextItems: readonly PromptContextItem[] = composerNode
    ? selectedConnections.flatMap(({ otherNode }) => {
        if (otherNode.kind === "note") {
          return [{ id: otherNode.id, title: otherNode.title, text: otherNode.text }];
        }
        if (otherNode.kind === "browser") {
          const url = normalizeBrowserUrl(otherNode.url);
          return url ? [{ id: otherNode.id, title: `${otherNode.title} URL`, text: url }] : [];
        }
        return [];
      })
    : [];
  const registerTerminalInput = useCallback((nodeId: string, handle: LiveTerminalInputHandle | null) => {
    if (handle) terminalInputsRef.current.set(nodeId, handle);
    else terminalInputsRef.current.delete(nodeId);
  }, []);
  useLayoutEffect(() => {
    if (!knowledgeClient || knowledgeOpenRevision <= handledKnowledgeRequestRef.current) return;
    handledKnowledgeRequestRef.current = knowledgeOpenRevision;
    setKnowledgeVisited(true);
    setKnowledgeOpen(true);
    setLayersOpen(false);
  }, [knowledgeClient, knowledgeOpenRevision]);
  const sessionCanvasTopologyKey = useMemo(
    () =>
      JSON.stringify({
        attachedSessionIds: state.nodes.flatMap((node) =>
          node.kind === "terminal" && node.sessionId ? [node.sessionId] : [],
        ),
        hiddenSessionIds: state.hiddenSessionIds,
      }),
    [state.hiddenSessionIds, state.nodes],
  );
  const markCanvasScrolling = useCallback(() => {
    setCanvasScrolling(true);
    if (scrollSettleTimerRef.current !== null) {
      globalThis.clearTimeout(scrollSettleTimerRef.current);
    }
    scrollSettleTimerRef.current = globalThis.setTimeout(() => {
      scrollSettleTimerRef.current = null;
      setCanvasScrolling(false);
    }, SCROLL_SETTLE_DELAY_MS);
  }, []);
  const focusNode = useCallback(
    (node: CanvasNode) => {
      const viewport = viewportRef.current;
      onSelectSession(
        node.kind === "terminal" && node.sessionId ? node.sessionId : null,
      );
      dispatch({ type: "node/select", nodeId: node.id });
      setLayersOpen(false);
      nodeElementsRef.current.get(node.id)?.focus({ preventScroll: true });
      if (!viewport) {
        return;
      }

      const size = getCanvasNodeSize(node);
      markCanvasScrolling();
      viewport.scrollTo({
        left: Math.max(
          0,
          (CANVAS_ORIGIN_X + node.x + size.width / 2) * state.zoom -
            viewport.clientWidth / 2,
        ),
        top: Math.max(
          0,
          (CANVAS_ORIGIN_Y + node.y + size.height / 2) * state.zoom -
            viewport.clientHeight / 2,
        ),
        behavior: canvasScrollBehavior(),
      });
    },
    [dispatch, markCanvasScrolling, onSelectSession, state.zoom],
  );

  useLayoutEffect(() => {
    // An offline empty session list is not evidence that saved sessions vanished.
    if (!isConnected) return;
    dispatch({
      type: "sessions/reconcile",
      knownSessionIds: sessions.map((session) => session.id),
      sessionNodes: projectSessions.map((session, index) =>
        createSessionTerminalCanvasNode(
          reconciledSessionPosition(index),
          session,
        ),
      ),
    });
  }, [dispatch, isConnected, projectSessions, sessionCanvasTopologyKey, sessions]);

  useLayoutEffect(() => {
    const remainingSelection = state.selectedNodeIds.filter((id) =>
      visibleNodeIds.has(id),
    );
    if (remainingSelection.length !== state.selectedNodeIds.length) {
      dispatch({ type: "nodes/select", nodeIds: remainingSelection });
    }
    if (
      state.connectionSourceId !== null &&
      !visibleNodeIds.has(state.connectionSourceId)
    ) {
      dispatch({ type: "connection/cancel" });
    }
  }, [
    dispatch,
    state.connectionSourceId,
    state.selectedNodeIds,
    visibleNodeIds,
  ]);

  useLayoutEffect(() => {
    if (focusSelectionRef.current && state.selectedNodeId) {
      focusSelectionRef.current = false;
      nodeElementsRef.current
        .get(state.selectedNodeId)
        ?.focus({ preventScroll: true });
    }
  }, [state.selectedNodeId]);

  useLayoutEffect(() => {
    if (!selectedSessionId) {
      handledFocusRequestRef.current = null;
      return;
    }
    const request: CanvasSessionFocusRequest = {
      sessionId: selectedSessionId,
      revision: sessionFocusRevision,
    };
    const handledRequest = handledFocusRequestRef.current;
    if (
      handledRequest?.sessionId === request.sessionId &&
      handledRequest.revision === request.revision
    ) {
      return;
    }

    const session = terminalSessions.get(request.sessionId);
    if (!session || session.projectId !== project?.id) {
      return;
    }
    const existingNode = visibleNodes.find(
      (node) =>
        node.kind === "terminal" && node.sessionId === request.sessionId,
    );
    if (!existingNode || hiddenSessionIds.has(request.sessionId)) {
      const projectSessionIndex = Math.max(
        0,
        projectSessions.findIndex((candidate) => candidate.id === session.id),
      );
      dispatch({
        type: "session/reveal",
        node: createSessionTerminalCanvasNode(
          reconciledSessionPosition(projectSessionIndex),
          session,
        ),
      });
      return;
    }
    if (!nodeElementsRef.current.has(existingNode.id)) {
      return;
    }

    handledFocusRequestRef.current = request;
    focusNode(existingNode);
  }, [
    dispatch,
    focusNode,
    hiddenSessionIds,
    project?.id,
    projectSessions,
    selectedSessionId,
    sessionFocusRevision,
    terminalSessions,
    visibleNodes,
  ]);

  useLayoutEffect(
    () => () => {
      if (scrollSettleTimerRef.current !== null) {
        globalThis.clearTimeout(scrollSettleTimerRef.current);
        scrollSettleTimerRef.current = null;
      }
    },
    [],
  );

  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport || viewportInitializedRef.current) {
      return;
    }
    viewportInitializedRef.current = true;
    viewport.scrollLeft = Math.max(
      0,
      (CANVAS_ORIGIN_X + INITIAL_VIEW_CENTER.x) * state.zoom -
        viewport.clientWidth / 2,
    );
    viewport.scrollTop = Math.max(
      0,
      (CANVAS_ORIGIN_Y + INITIAL_VIEW_CENTER.y) * state.zoom -
        viewport.clientHeight / 2,
    );
  }, [state.zoom]);

  function nextNodePosition() {
    const viewport = viewportRef.current;
    const offset = (state.nodes.length % 6) * 32;
    return {
      x:
        ((viewport?.scrollLeft ?? 0) + 260) / state.zoom -
        CANVAS_ORIGIN_X +
        offset,
      y:
        ((viewport?.scrollTop ?? 0) + 150) / state.zoom -
        CANVAS_ORIGIN_Y +
        offset,
    };
  }

  function addNote() {
    const node = createCanvasNode("note", nextNodePosition());
    onSelectSession(null);
    dispatch({
      type: "node/add",
      node: project ? { ...node, projectId: project.id } : node,
    });
  }

  function addBrowser() {
    const node = createCanvasNode("browser", nextNodePosition());
    setBrowserHandoffStatus(null);
    onSelectSession(null);
    dispatch({
      type: "node/add",
      node: project ? { ...node, projectId: project.id } : node,
    });
  }

  function addTerminal(configuration: CanvasTerminalConfiguration) {
    const terminal = createTerminalCanvasNode(
      nextNodePosition(),
      configuration,
    );
    const node = project
      ? { ...terminal, projectId: project.id }
      : terminal;
    onSelectSession(null);
    dispatch({
      type: "node/add",
      node,
    });
    setTerminalDialogOpen(false);
    if (project && isConnected) {
      void launchTerminal(node);
    }
  }

  async function handoffBrowserUrl(
    browser: BrowserCanvasNode,
    target: NoteCanvasNode | TerminalCanvasNode,
  ) {
    if (target.kind === "note") {
      const text = appendBrowserUrlToNote(target.text, browser.url);
      if (text === target.text) {
        setBrowserHandoffStatus("Enter a valid browser address first.");
        return;
      }
      dispatch({ type: "note/update", nodeId: target.id, text });
      setBrowserHandoffStatus(`Added the browser URL to ${target.title}.`);
      return;
    }

    const session = terminalSessions.get(target.sessionId ?? "");
    const payload = browserUrlForTerminal(browser.url);
    if (!session || !isLiveStatus(session.status)) {
      setBrowserHandoffStatus(
        `Start ${target.title} before inserting the browser URL.`,
      );
      return;
    }
    if (!payload) {
      setBrowserHandoffStatus("Enter a valid browser address first.");
      return;
    }
    try {
      await writeTerminal(session.id, new TextEncoder().encode(payload));
      setBrowserHandoffStatus(
        `Inserted a shell-safe URL into ${target.title}. Review it before pressing Enter.`,
      );
    } catch {
      setBrowserHandoffStatus(
        `The browser URL could not be inserted into ${target.title}.`,
      );
    }
  }

  async function launchTerminal(node: TerminalCanvasNode, session?: Session) {
    if (!project) {
      setTerminalErrors((current) => ({
        ...current,
        [node.id]: "Select a project before starting this terminal.",
      }));
      return;
    }
    setPendingTerminals((current) => new Set(current).add(node.id));
    setTerminalErrors((current) => {
      const next = { ...current };
      delete next[node.id];
      return next;
    });
    try {
      if (session) {
        await onStartSession(session.id);
        return;
      }
      const agent = await resolveTerminalAgent(
        node,
        agents,
        onCreateCustomAgent,
      );
      const created = await onCreateSession({
        projectId: project.id,
        name: node.title,
        agentId: agent.id,
        isolation: node.isolation ?? "current",
        relativeDirectory: relativeWorkingDirectory(project, node.workingDirectory),
      });
      dispatch({
        type: "terminal/attach",
        nodeId: node.id,
        sessionId: created.id,
        projectId: created.projectId,
      });
      onSelectSession(created.id);
      await onStartSession(created.id);
    } catch (error) {
      setTerminalErrors((current) => ({
        ...current,
        [node.id]: terminalErrorMessage(error),
      }));
    } finally {
      setPendingTerminals((current) => {
        const next = new Set(current);
        next.delete(node.id);
        return next;
      });
    }
  }

  async function deliverComposerInput(
    node: TerminalCanvasNode,
    encode: (modes: TerminalInputModes) => Uint8Array,
  ) {
    const session = terminalSessions.get(node.sessionId ?? "");
    const input = terminalInputsRef.current.get(node.id);
    if (!isConnected || !session || !isLiveStatus(session.status) || !input) {
      throw new Error("This terminal is not available for input.");
    }
    if (pendingComposerInputRef.current.has(node.id)) {
      throw new Error("Input is already being delivered to this terminal.");
    }
    pendingComposerInputRef.current.add(node.id);
    setPendingComposerNodes(new Set(pendingComposerInputRef.current));
    try {
      await input.writeInput(encode);
    } finally {
      pendingComposerInputRef.current.delete(node.id);
      setPendingComposerNodes(new Set(pendingComposerInputRef.current));
    }
  }

  async function sendComposerPrompt(node: TerminalCanvasNode, text: string) {
    const revision = node.promptDraftRevision ?? 0;
    await deliverComposerInput(node, (modes) => encodePromptInput(text, modes));
    dispatch({ type: "terminal/draft_sent", nodeId: node.id, text, revision });
  }

  function sendComposerKey(node: TerminalCanvasNode, key: PromptTerminalKey) {
    return deliverComposerInput(node, (modes) => encodePromptTerminalKey(key, modes));
  }

  function toggleComposer(node: TerminalCanvasNode) {
    selectNode(node);
    setLayersOpen(false);
    setKnowledgeOpen(false);
    setComposerOpen((open) => !(open && selectedNode?.id === node.id));
  }

  function toggleKnowledge() {
    if (!knowledgeClient) return;
    setKnowledgeVisited(true);
    setKnowledgeOpen((open) => !open);
    setLayersOpen(false);
  }

  function insertKnowledge(content: KnowledgeInsertion) {
    if (selectedNode?.kind !== "terminal") {
      throw new Error("Select a terminal before inserting this snapshot.");
    }
    const current = selectedNode.promptDraft ?? "";
    const separator = current.endsWith("\n\n") || !current ? "" : current.endsWith("\n") ? "\n" : "\n\n";
    const title = content.title || (content.kind === "prompt" ? "Untitled prompt" : "Untitled context");
    const text = `${current}${separator}Knowledge snapshot: ${title}\n${content.body}\n`;
    dispatch({ type: "terminal/draft", nodeId: selectedNode.id, text });
    setComposerOpen(true);
    setKnowledgeOpen(false);
  }

  function setZoom(zoom: number) {
    markCanvasScrolling();
    dispatch({ type: "zoom/set", zoom: Number(zoom.toFixed(2)) });
  }

  function selectAllNodes() {
    onSelectSession(null);
    dispatch({
      type: "nodes/select",
      nodeIds: visibleNodes.map((node) => node.id),
    });
  }

  function duplicateSelectedNodes() {
    if (selectedNodeIds.length === 0) return;
    onSelectSession(null);
    focusSelectionRef.current = true;
    dispatch(duplicateCanvasSelection(state, selectedNodeIds));
  }

  function removeSelectedNodes() {
    if (selectedNodeIds.length === 0) return;
    onSelectSession(null);
    dispatch({ type: "nodes/delete", nodeIds: selectedNodeIds });
    viewportRef.current?.focus({ preventScroll: true });
  }

  function selectNode(node: CanvasNode, additive = false, fromFocus = false) {
    if (fromFocus && state.selectedNodeIds.length > 0) return;
    const sessionId = node.kind === "terminal" ? node.sessionId : undefined;
    // A selection made here is already focused; do not treat its echo from
    // AppShell as a sidebar navigation request that would collapse the group.
    handledFocusRequestRef.current = sessionId
      ? { sessionId, revision: sessionFocusRevision }
      : null;
    if (additive || !state.selectedNodeIds.includes(node.id)) {
      dispatch({ type: "node/select", nodeId: node.id, additive });
    } else if (state.selectedNodeId !== node.id) {
      dispatch({
        type: "nodes/select",
        nodeIds: [...state.selectedNodeIds.filter((id) => id !== node.id), node.id],
      });
    }
    onSelectSession(sessionId ?? null);
  }

  function closeLayers() {
    setLayersOpen(false);
    layersTriggerRef.current?.focus();
  }

  function fitCanvasToItems() {
    const viewport = viewportRef.current;
    if (!viewport || storedVisibleNodes.length === 0) {
      return;
    }

    const storedBounds = canvasNodeBounds(storedVisibleNodes);
    const measuredViewportWidth = viewport.clientWidth || viewportWidth || 960;
    const viewportHeight = viewport.clientHeight || 640;
    const nextZoom = Number(
      Math.min(
        1,
        Math.max(
          0.5,
          Math.min(
            (measuredViewportWidth - 160) / storedBounds.width,
            (viewportHeight - 160) / storedBounds.height,
          ),
        ),
      ).toFixed(2),
    );
    const fittedBounds = canvasNodeBounds(
      effectiveCanvasNodes(
        storedVisibleNodes,
        isCompact,
        measuredViewportWidth,
        nextZoom,
      ),
    );

    markCanvasScrolling();
    setZoom(nextZoom);
    viewport.scrollTo({
      left: Math.max(
        0,
        (CANVAS_ORIGIN_X + fittedBounds.minimumX + fittedBounds.width / 2) *
          nextZoom -
          measuredViewportWidth / 2,
      ),
      top: Math.max(
        0,
        (CANVAS_ORIGIN_Y + fittedBounds.minimumY + fittedBounds.height / 2) *
          nextZoom -
          viewportHeight / 2,
      ),
      behavior: canvasScrollBehavior(),
    });
  }

  function resetCanvasLayout() {
    onSelectSession(null);
    if (!project) {
      dispatch({
        type: "document/hydrate",
        document: createInitialCanvasDocument(),
      });
      return;
    }

    const projectNodeIds = new Set(
      state.nodes
        .filter((node) => node.projectId === project.id)
        .map((node) => node.id),
    );
    const projectSessionIds = new Set(
      projectSessions.map((session) => session.id),
    );
    dispatch({
      type: "document/hydrate",
      document: {
        version: 2,
        nodes: state.nodes.filter((node) => !projectNodeIds.has(node.id)),
        connections: state.connections.filter(
          (connection) =>
            !projectNodeIds.has(connection.sourceNodeId) &&
            !projectNodeIds.has(connection.targetNodeId),
        ),
        zoom: state.zoom,
        hiddenSessionIds: state.hiddenSessionIds.filter(
          (sessionId) => !projectSessionIds.has(sessionId),
        ),
      },
    });
  }

  return (
    <main
      id="workspace"
      className="canvas-workspace"
      tabIndex={-1}
      aria-labelledby="canvas-workspace-title"
      onCompositionStartCapture={() => { canvasComposingRef.current = true; }}
      onCompositionEndCapture={() => { canvasComposingRef.current = false; }}
      onKeyDownCapture={(event) => {
        if (
          event.defaultPrevented || event.repeat || canvasComposingRef.current || event.nativeEvent.isComposing
          || event.nativeEvent.keyCode === 229 || !event.shiftKey || event.altKey
          || event.metaKey === event.ctrlKey || event.key.toLowerCase() !== "p"
        ) return;
        const target = event.target instanceof Element ? event.target : null;
        if (target?.closest('[data-shortcut-scope="knowledge-library"]')) return;
        const targetId = target?.closest("[data-canvas-node-id]")?.getAttribute("data-canvas-node-id");
        const targetNode = visibleNodes.find((node) => node.id === targetId);
        if (targetNode && targetNode.kind !== "terminal") return;
        const terminal = targetNode?.kind === "terminal" ? targetNode : selectedNode;
        if (terminal?.kind !== "terminal") return;
        if (
          target?.closest("input, textarea, select, [contenteditable]:not([contenteditable='false']), [role='textbox'], [role='dialog']")
          && !target?.closest("[data-terminal-root], .prompt-composer")
        ) return;
        event.preventDefault();
        event.stopPropagation();
        toggleComposer(terminal);
      }}
      onKeyDown={(event) => {
        if (event.defaultPrevented || isCanvasEditingTarget(event.target)) {
          return;
        }
        const command = event.metaKey || event.ctrlKey;
        if (command && event.key.toLowerCase() === "f") {
          event.preventDefault();
          setLayersOpen(true);
        } else if (command && event.key.toLowerCase() === "a") {
          event.preventDefault();
          selectAllNodes();
        } else if (command && event.key.toLowerCase() === "d") {
          event.preventDefault();
          duplicateSelectedNodes();
        } else if (event.key === "Delete" || event.key === "Backspace") {
          event.preventDefault();
          removeSelectedNodes();
        } else if (event.key === "Escape") {
          dispatch({ type: "node/select", nodeId: null });
        }
      }}
    >
      <div
        className="canvas-context"
        aria-live="polite"
        data-browser-obstruction="true"
      >
        <span className="canvas-context__eyebrow">Workspace</span>
        <h1 id="canvas-workspace-title">
          {project?.name ?? "My Workspace"}
        </h1>
        <span className="canvas-context__meta">
          {projects.length} {projects.length === 1 ? "project" : "projects"}
          <span aria-hidden="true"> · </span>
          {visibleNodes.filter((node) => node.kind === "terminal").length}{" "}
          terminals
          <span aria-hidden="true"> · </span>
          {visibleNodes.filter((node) => node.kind === "browser").length}{" "}
          browsers
        </span>
      </div>

      <div
        className="canvas-toolbar"
        role="toolbar"
        aria-label="Canvas tools"
        data-browser-obstruction="true"
      >
        <button
          className="canvas-tool"
          type="button"
          aria-label="Add terminal card"
          onClick={() => setTerminalDialogOpen(true)}
        >
          <Icon name="terminal" />
        </button>
        <button
          className="canvas-tool"
          type="button"
          aria-label="Add note"
          onClick={addNote}
        >
          <Icon name="note" />
        </button>
        <button
          className="canvas-tool"
          type="button"
          aria-label="Add browser"
          onClick={addBrowser}
        >
          <Icon name="browser" />
        </button>
        <button
          className="canvas-tool"
          type="button"
          aria-label="Toggle Prompt Composer"
          title="Prompt Composer (⌘/Ctrl+Shift+P)"
          aria-keyshortcuts="Control+Shift+P Meta+Shift+P"
          aria-expanded={Boolean(composerNode)}
          disabled={selectedNode?.kind !== "terminal"}
          onClick={() => {
            if (selectedNode?.kind === "terminal") toggleComposer(selectedNode);
          }}
        ><Icon name="pencil" /></button>
        <button
          className="canvas-tool"
          type="button"
          aria-label="Open prompts and context"
          title="Prompts and context library"
          aria-expanded={knowledgeOpen}
          disabled={!knowledgeClient}
          onClick={toggleKnowledge}
        ><Icon name="repository" /></button>
        <button
          className={
            visibleConnectionSourceId
              ? "canvas-tool canvas-tool--connecting"
              : "canvas-tool"
          }
          type="button"
          aria-label={
            visibleConnectionSourceId
              ? "Cancel connection"
              : "Connect selected item"
          }
          aria-pressed={visibleConnectionSourceId !== null}
          disabled={!selectedNode}
          onClick={() => {
            if (visibleConnectionSourceId) {
              dispatch({ type: "connection/cancel" });
            } else if (selectedNode) {
              dispatch({ type: "connection/start", nodeId: selectedNode.id });
            }
          }}
        >
          <Icon name="link" />
        </button>
        <span className="canvas-toolbar__divider" aria-hidden="true" />
        <button
          className="canvas-tool"
          type="button"
          aria-label="Select all canvas items"
          title="Select all canvas items (⌘/Ctrl+A)"
          disabled={visibleNodes.length === 0}
          onClick={selectAllNodes}
        >
          <Icon name="layers" />
        </button>
        <button
          className="canvas-tool"
          type="button"
          aria-label="Duplicate selected canvas items"
          title="Duplicate selected cards (⌘/Ctrl+D); sessions start manually"
          disabled={selectedNodeIds.length === 0}
          onClick={duplicateSelectedNodes}
        >
          <Icon name="copy" />
        </button>
        <button
          className="canvas-tool"
          type="button"
          aria-label={
            selectedNodeIds.length > 1
              ? "Remove selected items from canvas"
              : "Remove selected item from canvas"
          }
          title="Remove selected cards (Delete); sessions keep running"
          disabled={selectedNodeIds.length === 0}
          onClick={removeSelectedNodes}
        >
          <Icon name="trash" />
        </button>
        <button
          className="canvas-tool"
          type="button"
          aria-label="Reset canvas layout"
          title={
            project ? "Reset this project's canvas layout" : "Reset canvas layout"
          }
          onClick={resetCanvasLayout}
        >
          <Icon name="refresh" />
        </button>
      </div>

      <p
        className="canvas-selection-status"
        role="status"
        data-browser-obstruction="true"
      >
        {selectedNodeIds.length > 0
          ? `${selectedNodeIds.length} selected · Shift+click to add or remove`
          : "Shift+click to select multiple items"}
      </p>

      {visibleConnectionSourceId ? (
        <div
          className="canvas-connect-notice"
          role="status"
          data-browser-obstruction="true"
        >
          <Icon name="link" />
          Choose another canvas item to finish the connection.
          <button
            type="button"
            onClick={() => dispatch({ type: "connection/cancel" })}
          >
            Cancel
          </button>
        </div>
      ) : null}

      <div
        ref={viewportRef}
        className="canvas-viewport"
        data-browser-viewport="true"
        tabIndex={0}
        aria-label="Pannable canvas"
        aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight Control+a Meta+a Control+d Meta+d Control+f Meta+f Delete Backspace"
        onKeyDown={(event) => {
          if (event.defaultPrevented || event.currentTarget !== event.target) {
            return;
          }
          const step = event.altKey ? 16 : 80;
          const movement = keyboardMovement(event.key, step);
          if (movement) {
            event.preventDefault();
            markCanvasScrolling();
            if (selectedNodeIds.length > 0) {
              dispatch({
                type: "nodes/move",
                nodeIds: selectedNodeIds,
                delta: keyboardMovement(event.key, event.altKey ? 1 : 8) ?? movement,
              });
            } else {
              event.currentTarget.scrollLeft += movement.x;
              event.currentTarget.scrollTop += movement.y;
            }
          }
        }}
        onPointerDown={(event) => {
          if (
            event.button !== 0 ||
            (event.target instanceof Element &&
              event.target.closest(".canvas-node"))
          ) {
            return;
          }
          event.preventDefault();
          onSelectSession(null);
          dispatch({ type: "node/select", nodeId: null });
          panRef.current = {
            pointerId: event.pointerId,
            clientX: event.clientX,
            clientY: event.clientY,
            scrollLeft: event.currentTarget.scrollLeft,
            scrollTop: event.currentTarget.scrollTop,
          };
          setCanvasInteracting(true);
          markCanvasScrolling();
          event.currentTarget.setPointerCapture?.(event.pointerId);
        }}
        onPointerMove={(event) => {
          const pan = panRef.current;
          if (!pan || pan.pointerId !== event.pointerId) {
            return;
          }
          markCanvasScrolling();
          event.currentTarget.scrollLeft =
            pan.scrollLeft - (event.clientX - pan.clientX);
          event.currentTarget.scrollTop =
            pan.scrollTop - (event.clientY - pan.clientY);
        }}
        onPointerUp={(event) => {
          if (panRef.current?.pointerId === event.pointerId) {
            panRef.current = null;
            setCanvasInteracting(false);
            event.currentTarget.releasePointerCapture?.(event.pointerId);
          }
        }}
        onPointerCancel={() => {
          panRef.current = null;
          setCanvasInteracting(false);
        }}
        onWheel={(event) => {
          if (event.deltaX !== 0 || event.deltaY !== 0) {
            markCanvasScrolling();
          }
          if (event.shiftKey && event.deltaX === 0) {
            event.preventDefault();
            event.currentTarget.scrollLeft += event.deltaY;
          }
        }}
        onScroll={markCanvasScrolling}
      >
        <div
          className="canvas-stage"
          style={{ transform: `scale(${state.zoom})` }}
        >
          <CanvasConnections
            connections={visibleConnections}
            nodes={visibleNodes}
            selectedNodeId={state.selectedNodeId}
            connectionSourceId={visibleConnectionSourceId}
          />
          {visibleNodes.map((node) => {
            const storedNode = storedVisibleNodesById.get(node.id);
            const session =
              node.kind === "terminal"
                ? terminalSessions.get(node.sessionId ?? "")
                : undefined;
            const worktree = session
              ? worktrees.find((candidate) =>
                  session.worktreeId
                    ? candidate.id === session.worktreeId
                    : candidate.sessionId === session.id,
                )
              : undefined;
            return (
              <CanvasNodeCard
                key={node.id}
                node={node}
                storedTerminalSize={
                  storedNode?.kind === "terminal"
                    ? { width: storedNode.width, height: storedNode.height }
                    : undefined
                }
                session={session}
                agent={agents.find(
                  (agent) => agent.id === (session?.agentId ??
                    (node.kind === "terminal" ? node.agentId : undefined)),
                )}
                worktree={worktree}
                isConnected={isConnected}
                selected={selectedNodeIds.includes(node.id)}
                connectionSource={visibleConnectionSourceId}
                connectionCount={visibleConnections.filter(
                  (connection) =>
                    connection.sourceNodeId === node.id ||
                    connection.targetNodeId === node.id,
                ).length}
                onSelect={(additive, fromFocus) =>
                  selectNode(node, additive, fromFocus)
                }
                onConnect={() => {
                  if (
                    visibleConnectionSourceId &&
                    visibleConnectionSourceId !== node.id
                  ) {
                    dispatch({
                      type: "connection/complete",
                      targetNodeId: node.id,
                    });
                  } else if (visibleConnectionSourceId === node.id) {
                    dispatch({ type: "connection/cancel" });
                  } else {
                    dispatch({ type: "connection/start", nodeId: node.id });
                  }
                }}
                onCancelConnection={() =>
                  dispatch({ type: "connection/cancel" })
                }
                onDelete={() => {
                  onSelectSession(null);
                  dispatch({ type: "node/delete", nodeId: node.id });
                }}
                zoom={state.zoom}
                onMove={(delta) =>
                  dispatch({
                    type: "nodes/move",
                    nodeIds: selectedNodeIds.includes(node.id)
                      ? selectedNodeIds
                      : [node.id],
                    delta,
                  })
                }
                onResize={(size) =>
                  dispatch({ type: "node/resize", nodeId: node.id, size })
                }
                onManipulationChange={setCanvasInteracting}
                onNoteChange={(text) =>
                  dispatch({ type: "note/update", nodeId: node.id, text })
                }
                onStartTerminal={() =>
                  node.kind === "terminal" && launchTerminal(node, session)
                }
                composerOpen={composerNode?.id === node.id}
                onToggleComposer={() => {
                  if (node.kind === "terminal") toggleComposer(node);
                }}
                onTerminalInput={registerTerminalInput}
                onStartSession={onStartSession}
                onRestartSession={onRestartSession}
                onRenameSession={onRenameSession}
                onStopSession={onStopSession}
                onDeleteSession={onDeleteSession}
                onRemoveWorktree={onRemoveWorktree}
                onGitStatus={onGitStatus}
                onOpenPath={onOpenPath}
                elementRef={(element) => {
                  if (element) {
                    nodeElementsRef.current.set(node.id, element);
                  } else {
                    nodeElementsRef.current.delete(node.id);
                  }
                }}
                terminalPending={pendingTerminals.has(node.id)}
                terminalError={terminalErrors[node.id]}
                terminalTransport={{
                  subscribeTerminal,
                  writeTerminal,
                  resizeTerminal,
                }}
                browserRuntime={browserRuntime}
                browserActive={state.selectedNodeId === node.id}
                browserVisible={
                  state.zoom === 1 &&
                  !terminalDialogOpen &&
                  !layersOpen &&
                  !composerNode &&
                  !knowledgeOpen &&
                  !canvasInteracting &&
                  !canvasScrolling
                }
                browserUnavailableReason={
                  canvasInteracting
                    ? "The browser is hidden while the canvas item moves."
                    : canvasScrolling
                      ? "The browser is hidden while the canvas view moves."
                    : terminalDialogOpen
                    ? "The browser is hidden while a dialog covers the canvas."
                    : layersOpen
                      ? "The browser is hidden while canvas search is open."
                    : composerNode
                      ? "The browser is hidden while Prompt Composer is open."
                    : knowledgeOpen
                      ? "The browser is hidden while the knowledge library is open."
                    : state.zoom !== 1
                      ? "Use 100% zoom to interact with this page."
                      : undefined
                }
                onBrowserNavigate={(url) => {
                  setBrowserHandoffStatus(null);
                  dispatch({ type: "browser/navigate", nodeId: node.id, url });
                }}
              />
            );
          })}
        </div>
      </div>

      {layersOpen ? (
        <CanvasElementSearch
          key={project?.id ?? "shared"}
          nodes={visibleNodes}
          sessions={terminalSessions}
          agents={agents}
          onFocusNode={focusNode}
          onClose={closeLayers}
        />
      ) : null}

      {composerNode && !layersOpen && !terminalDialogOpen && !knowledgeOpen ? (
        <div className="canvas-prompt-composer" data-browser-obstruction="true" data-shortcut-scope="prompt-composer">
          <PromptComposer
            nodeId={composerNode.id}
            title={composerNode.title}
            value={composerNode.promptDraft ?? ""}
            onChange={(text) => dispatch({ type: "terminal/draft", nodeId: composerNode.id, text })}
            onSend={(text) => sendComposerPrompt(composerNode, text)}
            onTerminalKey={(key) => sendComposerKey(composerNode, key)}
            onClose={() => setComposerOpen(false)}
            disabledReason={composerDisabledReason}
            contextItems={composerContextItems}
            clearOnSend={false}
          />
        </div>
      ) : null}

      {knowledgeVisited && knowledgeClient ? (
        <CanvasKnowledgePanel
          open={knowledgeOpen && !layersOpen && !terminalDialogOpen}
          client={knowledgeClient}
          connectionKey={knowledgeConnectionKey}
          currentProject={project ?? null}
          projects={projects}
          targetTitle={selectedNode?.kind === "terminal" ? selectedNode.title : undefined}
          insertDisabledReason={selectedNode?.kind === "terminal" ? undefined : "Select a terminal to insert content into its draft."}
          onInsert={insertKnowledge}
          onClose={() => setKnowledgeOpen(false)}
        />
      ) : null}

      {selectedNode && selectedConnections.length > 0 && !layersOpen && !composerNode && !knowledgeOpen ? (
        <section
          className="canvas-connections-panel"
          aria-label={`Connections for ${selectedNode.title}`}
          data-browser-obstruction="true"
        >
          <header>
            <div>
              <span>Selected item</span>
              <h2>{selectedNode.title}</h2>
            </div>
            <span>{selectedConnections.length}</span>
          </header>
          <ul>
            {selectedConnections.map(({ connection, otherNode }) => {
              const handoff = getBrowserHandoff(selectedNode, otherNode);
              return (
                <li key={connection.id}>
                  <Icon name="link" />
                  <span>{otherNode.title}</span>
                  {handoff ? (
                    <button
                      className="canvas-connections-panel__handoff"
                      type="button"
                      aria-label={browserHandoffLabel(handoff.target)}
                      title={browserHandoffLabel(handoff.target)}
                      onClick={() =>
                        void handoffBrowserUrl(handoff.browser, handoff.target)
                      }
                    >
                      <Icon name="copy" />
                    </button>
                  ) : null}
                  <button
                    type="button"
                    aria-label={`Remove connection to ${otherNode.title}`}
                    onClick={() =>
                      dispatch({
                        type: "connection/delete",
                        connectionId: connection.id,
                      })
                    }
                  >
                    <Icon name="close" />
                  </button>
                </li>
              );
            })}
          </ul>
          {browserHandoffStatus ? (
            <p className="canvas-connections-panel__status" role="status">
              {browserHandoffStatus}
            </p>
          ) : null}
        </section>
      ) : null}

      <div
        className="canvas-view-controls"
        role="group"
        aria-label="Canvas view controls"
        data-browser-obstruction="true"
      >
        <button
          ref={layersTriggerRef}
          type="button"
          aria-label="Show canvas items"
          aria-expanded={layersOpen}
          aria-controls="canvas-layers-panel"
          onClick={() => setLayersOpen((open) => !open)}
        >
          <Icon name="layers" />
        </button>
        <button
          type="button"
          aria-label="Fit canvas to items"
          onClick={fitCanvasToItems}
        >
          <Icon name="map" />
        </button>
      </div>

      <div
        className="canvas-zoom"
        role="group"
        aria-label="Canvas zoom controls"
        data-browser-obstruction="true"
      >
        <button
          type="button"
          aria-label="Zoom out"
          disabled={state.zoom <= 0.5}
          onClick={() => setZoom(state.zoom - ZOOM_STEP)}
        >
          <Icon name="zoom-out" />
        </button>
        <output aria-live="polite">{Math.round(state.zoom * 100)}%</output>
        <button
          type="button"
          aria-label="Zoom in"
          disabled={state.zoom >= 1.5}
          onClick={() => setZoom(state.zoom + ZOOM_STEP)}
        >
          <Icon name="zoom-in" />
        </button>
      </div>

      <p
        className="canvas-save-status"
        role="status"
        data-browser-obstruction="true"
      >
        {persistenceAvailable
          ? "Canvas saved locally"
          : "Canvas persistence unavailable"}
      </p>

      {terminalDialogOpen ? (
        <NewCanvasTerminalDialog
          defaultWorkingDirectory={project?.path ?? "~"}
          onClose={() => setTerminalDialogOpen(false)}
          onCreate={addTerminal}
        />
      ) : null}
    </main>
  );
}

interface CanvasNodeCardProps {
  readonly node: CanvasNode;
  readonly storedTerminalSize?: {
    readonly width: number;
    readonly height: number;
  };
  readonly session?: Session;
  readonly agent?: AgentRecord;
  readonly worktree?: Worktree;
  readonly isConnected: boolean;
  readonly selected: boolean;
  readonly connectionSource: string | null;
  readonly connectionCount: number;
  readonly zoom: number;
  readonly onSelect: (additive?: boolean, fromFocus?: boolean) => void;
  readonly onConnect: () => void;
  readonly onCancelConnection: () => void;
  readonly onDelete: () => void;
  readonly onMove: (delta: { readonly x: number; readonly y: number }) => void;
  readonly onResize: (size: {
    readonly width: number;
    readonly height: number;
  }) => void;
  readonly onManipulationChange: (interacting: boolean) => void;
  readonly onNoteChange: (text: string) => void;
  readonly onStartTerminal: () => void;
  readonly composerOpen: boolean;
  readonly onToggleComposer: () => void;
  readonly onTerminalInput: (nodeId: string, handle: LiveTerminalInputHandle | null) => void;
  readonly onStartSession: (sessionId: string) => Promise<Session>;
  readonly onRestartSession: (sessionId: string) => Promise<Session>;
  readonly onRenameSession: (sessionId: string) => void;
  readonly onStopSession: (sessionId: string) => void;
  readonly onDeleteSession: (sessionId: string) => void;
  readonly onRemoveWorktree: (worktreeId: string) => void;
  readonly onGitStatus: (sessionId: string) => void;
  readonly onOpenPath: (path: string) => Promise<void>;
  readonly elementRef: (element: HTMLElement | null) => void;
  readonly terminalPending: boolean;
  readonly terminalError?: string;
  readonly terminalTransport: LiveTerminalTransport;
  readonly browserRuntime: BrowserRuntime;
  readonly browserActive: boolean;
  readonly browserVisible: boolean;
  readonly browserUnavailableReason?: string;
  readonly onBrowserNavigate: (url: string) => void;
}

function CanvasNodeCard({
  node,
  storedTerminalSize,
  session,
  agent,
  worktree,
  isConnected,
  selected,
  connectionSource,
  connectionCount,
  zoom,
  onSelect,
  onConnect,
  onCancelConnection,
  onDelete,
  onMove,
  onResize,
  onManipulationChange,
  onNoteChange,
  onStartTerminal,
  composerOpen,
  onToggleComposer,
  onTerminalInput,
  onStartSession,
  onRestartSession,
  onRenameSession,
  onStopSession,
  onDeleteSession,
  onRemoveWorktree,
  onGitStatus,
  onOpenPath,
  elementRef,
  terminalPending,
  terminalError,
  terminalTransport,
  browserRuntime,
  browserActive,
  browserVisible,
  browserUnavailableReason,
  onBrowserNavigate,
}: CanvasNodeCardProps) {
  const registerInput = useCallback((handle: LiveTerminalInputHandle | null) => {
    onTerminalInput(node.id, handle);
  }, [node.id, onTerminalInput]);
  const dragRef = useRef<{
    readonly pointerId: number;
    readonly clientX: number;
    readonly clientY: number;
  } | null>(null);
  const pointerSelectingRef = useRef(false);
  const resizeRef = useRef<{
    readonly pointerId: number;
    readonly clientX: number;
    readonly clientY: number;
    readonly width: number;
    readonly height: number;
  } | null>(null);
  const isConnectionTarget =
    connectionSource !== null && connectionSource !== node.id;
  const classes = [
    "canvas-node",
    `canvas-node--${node.kind}`,
    selected ? "canvas-node--selected" : "",
    isConnectionTarget ? "canvas-node--connection-target" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <article
      ref={elementRef}
      className={classes}
      style={{
        transform: `translate(${toStagePoint(node).x}px, ${toStagePoint(node).y}px)`,
        ...(node.kind !== "note"
          ? { width: `${node.width}px`, height: `${node.height}px` }
          : {}),
      }}
      tabIndex={0}
      aria-label={`${node.title}, ${node.kind} canvas item`}
      data-canvas-node-id={node.id}
      data-canvas-session-id={session?.id}
      data-selected={selected ? "true" : undefined}
      aria-describedby={selected ? `${node.id}-selection-status` : undefined}
      aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight Shift+Space"
      onFocus={(event) => {
        if (event.currentTarget === event.target && !pointerSelectingRef.current) {
          onSelect(false, true);
        } else if (
          event.target instanceof Element &&
          event.target.closest("[data-terminal-root]")
        ) {
          onSelect();
        }
      }}
      onKeyDown={(event) => {
        if (event.currentTarget !== event.target) {
          return;
        }
        const step = event.altKey ? 1 : 8;
        const movement = keyboardMovement(event.key, step);
        if (movement) {
          event.preventDefault();
          onManipulationChange(true);
          onSelect();
          onMove(movement);
        } else if (event.key === " " && event.shiftKey) {
          event.preventDefault();
          onSelect(true);
        } else if (event.key === "Escape" && connectionSource) {
          event.preventDefault();
          onCancelConnection();
        }
      }}
      onKeyUp={(event) => {
        if (
          event.currentTarget === event.target &&
          keyboardMovement(event.key, 1)
        ) {
          onManipulationChange(false);
        }
      }}
      onBlur={(event) => {
        if (event.currentTarget === event.target) {
          onManipulationChange(false);
        }
      }}
      onPointerDown={(event) => {
        event.stopPropagation();
        if (event.button !== 0) return;
        if (
          event.target instanceof Element &&
          event.target.closest("[data-terminal-root]")
        ) {
          onSelect();
          return;
        }
        if (isCanvasEditingTarget(event.target)) return;
        pointerSelectingRef.current = true;
        onSelect(event.shiftKey);
      }}
      onPointerUp={() => {
        pointerSelectingRef.current = false;
      }}
      onPointerCancel={() => {
        pointerSelectingRef.current = false;
      }}
    >
      {selected ? (
        <span id={`${node.id}-selection-status`} className="visually-hidden">
          Selected canvas item
        </span>
      ) : null}
      <header
        className="canvas-node__header"
        aria-label={`Move ${node.title}`}
        onPointerDown={(event) => {
          if (
            event.button !== 0 ||
            (event.target instanceof Element &&
              event.target.closest("button, summary"))
          ) {
            return;
          }
          event.preventDefault();
          event.stopPropagation();
          pointerSelectingRef.current = true;
          onSelect(event.shiftKey);
          event.currentTarget.closest("article")?.focus({ preventScroll: true });
          if (event.shiftKey) return;
          dragRef.current = {
            pointerId: event.pointerId,
            clientX: event.clientX,
            clientY: event.clientY,
          };
          onManipulationChange(true);
          event.currentTarget.setPointerCapture?.(event.pointerId);
        }}
        onPointerMove={(event) => {
          const drag = dragRef.current;
          if (!drag || drag.pointerId !== event.pointerId) {
            return;
          }
          onMove({
            x: (event.clientX - drag.clientX) / zoom,
            y: (event.clientY - drag.clientY) / zoom,
          });
          dragRef.current = {
            pointerId: event.pointerId,
            clientX: event.clientX,
            clientY: event.clientY,
          };
        }}
        onPointerUp={(event) => {
          if (dragRef.current?.pointerId === event.pointerId) {
            dragRef.current = null;
            onManipulationChange(false);
            event.currentTarget.releasePointerCapture?.(event.pointerId);
          }
        }}
        onPointerCancel={() => {
          dragRef.current = null;
          onManipulationChange(false);
        }}
      >
        <div className="canvas-node__identity">
          {node.kind === "terminal" ? (
            <span className="canvas-window-controls" aria-hidden="true">
              <i />
              <i />
              <i />
            </span>
          ) : (
            <span className="canvas-node__kind-icon" aria-hidden="true">
              <Icon name={node.kind === "browser" ? "browser" : "note"} />
            </span>
          )}
          <strong>{node.title}</strong>
          {connectionCount > 0 ? (
            <span className="canvas-node__connection-count">
              <Icon name="link" /> {connectionCount}
            </span>
          ) : null}
        </div>
        <div className="canvas-node__actions">
          {node.kind === "terminal" ? (
            <button
              type="button"
              aria-label={`Open Prompt Composer for ${node.title}`}
              title="Prompt Composer (⌘/Ctrl+Shift+P)"
              aria-expanded={composerOpen}
              onClick={(event) => {
                event.stopPropagation();
                onToggleComposer();
              }}
            ><Icon name="pencil" /></button>
          ) : null}
          {selected ? (
            <span className="canvas-node__selected-label">Selected</span>
          ) : null}
          {session ? (
            <CanvasSessionActions
              session={session}
              worktree={worktree}
              isConnected={isConnected}
              onStartSession={onStartSession}
              onRestartSession={onRestartSession}
              onRenameSession={onRenameSession}
              onStopSession={onStopSession}
              onDeleteSession={onDeleteSession}
              onRemoveWorktree={onRemoveWorktree}
              onGitStatus={onGitStatus}
              onOpenPath={onOpenPath}
            />
          ) : null}
          <button
            type="button"
            aria-label={
              isConnectionTarget
                ? `Connect to ${node.title}`
                : connectionSource === node.id
                  ? `Cancel connection from ${node.title}`
                  : `Start connection from ${node.title}`
            }
            aria-pressed={connectionSource === node.id}
            onClick={(event) => {
              event.stopPropagation();
              onConnect();
            }}
          >
            <Icon name="link" />
          </button>
          <button
            type="button"
            aria-label={`Remove ${node.title} from canvas`}
            title="Remove this card from the canvas without deleting its session"
            onClick={(event) => {
              event.stopPropagation();
              onDelete();
            }}
          >
            <Icon name="trash" />
          </button>
        </div>
      </header>
      {node.kind === "terminal" ? (
        <TerminalNodeBody
          node={node}
          agent={agent}
          session={session}
          onStart={onStartTerminal}
          pending={terminalPending}
          error={terminalError}
          startDisabledReason={terminalStartDisabledReason(
            session,
            worktree,
            isConnected,
          )}
          transport={terminalTransport}
          inputRef={registerInput}
        />
      ) : node.kind === "note" ? (
        <NoteNodeBody node={node} onChange={onNoteChange} />
      ) : (
        <BrowserSurface
          nodeId={node.id}
          url={node.url}
          accessibleLabel={`Browser surface for ${node.title}`}
          active={browserActive}
          visible={browserVisible}
          unavailableReason={browserUnavailableReason}
          runtime={browserRuntime}
          onActivate={onSelect}
          onNavigate={onBrowserNavigate}
          onOpenExternal={(url) => browserRuntime.openExternal(url)}
        />
      )}
      {node.kind !== "note" && selected ? (
        <button
          className="canvas-node__resize-handle"
          type="button"
          aria-label={`Resize ${node.title}`}
          aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight"
          title="Drag to resize. Arrow keys resize; hold Alt for 1 px."
          onKeyDown={(event) => {
            const step = event.altKey ? 1 : 16;
            const size = keyboardResize(
              event.key,
              {
                ...node,
                width: storedTerminalSize?.width ?? node.width,
                height: storedTerminalSize?.height ?? node.height,
              },
              step,
            );
            if (size) {
              event.preventDefault();
              onManipulationChange(true);
              onResize(size);
            }
          }}
          onKeyUp={(event) => {
            if (keyboardResize(event.key, node, 1)) {
              onManipulationChange(false);
            }
          }}
          onBlur={() => onManipulationChange(false)}
          onPointerDown={(event) => {
            event.preventDefault();
            event.stopPropagation();
            resizeRef.current = {
              pointerId: event.pointerId,
              clientX: event.clientX,
              clientY: event.clientY,
              width: storedTerminalSize?.width ?? node.width,
              height: storedTerminalSize?.height ?? node.height,
            };
            onManipulationChange(true);
            event.currentTarget.setPointerCapture?.(event.pointerId);
          }}
          onPointerMove={(event) => {
            const resize = resizeRef.current;
            if (!resize || resize.pointerId !== event.pointerId) {
              return;
            }
            onResize({
              width: resize.width + (event.clientX - resize.clientX) / zoom,
              height: resize.height + (event.clientY - resize.clientY) / zoom,
            });
          }}
          onPointerUp={(event) => {
            if (resizeRef.current?.pointerId === event.pointerId) {
              resizeRef.current = null;
              onManipulationChange(false);
              event.currentTarget.releasePointerCapture?.(event.pointerId);
            }
          }}
          onPointerCancel={() => {
            resizeRef.current = null;
            onManipulationChange(false);
          }}
        >
          <span aria-hidden="true" />
        </button>
      ) : null}
    </article>
  );
}

function useElementWidth(elementRef: {
  readonly current: HTMLElement | null;
}): number {
  const subscribe = useCallback(
    (onStoreChange: () => void) => {
      const element = elementRef.current;
      if (!element || typeof window === "undefined") {
        return () => undefined;
      }
      const observer =
        typeof ResizeObserver === "function"
          ? new ResizeObserver(() => onStoreChange())
          : undefined;
      observer?.observe(element);
      window.addEventListener("resize", onStoreChange);
      return () => {
        observer?.disconnect();
        window.removeEventListener("resize", onStoreChange);
      };
    },
    [elementRef],
  );
  const getSnapshot = useCallback(
    () => elementRef.current?.clientWidth ?? 0,
    [elementRef],
  );
  return useSyncExternalStore(subscribe, getSnapshot, () => 0);
}

function effectiveCanvasNodes(
  nodes: readonly CanvasNode[],
  isCompact: boolean,
  viewportWidth: number,
  zoom: number,
): readonly CanvasNode[] {
  if (!isCompact || viewportWidth <= 0) {
    return nodes;
  }
  const maximumTerminalWidth = Math.max(
    1,
    (viewportWidth - COMPACT_TERMINAL_GUTTER_PX) / zoom,
  );
  return nodes.map((node) =>
    node.kind === "terminal" && node.width > maximumTerminalWidth
      ? { ...node, width: maximumTerminalWidth }
      : node,
  );
}

function canvasNodeBounds(nodes: readonly CanvasNode[]) {
  const minimumX = Math.min(...nodes.map((node) => node.x));
  const minimumY = Math.min(...nodes.map((node) => node.y));
  const maximumX = Math.max(
    ...nodes.map((node) => node.x + getCanvasNodeSize(node).width),
  );
  const maximumY = Math.max(
    ...nodes.map((node) => node.y + getCanvasNodeSize(node).height),
  );
  return {
    minimumX,
    minimumY,
    width: maximumX - minimumX,
    height: maximumY - minimumY,
  };
}

interface CanvasSessionActionsProps {
  readonly session: Session;
  readonly worktree?: Worktree;
  readonly isConnected: boolean;
  readonly onStartSession: (sessionId: string) => Promise<Session>;
  readonly onRestartSession: (sessionId: string) => Promise<Session>;
  readonly onRenameSession: (sessionId: string) => void;
  readonly onStopSession: (sessionId: string) => void;
  readonly onDeleteSession: (sessionId: string) => void;
  readonly onRemoveWorktree: (worktreeId: string) => void;
  readonly onGitStatus: (sessionId: string) => void;
  readonly onOpenPath: (path: string) => Promise<void>;
}

function CanvasSessionActions({
  session,
  worktree,
  isConnected,
  onStartSession,
  onRestartSession,
  onRenameSession,
  onStopSession,
  onDeleteSession,
  onRemoveWorktree,
  onGitStatus,
  onOpenPath,
}: CanvasSessionActionsProps) {
  const actionsContainerRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const [actionsOpen, setActionsOpen] = useState(false);
  const [pendingAction, setPendingAction] = useState<string>();
  const [actionError, setActionError] = useState<ApiErrorData>();
  const live = isLiveStatus(session.status);
  const managedWorktreeUnavailable = Boolean(session.worktreeId && !worktree);
  const availablePath = worktree?.path ?? session.worktreePath ?? session.cwd;
  const path = managedWorktreeUnavailable
    ? undefined
    : availablePath || undefined;
  const unavailableWorktreeReason = managedWorktreeUnavailable
    ? "The managed worktree is no longer available."
    : undefined;
  const disconnectedReason = !isConnected
    ? "Connect the local daemon first."
    : undefined;

  useEffect(() => {
    if (!actionsOpen) {
      return;
    }

    function closeForOutsideInteraction(event: Event) {
      const target = event.target;
      if (
        target instanceof Node &&
        !actionsContainerRef.current?.contains(target)
      ) {
        setActionsOpen(false);
      }
    }

    document.addEventListener("pointerdown", closeForOutsideInteraction, true);
    document.addEventListener("focusin", closeForOutsideInteraction);
    return () => {
      document.removeEventListener(
        "pointerdown",
        closeForOutsideInteraction,
        true,
      );
      document.removeEventListener("focusin", closeForOutsideInteraction);
    };
  }, [actionsOpen]);

  function closeActionDisclosure() {
    triggerRef.current?.focus();
    setActionsOpen(false);
  }

  async function runDirectAction(
    name: string,
    action: () => Promise<unknown>,
  ) {
    if (pendingAction) {
      return;
    }
    setPendingAction(name);
    setActionError(undefined);
    try {
      await action();
      closeActionDisclosure();
    } catch (error) {
      setActionError(errorData(error));
    } finally {
      setPendingAction(undefined);
    }
  }

  function runOverlayAction(action: () => void) {
    if (pendingAction) {
      return;
    }
    setActionError(undefined);
    try {
      action();
      closeActionDisclosure();
    } catch (error) {
      setActionError(errorData(error));
    }
  }

  return (
    <div
      ref={actionsContainerRef}
      className="canvas-node__session-actions"
      onPointerDown={(event) => event.stopPropagation()}
      onKeyDown={(event) => {
        if (actionsOpen && event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          closeActionDisclosure();
        }
      }}
    >
      <button
        ref={triggerRef}
        className="canvas-node__session-actions-trigger"
        type="button"
        aria-label={`Session actions for ${session.name}`}
        aria-expanded={actionsOpen}
        aria-controls={`canvas-session-actions-${session.id}`}
        title="Session actions"
        onClick={() => setActionsOpen((open) => !open)}
      >
        <Icon name="more" />
      </button>
      {actionsOpen ? (
        <div
          id={`canvas-session-actions-${session.id}`}
          className="canvas-node__session-actions-panel"
          data-browser-obstruction="true"
          role="group"
          aria-label={`Actions for ${session.name}`}
          aria-busy={pendingAction !== undefined}
        >
          <strong>{session.name}</strong>
          <StatusBadge status={session.status} compact />
          <SessionActionButton
            label="Start session"
            icon="play"
            pending={pendingAction === "start"}
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : live
                  ? "This session is already running."
                  : disconnectedReason ?? unavailableWorktreeReason
            }
            onClick={() =>
              void runDirectAction("start", () => onStartSession(session.id))
            }
          />
          <SessionActionButton
            label="Restart session"
            icon="refresh"
            pending={pendingAction === "restart"}
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : disconnectedReason ?? unavailableWorktreeReason
            }
            onClick={() =>
              void runDirectAction("restart", () =>
                onRestartSession(session.id),
              )
            }
          />
          <SessionActionButton
            label="Rename session"
            icon="pencil"
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : disconnectedReason
            }
            onClick={() => runOverlayAction(() => onRenameSession(session.id))}
          />
          <SessionActionButton
            label="Stop process"
            icon="stop"
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : disconnectedReason ??
                  (!live ? "This session has no live process." : undefined)
            }
            onClick={() => runOverlayAction(() => onStopSession(session.id))}
          />
          <SessionActionButton
            label="Git status"
            icon="branch"
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : disconnectedReason ?? unavailableWorktreeReason
            }
            onClick={() => runOverlayAction(() => onGitStatus(session.id))}
          />
          <SessionActionButton
            label="Open working directory"
            icon="folder"
            pending={pendingAction === "open-path"}
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : path
                  ? undefined
                  : managedWorktreeUnavailable
                    ? "The managed worktree path is no longer available."
                    : "This session has no working directory."
            }
            onClick={() => {
              if (path) {
                void runDirectAction("open-path", () => onOpenPath(path));
              }
            }}
          />
          <SessionActionButton
            label="Delete session metadata"
            icon="trash"
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : disconnectedReason ??
                  (live
                    ? "Stop the process before deleting the session."
                    : undefined)
            }
            onClick={() =>
              runOverlayAction(() => onDeleteSession(session.id))
            }
          />
          <SessionActionButton
            label="Remove worktree"
            icon="worktree"
            disabledReason={
              pendingAction
                ? "Another session action is in progress."
                : disconnectedReason ??
                  (live
                    ? "Stop the process before removing its worktree."
                    : worktree
                      ? undefined
                      : "This session has no available managed worktree.")
            }
            onClick={() => {
              if (worktree) {
                runOverlayAction(() => onRemoveWorktree(worktree.id));
              }
            }}
          />
          {actionError ? (
            <div className="canvas-node__session-action-error" role="alert">
              <strong>{actionError.message}</strong>
              {actionError.action ? <span>{actionError.action}</span> : null}
              <button
                type="button"
                aria-label="Dismiss session action error"
                onClick={() => setActionError(undefined)}
              >
                <Icon name="close" />
              </button>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function SessionActionButton({
  label,
  icon,
  pending = false,
  disabledReason,
  onClick,
}: {
  readonly label: string;
  readonly icon: Parameters<typeof Icon>[0]["name"];
  readonly pending?: boolean;
  readonly disabledReason?: string;
  readonly onClick: () => void;
}) {
  const disabledReasonId = useId();
  const disabled = disabledReason !== undefined;
  return (
    <>
      <button
        type="button"
        aria-disabled={disabled ? true : undefined}
        title={disabledReason}
        aria-busy={pending}
        aria-describedby={disabledReason ? disabledReasonId : undefined}
        onClick={() => {
          if (!disabled) {
            onClick();
          }
        }}
      >
        <Icon name={icon} />
        <span>{pending ? `${label}…` : label}</span>
      </button>
      {disabledReason ? (
        <span id={disabledReasonId} className="visually-hidden">
          {disabledReason}
        </span>
      ) : null}
    </>
  );
}

function keyboardMovement(
  key: string,
  step: number,
): { readonly x: number; readonly y: number } | null {
  switch (key) {
    case "ArrowUp":
      return { x: 0, y: -step };
    case "ArrowDown":
      return { x: 0, y: step };
    case "ArrowLeft":
      return { x: -step, y: 0 };
    case "ArrowRight":
      return { x: step, y: 0 };
    default:
      return null;
  }
}

function reconciledSessionPosition(index: number) {
  return {
    x: 170 + (index % 3) * 464,
    y: 720 + Math.floor(index / 3) * 288,
  };
}

function canvasScrollBehavior(): ScrollBehavior {
  return globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches
    ? "auto"
    : "smooth";
}

function terminalStartDisabledReason(
  session: Session | undefined,
  worktree: Worktree | undefined,
  isConnected: boolean,
): string | undefined {
  if (!isConnected) {
    return "Connect the local daemon first.";
  }
  if (session?.worktreeId && !worktree) {
    return "The managed worktree is no longer available.";
  }
  return undefined;
}

interface BrowserHandoff {
  readonly browser: BrowserCanvasNode;
  readonly target: NoteCanvasNode | TerminalCanvasNode;
}

function getBrowserHandoff(
  selectedNode: CanvasNode,
  otherNode: CanvasNode,
): BrowserHandoff | null {
  if (selectedNode.kind === "browser" && otherNode.kind !== "browser") {
    return { browser: selectedNode, target: otherNode };
  }
  if (otherNode.kind === "browser" && selectedNode.kind !== "browser") {
    return { browser: otherNode, target: selectedNode };
  }
  return null;
}

function browserHandoffLabel(
  target: NoteCanvasNode | TerminalCanvasNode,
): string {
  return target.kind === "note"
    ? `Add browser URL to ${target.title}`
    : `Insert browser URL into ${target.title}`;
}

function keyboardResize(
  key: string,
  node: TerminalCanvasNode | BrowserCanvasNode,
  step: number,
): { readonly width: number; readonly height: number } | null {
  switch (key) {
    case "ArrowUp":
      return { width: node.width, height: node.height - step };
    case "ArrowDown":
      return { width: node.width, height: node.height + step };
    case "ArrowLeft":
      return { width: node.width - step, height: node.height };
    case "ArrowRight":
      return { width: node.width + step, height: node.height };
    default:
      return null;
  }
}

function TerminalNodeBody({
  node,
  agent,
  session,
  onStart,
  pending,
  error,
  startDisabledReason,
  transport,
  inputRef,
}: {
  readonly node: TerminalCanvasNode;
  readonly agent?: AgentRecord;
  readonly session?: Session;
  readonly onStart: () => void;
  readonly pending: boolean;
  readonly error?: string;
  readonly startDisabledReason?: string;
  readonly transport: LiveTerminalTransport;
  readonly inputRef: Ref<LiveTerminalInputHandle>;
}) {
  const live = session ? isLiveStatus(session.status) : false;
  const startDisabled = pending || startDisabledReason !== undefined;
  return (
    <div
      className="canvas-terminal"
      role="region"
      aria-label={`Terminal surface for ${node.title}`}
      data-terminal-root="true"
    >
      <div className="canvas-terminal__status">
        {session ? (
          <>
            <StatusBadge status={session.status} compact />
            <span className="mono">{session.branch ?? session.cwd}</span>
          </>
        ) : (
          <span className="canvas-terminal__draft">
            <Icon name="terminal" /> {agent?.displayName ?? node.executable ?? (node.agentId ? "Saved agent" : "Shell")} draft
          </span>
        )}
      </div>
      {session && live ? (
        <div className="canvas-terminal__body canvas-terminal__body--live">
          <LiveTerminal session={session} inputRef={inputRef} {...transport} />
        </div>
      ) : (
        <div className="canvas-terminal__body">
          <Icon name="terminal" />
          <p>
            {error ??
              startDisabledReason ??
              (session
                ? "This terminal is stopped. Start it to attach a fresh live PTY."
                : `${agent?.displayName ?? node.executable ?? (node.agentId ? "The saved agent" : "A login shell")} is ready to start in this project.`)}
          </p>
          <div className="canvas-terminal__body-actions">
            <button
              type="button"
              aria-disabled={startDisabled ? true : undefined}
              title={startDisabledReason}
              aria-busy={pending}
              onClick={() => {
                if (!startDisabled) {
                  onStart();
                }
              }}
            >
              {pending ? "Starting…" : error ? "Retry terminal" : "Start terminal"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

async function resolveTerminalAgent(
  node: TerminalCanvasNode,
  agents: readonly AgentRecord[],
  createCustomAgent: (
    input: CreateCustomAgentInput,
  ) => Promise<AgentRecord>,
): Promise<AgentRecord> {
  if (node.agentId) {
    const savedAgent = agents.find((agent) => agent.id === node.agentId);
    if (!savedAgent) {
      throw new Error("The original agent is unavailable. Restore it before starting this copy.");
    }
    if (!savedAgent.enabled) {
      throw new Error("The original agent is disabled. Enable it before starting this copy.");
    }
    return savedAgent;
  }
  if (node.preset === "custom") {
    if (!node.executable) {
      throw new Error("Choose an executable before starting this terminal.");
    }
    return createCustomAgent({
      displayName: node.title,
      command: { executable: node.executable, args: [], env: {} },
    });
  }
  const executable = node.preset === "shell" ? undefined : node.executable;
  const match = agents.find((agent) => {
    if (!agent.enabled) {
      return false;
    }
    if (node.preset === "shell") {
      return agent.displayName.toLowerCase() === "shell";
    }
    const commandName = agent.command.executable.split(/[\\/]/).pop();
    return commandName?.toLowerCase() === executable?.toLowerCase();
  });
  if (!match) {
    throw new Error(
      `${node.preset === "shell" ? "Shell" : node.title} is not available in Jig yet.`,
    );
  }
  return match;
}

function relativeWorkingDirectory(
  project: Project,
  selectedDirectory?: string,
): string | undefined {
  if (!selectedDirectory || selectedDirectory === "~") {
    return undefined;
  }
  const root = (project.repositoryRoot ?? project.path).replace(/\/$/, "");
  const selected = selectedDirectory.replace(/\/$/, "");
  if (selected === root || selected === project.path.replace(/\/$/, "")) {
    return undefined;
  }
  if (!selected.startsWith(`${root}/`)) {
    throw new Error("Choose a working directory inside the selected project.");
  }
  return selected.slice(root.length + 1);
}

function terminalErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return "Jig could not start this terminal. Try again.";
}

function NoteNodeBody({
  node,
  onChange,
}: {
  readonly node: NoteCanvasNode;
  readonly onChange: (text: string) => void;
}) {
  return (
    <label className="canvas-note">
      <span className="visually-hidden">{node.title}</span>
      <textarea
        value={node.text}
        maxLength={50_000}
        aria-label={`${node.title} content`}
        placeholder="Write a note…"
        onChange={(event) => onChange(event.currentTarget.value)}
        onPointerDown={(event) => event.stopPropagation()}
      />
    </label>
  );
}

function isCanvasEditingTarget(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    target.closest(
      "input, textarea, select, button, a, summary, [contenteditable]:not([contenteditable='false']), [role='textbox'], [data-terminal-root], [data-shortcut-scope], .xterm, [role='dialog']",
    ) !== null
  );
}
