export const CANVAS_STORAGE_KEY = "cli-master.canvas.v1";
export const CANVAS_DOCUMENT_VERSION = 2;
export const CANVAS_DOCUMENT_UPDATED_EVENT = "cli-master:canvas-document-updated";

export type CanvasNodeKind = "terminal" | "note" | "browser";
export type TerminalPreset = "shell" | "codex" | "claude" | "gemini" | "opencode" | "custom";

export const DEFAULT_TERMINAL_SIZE = { width: 432, height: 256 } as const;
export const DEFAULT_BROWSER_SIZE = { width: 640, height: 420 } as const;
export const NOTE_SIZE = { width: 288, height: 288 } as const;

export interface CanvasPoint {
  readonly x: number;
  readonly y: number;
}

interface CanvasNodeBase extends CanvasPoint {
  readonly id: string;
  readonly title: string;
  readonly kind: CanvasNodeKind;
  /** New nodes are scoped to a project; absent means a legacy shared node. */
  readonly projectId?: string;
}

export interface TerminalCanvasNode extends CanvasNodeBase {
  readonly kind: "terminal";
  readonly sessionId?: string;
  /** Reference the persisted agent definition without copying its environment. */
  readonly agentId?: string;
  /** User-authored composer text, never PTY output or an auto-send instruction. */
  readonly promptDraft?: string;
  readonly promptDraftRevision?: number;
  readonly preset: TerminalPreset;
  readonly executable?: string;
  readonly workingDirectory?: string;
  readonly width: number;
  readonly height: number;
}

export interface NoteCanvasNode extends CanvasNodeBase {
  readonly kind: "note";
  readonly text: string;
}

export interface BrowserCanvasNode extends CanvasNodeBase {
  readonly kind: "browser";
  readonly url: string;
  readonly width: number;
  readonly height: number;
}

export type CanvasNode = TerminalCanvasNode | NoteCanvasNode | BrowserCanvasNode;

export interface CanvasTerminalConfiguration {
  readonly title: string;
  readonly preset: TerminalPreset;
  readonly executable?: string;
  readonly workingDirectory?: string;
}

export interface CanvasConnection {
  readonly id: string;
  readonly sourceNodeId: string;
  readonly targetNodeId: string;
}

export interface CanvasDocument {
  readonly version: 2;
  readonly nodes: readonly CanvasNode[];
  readonly connections: readonly CanvasConnection[];
  readonly zoom: number;
  /** Session cards dismissed from the canvas without deleting session metadata. */
  readonly hiddenSessionIds?: readonly string[];
}

export interface CanvasState extends CanvasDocument {
  readonly hiddenSessionIds: readonly string[];
  readonly selectedNodeId: string | null;
  readonly selectedNodeIds: readonly string[];
  readonly connectionSourceId: string | null;
}

export interface CanvasSessionReference {
  readonly id: string;
  readonly projectId: string;
  readonly name: string;
  readonly cwd: string;
  readonly agentId?: string;
}

export type CanvasAction =
  | { readonly type: "document/hydrate"; readonly document: CanvasDocument }
  | { readonly type: "node/add"; readonly node: CanvasNode }
  | {
      readonly type: "node/move";
      readonly nodeId: string;
      readonly position: CanvasPoint;
    }
  | {
      readonly type: "node/rename";
      readonly nodeId: string;
      readonly title: string;
    }
  | {
      readonly type: "note/update";
      readonly nodeId: string;
      readonly text: string;
    }
  | {
      readonly type: "terminal/draft";
      readonly nodeId: string;
      readonly text: string;
    }
  | {
      readonly type: "terminal/draft_sent";
      readonly nodeId: string;
      readonly text: string;
      readonly revision: number;
    }
  | {
      readonly type: "terminal/configure";
      readonly nodeId: string;
      readonly configuration: CanvasTerminalConfiguration;
    }
  | {
      readonly type: "node/resize" | "terminal/resize";
      readonly nodeId: string;
      readonly size: { readonly width: number; readonly height: number };
    }
  | {
      readonly type: "terminal/attach";
      readonly nodeId: string;
      readonly sessionId: string;
      readonly projectId: string;
    }
  | {
      readonly type: "sessions/reconcile";
      readonly knownSessionIds: readonly string[];
      readonly sessionNodes: readonly TerminalCanvasNode[];
    }
  | {
      readonly type: "session/reveal";
      readonly node: TerminalCanvasNode;
    }
  | {
      readonly type: "browser/navigate";
      readonly nodeId: string;
      readonly url: string;
    }
  | { readonly type: "node/delete"; readonly nodeId: string }
  | { readonly type: "node/select"; readonly nodeId: string | null; readonly additive?: boolean }
  | { readonly type: "nodes/select"; readonly nodeIds: readonly string[] }
  | { readonly type: "nodes/move"; readonly nodeIds: readonly string[]; readonly delta: CanvasPoint }
  | { readonly type: "nodes/delete"; readonly nodeIds: readonly string[] }
  | { readonly type: "nodes/duplicate"; readonly nodes: readonly CanvasNode[]; readonly connections: readonly CanvasConnection[] }
  | { readonly type: "connection/start"; readonly nodeId: string }
  | { readonly type: "connection/complete"; readonly targetNodeId: string }
  | { readonly type: "connection/cancel" }
  | { readonly type: "connection/delete"; readonly connectionId: string }
  | { readonly type: "zoom/set"; readonly zoom: number };

