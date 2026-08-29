export const CANVAS_STORAGE_KEY = "cli-master.canvas.v1";
export const CANVAS_DOCUMENT_VERSION = 1;
export const CANVAS_DOCUMENT_UPDATED_EVENT = "cli-master:canvas-document-updated";

export type CanvasNodeKind = "terminal" | "note";
export type TerminalPreset = "shell" | "codex" | "claude" | "opencode" | "custom";

export const DEFAULT_TERMINAL_SIZE = { width: 432, height: 256 } as const;
export const NOTE_SIZE = { width: 288, height: 288 } as const;

export interface CanvasPoint {
  readonly x: number;
  readonly y: number;
}

interface CanvasNodeBase extends CanvasPoint {
  readonly id: string;
  readonly title: string;
  readonly kind: CanvasNodeKind;
}

export interface TerminalCanvasNode extends CanvasNodeBase {
  readonly kind: "terminal";
  readonly sessionId?: string;
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

export type CanvasNode = TerminalCanvasNode | NoteCanvasNode;

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
  readonly version: 1;
  readonly nodes: readonly CanvasNode[];
  readonly connections: readonly CanvasConnection[];
  readonly zoom: number;
}

export interface CanvasState extends CanvasDocument {
  readonly selectedNodeId: string | null;
  readonly connectionSourceId: string | null;
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
      readonly type: "terminal/configure";
      readonly nodeId: string;
      readonly configuration: CanvasTerminalConfiguration;
    }
  | {
      readonly type: "terminal/resize";
      readonly nodeId: string;
      readonly size: { readonly width: number; readonly height: number };
    }
  | { readonly type: "node/delete"; readonly nodeId: string }
  | { readonly type: "node/select"; readonly nodeId: string | null }
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
const MAX_EXECUTABLE_LENGTH = 256;
const MAX_WORKING_DIRECTORY_LENGTH = 1_024;

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
  };
}

export function createInitialCanvasState(
  document: CanvasDocument = createInitialCanvasDocument(),
): CanvasState {
  return {
    ...document,
    selectedNodeId: null,
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
      };
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
    case "terminal/configure":
      return updateNode(state, action.nodeId, (node) =>
        node.kind === "terminal"
          ? configureTerminalNode(node, action.configuration)
          : node,
      );
    case "terminal/resize":
      return updateNode(state, action.nodeId, (node) =>
        node.kind === "terminal"
          ? {
              ...node,
              width: normalizeNumber(
                action.size.width,
                node.width,
                MIN_TERMINAL_WIDTH,
                MAX_TERMINAL_WIDTH,
              ),
              height: normalizeNumber(
                action.size.height,
                node.height,
                MIN_TERMINAL_HEIGHT,
                MAX_TERMINAL_HEIGHT,
              ),
            }
          : node,
      );
    case "node/delete":
      return {
        ...state,
        nodes: state.nodes.filter((node) => node.id !== action.nodeId),
        connections: state.connections.filter(
          (connection) =>
            connection.sourceNodeId !== action.nodeId &&
            connection.targetNodeId !== action.nodeId,
        ),
        selectedNodeId:
          state.selectedNodeId === action.nodeId
            ? null
            : state.selectedNodeId,
        connectionSourceId:
          state.connectionSourceId === action.nodeId
            ? null
            : state.connectionSourceId,
      };
    case "node/select":
      return nodeExists(state.nodes, action.nodeId)
        ? { ...state, selectedNodeId: action.nodeId }
        : state;
    case "connection/start":
      return nodeExists(state.nodes, action.nodeId)
        ? {
            ...state,
            selectedNodeId: action.nodeId,
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

export function toCanvasDocument(state: CanvasState): CanvasDocument {
  return {
    version: CANVAS_DOCUMENT_VERSION,
    nodes: state.nodes,
    connections: state.connections,
    zoom: state.zoom,
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
  return createTerminalCanvasNode(normalizedPosition, {}, id);
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

export function getCanvasNodeSize(
  node: CanvasNode,
): { readonly width: number; readonly height: number } {
  return node.kind === "terminal"
    ? { width: node.width, height: node.height }
    : NOTE_SIZE;
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

function updateNode(
  state: CanvasState,
  nodeId: string,
  update: (node: CanvasNode) => CanvasNode,
): CanvasState {
  if (!state.nodes.some((node) => node.id === nodeId)) {
    return state;
  }
  return {
    ...state,
    nodes: state.nodes.map((node) =>
      node.id === nodeId ? update(node) : node,
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
  if (!isRecord(value) || value.version !== CANVAS_DOCUMENT_VERSION) {
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
  };
}

function parseNode(value: unknown): CanvasNode | null {
  if (
    !isRecord(value) ||
    typeof value.id !== "string" ||
    (value.kind !== "terminal" && value.kind !== "note")
  ) {
    return null;
  }
  const base = {
    id: value.id,
    kind: value.kind,
    title: normalizeTitle(value.title, value.kind === "note" ? "Notes" : "Terminal"),
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

function normalizeTerminalPreset(value: unknown): TerminalPreset {
  return value === "codex" ||
    value === "claude" ||
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
