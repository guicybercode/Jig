export interface DaemonDiagnostics {
  connected: boolean;
  instanceId?: string;
  status: string;
}

export interface SqliteDiagnostics {
  fileExists: boolean;
  available: boolean;
  schemaVersion?: number;
  status: string;
}

export interface AgentDiagnostics {
  key: string;
  displayName: string;
  detected: boolean;
  executable?: string;
}

export interface ExecutableDiagnostics {
  name: string;
  path?: string;
}

export interface LogRecordDto {
  timestamp: string;
  level: string;
  target: string;
  operation: string;
  sessionId?: string;
  projectId?: string;
  errorCode?: string;
  message: string;
}

export interface ApiError {
  code: string;
  message: string;
  action?: string;
  title?: string;
  recoverable?: boolean;
  details?: Record<string, unknown>;
}

export interface DiagnosticsReport {
  appVersion: string;
  os: string;
  arch: string;
  dataDir: string;
  configDir: string;
  runtimeDir: string;
  databasePath: string;
  logDir: string;
  gitVersion?: string;
  gitAvailable: boolean;
  daemon: DaemonDiagnostics;
  sqlite: SqliteDiagnostics;
  agents: AgentDiagnostics[];
  executables: ExecutableDiagnostics[];
  sessionCount: number;
  worktreeCount: number;
  recentLogs: LogRecordDto[];
  recentErrors: ApiError[];
}

export type DiagnosticsLoader = () => Promise<DiagnosticsReport>;