const MIN_POSITION = -2_000;
const MAX_POSITION = 8_000;
const MIN_ZOOM = 0.5;
const MAX_ZOOM = 1.5;
const MAX_TITLE_LENGTH = 80;
const MAX_NOTE_LENGTH = 50_000;
const MIN_TERMINAL_WIDTH = 320;
const MAX_TERMINAL_WIDTH = 960;
const MIN_TERMINAL_HEIGHT = 192;
const MAX_TERMINAL_HEIGHT = 720;
const MIN_BROWSER_WIDTH = 420;
const MAX_BROWSER_WIDTH = 1_280;
const MIN_BROWSER_HEIGHT = 320;
const MAX_BROWSER_HEIGHT = 900;
const MAX_EXECUTABLE_LENGTH = 256;
const MAX_WORKING_DIRECTORY_LENGTH = 1_024;
const MAX_BROWSER_URL_LENGTH = 2_048;
const SENSITIVE_BROWSER_QUERY_KEYS = new Set([
  "accesstoken",
  "apikey",
  "authorization",
  "auth",
  "clientsecret",
  "code",
  "credential",
  "credentials",
  "idtoken",
  "key",
  "password",
  "passwd",
  "policy",
  "pwd",
  "refreshtoken",
  "samlresponse",
  "secret",
  "session",
  "sessionid",
  "sig",
  "signature",
  "state",
  "token",
]);

/** Creates the first-launch composition shown before project sessions exist. */
export function createInitialCanvasDocument(): CanvasDocument {
  const nodes: readonly CanvasNode[] = [
    createTerminalCanvasNode(
      { x: 170, y: 210 },
      { title: "Terminal 1", preset: "shell" },
      "terminal-primary",
    ),
    createTerminalCanvasNode(
      { x: 560, y: 90 },
      { title: "Terminal 2", preset: "shell" },
      "terminal-secondary",
    ),
    {
      id: "note-first",
      kind: "note",
      title: "Notes",
      text: "Write a note for this workspace…",
      x: 600,
      y: 390,
    },
  ];
  return {
    version: CANVAS_DOCUMENT_VERSION,
    nodes,
    connections: [
      createConnection(nodes[1]?.id ?? "", nodes[0]?.id ?? ""),
      createConnection(nodes[0]?.id ?? "", nodes[2]?.id ?? ""),
    ],
    zoom: 1,
    hiddenSessionIds: [],
  };
}

export function createInitialCanvasState(
  document: CanvasDocument = createInitialCanvasDocument(),
): CanvasState {
  return {
    ...document,
    hiddenSessionIds: document.hiddenSessionIds ?? [],
    selectedNodeId: null,
    selectedNodeIds: [],
    connectionSourceId: null,
  };
}

export function canvasReducer(
  state: CanvasState,
  action: CanvasAction,
): CanvasState {
  switch (action.type) {
    case "document/hydrate":
      return createInitialCanvasState(action.document);
    case "node/add":
      if (state.nodes.some((node) => node.id === action.node.id)) {
        return state;
      }
      return {
        ...state,
        nodes: [...state.nodes, normalizeNode(action.node)],
        selectedNodeId: action.node.id,
        selectedNodeIds: [action.node.id],
      };
    case "nodes/select":
      return selectNodes(state, action.nodeIds);
    case "nodes/move":
      return moveNodes(state, action.nodeIds, action.delta);
    case "nodes/delete":
      return deleteNodes(state, action.nodeIds);
    case "nodes/duplicate": {
      const nodes = action.nodes.map(normalizeNode);
      if (nodes.length === 0 || new Set(nodes.map((node) => node.id)).size !== nodes.length ||
          nodes.some((node) => nodeExists(state.nodes, node.id))) {
        return state;
      }
      const nodeIds = new Set(nodes.map((node) => node.id));
      const connections = action.connections.flatMap((connection) => {
        const parsed = parseConnection(connection, nodeIds);
        return parsed ? [parsed] : [];
      });
      return selectNodes({
        ...state,
        nodes: [...state.nodes, ...nodes],
        connections: [...state.connections, ...connections],
        connectionSourceId: null,
      }, nodes.map((node) => node.id));
    }
    case "node/move":
      return updateNode(state, action.nodeId, (node) => ({
        ...node,
        x: clamp(action.position.x, MIN_POSITION, MAX_POSITION),
        y: clamp(action.position.y, MIN_POSITION, MAX_POSITION),
      }));
    case "node/rename":
      return updateNode(state, action.nodeId, (node) => ({
        ...node,
        title: normalizeTitle(action.title, node.title),
      }));
    case "note/update":
      return updateNode(state, action.nodeId, (node) =>
        node.kind === "note"
          ? { ...node, text: action.text.slice(0, MAX_NOTE_LENGTH) }
          : node,
      );
    case "terminal/draft":
      return updateNode(state, action.nodeId, (node) =>
        node.kind === "terminal" ? updatePromptDraft(node, action.text) : node,
      );
    case "terminal/draft_sent":
      return updateNode(state, action.nodeId, (node) =>
        node.kind === "terminal" && (node.promptDraft ?? "") === action.text
          && (node.promptDraftRevision ?? 0) === action.revision
          ? updatePromptDraft(node, "") : node,
      );
    case "terminal/configure":
      return updateNode(state, action.nodeId, (node) =>
        node.kind === "terminal"
          ? configureTerminalNode(node, action.configuration)
          : node,
      );
    case "node/resize":
      return updateNode(state, action.nodeId, (node) =>
        resizeCanvasNode(node, action.size),
      );
    case "terminal/resize":
      return updateNode(state, action.nodeId, (node) =>
        node.kind === "terminal" ? resizeCanvasNode(node, action.size) : node,
      );
    case "terminal/attach":
      return updateNode(state, action.nodeId, (node) =>
        node.kind === "terminal"
          ? {
              ...node,
              sessionId: action.sessionId,
              projectId: action.projectId,
            }
          : node,
      );
    case "sessions/reconcile":
      return reconcileSessionNodes(
        state,
        action.knownSessionIds,
        action.sessionNodes,
      );
    case "session/reveal":
      return revealSessionNode(state, action.node);
    case "browser/navigate":
      return updateNode(state, action.nodeId, (node) =>
        node.kind === "browser"
          ? navigateBrowserNode(node, action.url)
          : node,
      );
    case "node/delete":
      return deleteNodes(state, [action.nodeId]);
    case "node/select":
      if (action.nodeId === null) {
        return selectNodes(state, []);
      }
      if (!nodeExists(state.nodes, action.nodeId)) return state;
      return selectNodes(state, action.additive
        ? state.selectedNodeIds.includes(action.nodeId)
          ? state.selectedNodeIds.filter((id) => id !== action.nodeId)
          : [...state.selectedNodeIds, action.nodeId]
        : [action.nodeId]);
    case "connection/start":
      return nodeExists(state.nodes, action.nodeId)
        ? {
            ...state,
            selectedNodeId: action.nodeId,
            selectedNodeIds: [action.nodeId],
            connectionSourceId: action.nodeId,
          }
        : state;
    case "connection/complete":
      return completeConnection(state, action.targetNodeId);
    case "connection/cancel":
      return { ...state, connectionSourceId: null };
    case "connection/delete":
      return {
        ...state,
        connections: state.connections.filter(
          (connection) => connection.id !== action.connectionId,
        ),
      };
    case "zoom/set":
      return {
        ...state,
        zoom: clamp(action.zoom, MIN_ZOOM, MAX_ZOOM),
      };
  }
}

export function toCanvasDocument(state: CanvasDocument): CanvasDocument {
  return {
    version: CANVAS_DOCUMENT_VERSION,
    nodes: state.nodes.map((node) => node.kind === "browser"
      ? { ...node, url: normalizeBrowserUrl(node.url) }
      : node),
    connections: state.connections,
    zoom: state.zoom,
    hiddenSessionIds: state.hiddenSessionIds,
  };
}

export function parseCanvasDocument(value: string | null): CanvasDocument {
  if (!value) {
    return createInitialCanvasDocument();
  }
  try {
    return normalizeDocument(JSON.parse(value));
  } catch {
    return createInitialCanvasDocument();
  }
}

export function serializeCanvasDocument(state: CanvasState): string {
  return JSON.stringify(toCanvasDocument(state));
}

/** Copies graph metadata, never a running process or its terminal stream. */
export function duplicateCanvasSelection(
  state: CanvasState,
  nodeIds: readonly string[],
): CanvasAction {
  const selectedIds = new Set(nodeIds);
  const originals = state.nodes.filter((node) => selectedIds.has(node.id));
  const idMap = new Map(originals.map((node) => [node.id, createId(node.kind)]));
  const delta = boundedMoveDelta(originals, { x: 32, y: 32 });
  const nodes = originals.map((node): CanvasNode => ({
    ...node,
    ...(node.kind === "terminal" ? { sessionId: undefined } : {}),
    id: idMap.get(node.id)!,
    title: `${node.title.slice(0, MAX_TITLE_LENGTH - 5)} copy`,
    x: node.x + delta.x,
    y: node.y + delta.y,
  }));
  const connections = state.connections.flatMap((connection) => {
    const source = idMap.get(connection.sourceNodeId);
    const target = idMap.get(connection.targetNodeId);
    return source && target ? [createConnection(source, target)] : [];
  });
  return { type: "nodes/duplicate", nodes, connections };
}

export function createCanvasNode(
  kind: CanvasNodeKind,
  position: CanvasPoint,
  id = createId(kind),
): CanvasNode {
  const normalizedPosition = {
    x: clamp(position.x, MIN_POSITION, MAX_POSITION),
    y: clamp(position.y, MIN_POSITION, MAX_POSITION),
  };
  if (kind === "note") {
    return {
      id,
      kind,
      title: "Notes",
      text: "",
      ...normalizedPosition,
    };
  }
  if (kind === "browser") {
    return createBrowserCanvasNode(normalizedPosition, "", id);
  }
  return createTerminalCanvasNode(normalizedPosition, {}, id);
}

export function createBrowserCanvasNode(
  position: CanvasPoint,
  url = "",
  id = createId("browser"),
): BrowserCanvasNode {
  return {
    id,
    kind: "browser",
    title: "Browser",
    url: normalizeBrowserUrl(url),
    x: clamp(position.x, MIN_POSITION, MAX_POSITION),
    y: clamp(position.y, MIN_POSITION, MAX_POSITION),
    ...DEFAULT_BROWSER_SIZE,
  };
}

export function createTerminalCanvasNode(
  position: CanvasPoint,
  configuration: Partial<CanvasTerminalConfiguration> = {},
  id = createId("terminal"),
): TerminalCanvasNode {
  const preset = normalizeTerminalPreset(configuration.preset);
  const executable = normalizeOptionalText(
    configuration.executable ?? executableForPreset(preset),
    MAX_EXECUTABLE_LENGTH,
  );
  return {
    id,
    kind: "terminal",
    title: normalizeTitle(configuration.title, titleForPreset(preset)),
    preset,
    executable,
    workingDirectory: normalizeOptionalText(
      configuration.workingDirectory,
      MAX_WORKING_DIRECTORY_LENGTH,
    ),
    x: clamp(position.x, MIN_POSITION, MAX_POSITION),
    y: clamp(position.y, MIN_POSITION, MAX_POSITION),
    ...DEFAULT_TERMINAL_SIZE,
  };
}

/** Creates the stable canvas representation of an existing daemon session. */
export function createSessionTerminalCanvasNode(
  position: CanvasPoint,
  session: CanvasSessionReference,
): TerminalCanvasNode {
  return {
    ...createTerminalCanvasNode(
      position,
      {
        title: session.name,
        preset: "shell",
        workingDirectory: session.cwd,
      },
      `terminal-session-${session.id}`,
    ),
    sessionId: session.id,
    agentId: session.agentId,
    projectId: session.projectId,
  };
}

export function getCanvasNodeSize(
  node: CanvasNode,
): { readonly width: number; readonly height: number } {
  return node.kind === "note"
    ? NOTE_SIZE
    : { width: node.width, height: node.height };
}

/** Resolves a user-entered address for transient HTTP(S) navigation. */
export function normalizeBrowserNavigationUrl(value: unknown): string {
  if (typeof value !== "string") {
    return "";
  }
  const trimmed = value.trim();
  if (
    !trimmed ||
    trimmed.length > MAX_BROWSER_URL_LENGTH ||
    /[\u0000-\u001f\u007f]/.test(trimmed)
  ) {
    return "";
  }
  const candidate = /^[a-z][a-z\d+.-]*:/i.test(trimmed)
    ? trimmed
    : `https://${trimmed}`;
  try {
    const url = new URL(candidate);
    if (
      (url.protocol !== "https:" && url.protocol !== "http:") ||
      !url.hostname ||
      url.username ||
      url.password
    ) {
      return "";
    }
    return url.toString();
  } catch {
    return "";
  }
}

/** Redacts a valid browser address before canvas persistence or handoff. */
export function normalizeBrowserUrl(value: unknown): string {
  const navigationUrl = normalizeBrowserNavigationUrl(value);
  if (!navigationUrl) {
    return "";
  }
  const url = new URL(navigationUrl);
  url.hash = "";
  for (const key of [...url.searchParams.keys()]) {
    if (isSensitiveBrowserQueryKey(key)) {
      url.searchParams.delete(key);
    }
  }
  return url.toString();
}

function isSensitiveBrowserQueryKey(value: string): boolean {
  const key = value.toLowerCase().replace(/[^a-z0-9]/g, "");
  return (
    key.startsWith("xamz") ||
    key.endsWith("token") ||
    SENSITIVE_BROWSER_QUERY_KEYS.has(key)
  );
}

function navigateBrowserNode(
  node: BrowserCanvasNode,
  value: unknown,
): BrowserCanvasNode {
  const url = normalizeBrowserUrl(value);
  return url ? { ...node, url } : node;
}

function reconcileSessionNodes(
  state: CanvasState,
  knownSessionIds: readonly string[],
  sessionNodes: readonly TerminalCanvasNode[],
): CanvasState {
  const knownSessionIdSet = new Set(knownSessionIds);
  const hiddenSessionIds = state.hiddenSessionIds.filter((sessionId) =>
    knownSessionIdSet.has(sessionId),
  );
  const hiddenSessionIdSet = new Set(hiddenSessionIds);
  let nodes = state.nodes;

  for (const sessionNode of sessionNodes) {
    if (!sessionNode.sessionId) {
      continue;
    }
    const existingIndex = nodes.findIndex(
      (node) =>
        node.kind === "terminal" && node.sessionId === sessionNode.sessionId,
    );
    if (existingIndex === -1) {
      if (!hiddenSessionIdSet.has(sessionNode.sessionId)) {
        nodes = [...nodes, normalizeNode(sessionNode)];
      }
      continue;
    }

    const existingNode = nodes[existingIndex];
    if (
      existingNode?.kind === "terminal" &&
      (existingNode.title !== sessionNode.title ||
        existingNode.projectId !== sessionNode.projectId ||
        existingNode.agentId !== sessionNode.agentId)
    ) {
      nodes = nodes.map((node, index) =>
        index === existingIndex
          ? {
              ...existingNode,
              title: sessionNode.title,
              projectId: sessionNode.projectId,
              agentId: sessionNode.agentId,
            }
          : node,
      );
    }
  }

  if (
    nodes === state.nodes &&
    stringArraysEqual(hiddenSessionIds, state.hiddenSessionIds)
  ) {
    return state;
  }
  return { ...state, nodes, hiddenSessionIds };
}

function revealSessionNode(
  state: CanvasState,
  sessionNode: TerminalCanvasNode,
): CanvasState {
  if (!sessionNode.sessionId) {
    return state;
  }
  const existingNode = state.nodes.find(
    (node) =>
      node.kind === "terminal" && node.sessionId === sessionNode.sessionId,
  );
  const revealedNode = existingNode ?? normalizeNode(sessionNode);
  if (revealedNode.kind !== "terminal") {
    return state;
  }
  return {
    ...state,
    nodes: existingNode ? state.nodes : [...state.nodes, revealedNode],
    hiddenSessionIds: state.hiddenSessionIds.filter(
      (sessionId) => sessionId !== sessionNode.sessionId,
    ),
    selectedNodeId: revealedNode.id,
    selectedNodeIds: [revealedNode.id],
    connectionSourceId: null,
  };
}

function completeConnection(
  state: CanvasState,
  targetNodeId: string,
): CanvasState {
  const sourceNodeId = state.connectionSourceId;
  if (
    !sourceNodeId ||
    sourceNodeId === targetNodeId ||
    !nodeExists(state.nodes, targetNodeId)
  ) {
    return { ...state, connectionSourceId: null };
  }
  const duplicate = state.connections.some(
    (connection) =>
      (connection.sourceNodeId === sourceNodeId &&
        connection.targetNodeId === targetNodeId) ||
      (connection.sourceNodeId === targetNodeId &&
        connection.targetNodeId === sourceNodeId),
  );
  return {
    ...state,
    selectedNodeId: targetNodeId,
    selectedNodeIds: [targetNodeId],
    connectionSourceId: null,
    connections: duplicate
      ? state.connections
      : [...state.connections, createConnection(sourceNodeId, targetNodeId)],
  };
}

function createConnection(
  sourceNodeId: string,
  targetNodeId: string,
): CanvasConnection {
  return {
    id: `connection-${sourceNodeId}-${targetNodeId}`,
    sourceNodeId,
    targetNodeId,
  };
}

function selectNodes(state: CanvasState, nodeIds: readonly string[]): CanvasState {
  const selectedNodeIds = [...new Set(nodeIds)].filter((id) => nodeExists(state.nodes, id));
  if (stringArraysEqual(selectedNodeIds, state.selectedNodeIds)) return state;
  return {
    ...state,
    selectedNodeIds,
    selectedNodeId: selectedNodeIds[selectedNodeIds.length - 1] ?? null,
  };
}

function boundedMoveDelta(nodes: readonly CanvasNode[], delta: CanvasPoint): CanvasPoint {
  if (nodes.length === 0 || !Number.isFinite(delta.x) || !Number.isFinite(delta.y)) {
    return { x: 0, y: 0 };
  }
  // Clamp the entire group once so cards keep their relative positions at edges.
  return nodes.reduce((bounded, node) => ({
    x: clamp(bounded.x, MIN_POSITION - node.x, MAX_POSITION - node.x),
    y: clamp(bounded.y, MIN_POSITION - node.y, MAX_POSITION - node.y),
  }), delta);
}

function moveNodes(state: CanvasState, nodeIds: readonly string[], delta: CanvasPoint): CanvasState {
  const ids = new Set(nodeIds);
  const bounded = boundedMoveDelta(state.nodes.filter((node) => ids.has(node.id)), delta);
  if (bounded.x === 0 && bounded.y === 0) return state;
  return {
    ...state,
    nodes: state.nodes.map((node) => ids.has(node.id)
      ? { ...node, x: node.x + bounded.x, y: node.y + bounded.y }
      : node),
  };
}

function deleteNodes(state: CanvasState, nodeIds: readonly string[]): CanvasState {
  const ids = new Set(nodeIds);
  const deletedNodes = state.nodes.filter((node) => ids.has(node.id));
  if (deletedNodes.length === 0) return state;
  const selectedNodeIds = state.selectedNodeIds.filter((id) => !ids.has(id));
  return {
    ...state,
    nodes: state.nodes.filter((node) => !ids.has(node.id)),
    connections: state.connections.filter((connection) =>
      !ids.has(connection.sourceNodeId) && !ids.has(connection.targetNodeId)),
    selectedNodeIds,
    selectedNodeId: selectedNodeIds[selectedNodeIds.length - 1] ?? null,
    connectionSourceId: state.connectionSourceId && ids.has(state.connectionSourceId)
      ? null : state.connectionSourceId,
    hiddenSessionIds: [...new Set([
      ...state.hiddenSessionIds,
      ...deletedNodes.flatMap((node) => node.kind === "terminal" && node.sessionId ? [node.sessionId] : []),
    ])],
  };
}

function updateNode(
  state: CanvasState,
  nodeId: string,
  update: (node: CanvasNode) => CanvasNode,
): CanvasState {
  const current = state.nodes.find((node) => node.id === nodeId);
  if (!current) return state;
  const updated = update(current);
  if (updated === current) return state;
  return {
    ...state,
    nodes: state.nodes.map((node) =>
      node === current ? updated : node,
    ),
  };
}

function nodeExists(
  nodes: readonly CanvasNode[],
  nodeId: string | null,
): nodeId is string {
  return nodeId !== null && nodes.some((node) => node.id === nodeId);
}

function normalizeDocument(value: unknown): CanvasDocument {
  if (
    !isRecord(value) ||
    (value.version !== 1 && value.version !== CANVAS_DOCUMENT_VERSION)
  ) {
    return createInitialCanvasDocument();
  }
  const nodes = Array.isArray(value.nodes)
    ? value.nodes.flatMap((node) => {
        const normalized = parseNode(node);
        return normalized ? [normalized] : [];
      })
    : [];
  const nodeIds = new Set(nodes.map((node) => node.id));
  const connections = Array.isArray(value.connections)
    ? value.connections.flatMap((connection) => {
        const normalized = parseConnection(connection, nodeIds);
        return normalized ? [normalized] : [];
      })
    : [];
  return {
    version: CANVAS_DOCUMENT_VERSION,
    nodes,
    connections,
    zoom: normalizeNumber(value.zoom, 1, MIN_ZOOM, MAX_ZOOM),
    hiddenSessionIds: normalizeStringArray(value.hiddenSessionIds),
  };
}

function parseNode(value: unknown): CanvasNode | null {
  if (
    !isRecord(value) ||
    typeof value.id !== "string" ||
    (value.kind !== "terminal" &&
      value.kind !== "note" &&
      value.kind !== "browser")
  ) {
    return null;
  }
  const base = {
    id: value.id,
    kind: value.kind,
    projectId:
      typeof value.projectId === "string" ? value.projectId : undefined,
    title: normalizeTitle(
      value.title,
      value.kind === "note"
        ? "Notes"
        : value.kind === "browser"
          ? "Browser"
          : "Terminal",
    ),
    x: normalizeNumber(value.x, 0, MIN_POSITION, MAX_POSITION),
    y: normalizeNumber(value.y, 0, MIN_POSITION, MAX_POSITION),
  };
  if (value.kind === "note") {
    return {
      ...base,
      kind: "note",
      text:
        typeof value.text === "string"
          ? value.text.slice(0, MAX_NOTE_LENGTH)
          : "",
    };
  }
  if (value.kind === "browser") {
    return {
      ...base,
      kind: "browser",
      url: normalizeBrowserUrl(value.url),
      width: normalizeNumber(
        value.width,
        DEFAULT_BROWSER_SIZE.width,
        MIN_BROWSER_WIDTH,
        MAX_BROWSER_WIDTH,
      ),
      height: normalizeNumber(
        value.height,
        DEFAULT_BROWSER_SIZE.height,
        MIN_BROWSER_HEIGHT,
        MAX_BROWSER_HEIGHT,
      ),
    };
  }
  return {
    ...base,
    kind: "terminal",
    preset: normalizeTerminalPreset(value.preset),
    executable: normalizeOptionalText(
      typeof value.executable === "string"
        ? value.executable
        : executableForPreset(normalizeTerminalPreset(value.preset)),
      MAX_EXECUTABLE_LENGTH,
    ),
    workingDirectory: normalizeOptionalText(
      value.workingDirectory,
      MAX_WORKING_DIRECTORY_LENGTH,
    ),
    width: normalizeNumber(
      value.width,
      DEFAULT_TERMINAL_SIZE.width,
      MIN_TERMINAL_WIDTH,
      MAX_TERMINAL_WIDTH,
    ),
    height: normalizeNumber(
      value.height,
      DEFAULT_TERMINAL_SIZE.height,
      MIN_TERMINAL_HEIGHT,
      MAX_TERMINAL_HEIGHT,
    ),
    sessionId:
      typeof value.sessionId === "string" ? value.sessionId : undefined,
    agentId:
      typeof value.agentId === "string" ? value.agentId : undefined,
    promptDraft: typeof value.promptDraft === "string" ? value.promptDraft : undefined,
    promptDraftRevision: typeof value.promptDraft === "string"
      ? normalizePromptDraftRevision(value.promptDraftRevision) : undefined,
  };
}

function normalizePromptDraftRevision(value: unknown): number {
  return typeof value === "number" && Number.isSafeInteger(value)
    && value >= 0 && value < Number.MAX_SAFE_INTEGER ? value : 0;
}

function updatePromptDraft(node: TerminalCanvasNode, text: string): TerminalCanvasNode {
  if ((node.promptDraft ?? "") === text) return node;
  return {
    ...node,
    promptDraft: text,
    promptDraftRevision: normalizePromptDraftRevision(node.promptDraftRevision) + 1,
  };
}

function configureTerminalNode(
  node: TerminalCanvasNode,
  configuration: CanvasTerminalConfiguration,
): TerminalCanvasNode {
  const preset = normalizeTerminalPreset(configuration.preset);
  return {
    ...node,
    title: normalizeTitle(configuration.title, titleForPreset(preset)),
    preset,
    executable: normalizeOptionalText(
      configuration.executable ?? executableForPreset(preset),
      MAX_EXECUTABLE_LENGTH,
    ),
    workingDirectory: normalizeOptionalText(
      configuration.workingDirectory,
      MAX_WORKING_DIRECTORY_LENGTH,
    ),
  };
}

function resizeCanvasNode(
  node: CanvasNode,
  size: { readonly width: number; readonly height: number },
): CanvasNode {
  if (node.kind === "note") {
    return node;
  }
  const limits =
    node.kind === "browser"
      ? {
          minimumWidth: MIN_BROWSER_WIDTH,
          maximumWidth: MAX_BROWSER_WIDTH,
          minimumHeight: MIN_BROWSER_HEIGHT,
          maximumHeight: MAX_BROWSER_HEIGHT,
        }
      : {
          minimumWidth: MIN_TERMINAL_WIDTH,
          maximumWidth: MAX_TERMINAL_WIDTH,
          minimumHeight: MIN_TERMINAL_HEIGHT,
          maximumHeight: MAX_TERMINAL_HEIGHT,
        };
  return {
    ...node,
    width: normalizeNumber(
      size.width,
      node.width,
      limits.minimumWidth,
      limits.maximumWidth,
    ),
    height: normalizeNumber(
      size.height,
      node.height,
      limits.minimumHeight,
      limits.maximumHeight,
    ),
  };
}

function normalizeTerminalPreset(value: unknown): TerminalPreset {
  return value === "codex" ||
    value === "claude" ||
    value === "gemini" ||
    value === "opencode" ||
    value === "custom"
    ? value
    : "shell";
}

function executableForPreset(preset: TerminalPreset): string | undefined {
  switch (preset) {
    case "codex":
      return "codex";
    case "claude":
      return "claude";
    case "gemini":
      return "gemini";
    case "opencode":
      return "opencode";
    case "shell":
    case "custom":
      return undefined;
  }
}

function titleForPreset(preset: TerminalPreset): string {
  switch (preset) {
    case "codex":
      return "Codex";
    case "claude":
      return "Claude";
    case "gemini":
      return "Gemini";
    case "opencode":
      return "OpenCode";
    case "shell":
      return "New terminal";
    case "custom":
      return "Custom terminal";
  }
}

function normalizeOptionalText(
  value: unknown,
  maximumLength: number,
): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const normalized = value.trim().slice(0, maximumLength);
  return normalized || undefined;
}

function parseConnection(
  value: unknown,
  nodeIds: ReadonlySet<string>,
): CanvasConnection | null {
  if (
    !isRecord(value) ||
    typeof value.id !== "string" ||
    typeof value.sourceNodeId !== "string" ||
    typeof value.targetNodeId !== "string" ||
    value.sourceNodeId === value.targetNodeId ||
    !nodeIds.has(value.sourceNodeId) ||
    !nodeIds.has(value.targetNodeId)
  ) {
    return null;
  }
  return {
    id: value.id,
    sourceNodeId: value.sourceNodeId,
    targetNodeId: value.targetNodeId,
  };
}

function normalizeNode(node: CanvasNode): CanvasNode {
  const normalized = parseNode(node);
  return normalized ?? node;
}

function normalizeTitle(value: unknown, fallback: string): string {
  if (typeof value !== "string") {
    return fallback;
  }
  const normalized = value.trim().slice(0, MAX_TITLE_LENGTH);
  return normalized || fallback;
}

function normalizeNumber(
  value: unknown,
  fallback: number,
  minimum: number,
  maximum: number,
): number {
  return typeof value === "number" && Number.isFinite(value)
    ? clamp(value, minimum, maximum)
    : fallback;
}

function normalizeStringArray(value: unknown): readonly string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return [...new Set(value.filter((item): item is string => typeof item === "string"))];
}

function stringArraysEqual(
  left: readonly string[],
  right: readonly string[],
): boolean {
  return (
    left.length === right.length &&
    left.every((value, index) => value === right[index])
  );
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function createId(kind: CanvasNodeKind): string {
  const suffix = globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`;
  return `${kind}-${suffix}`;
}
