import { useEffect, useState } from "react";

import type { ApiErrorData, DiagnosticsSnapshot, HelloResponse, StateSnapshot } from "../../../ipc/types";
import { Icon } from "../../components/Icon";
import { errorData } from "../../utils";

interface DiagnosticsViewProps {
  readonly hello?: HelloResponse;
  readonly snapshot?: StateSnapshot;
  readonly onLoad: () => Promise<DiagnosticsSnapshot>;
  readonly onRetryConnection: () => void;
}

/** Loads sanitized daemon diagnostics only while the diagnostics view is open. */
export function DiagnosticsView({
  hello,
  snapshot,
  onLoad,
  onRetryConnection,
}: DiagnosticsViewProps) {
  const [diagnostics, setDiagnostics] = useState<DiagnosticsSnapshot>();
  const [error, setError] = useState<ApiErrorData>();
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let current = true;
    setLoading(true);
    setError(undefined);
    void onLoad()
      .then((result) => {
        if (current) {
          setDiagnostics(result);
        }
      })
      .catch((failure: unknown) => {
        if (current) {
          setError(errorData(failure));
        }
      })
      .finally(() => {
        if (current) {
          setLoading(false);
        }
      });
    return () => {
      current = false;
    };
  }, [attempt, onLoad]);

  return (
    <main id="workspace" className="workspace workspace--document" tabIndex={-1}>
      <header className="workspace-header">
        <div className="workspace-header__identity">
          <p className="workspace-header__eyebrow">Local system</p>
          <h1>Diagnostics</h1>
        </div>
        <button className="button button--secondary" type="button" onClick={onRetryConnection}>
          <Icon name="refresh" />
          Reconnect
        </button>
      </header>
      <div className="document-view">
        <section className="document-section" aria-labelledby="connection-diagnostics-title">
          <h2 id="connection-diagnostics-title">Connection</h2>
          <dl className="metadata-grid">
            <div>
              <dt>Protocol</dt>
              <dd>{diagnostics?.protocolVersion ?? hello?.protocolVersion ?? "Unavailable"}</dd>
            </div>
            <div>
              <dt>Daemon</dt>
              <dd>{diagnostics?.daemonVersion ?? hello?.daemonVersion ?? "Unavailable"}</dd>
            </div>
            <div>
              <dt>Schema</dt>
              <dd>{diagnostics?.schemaVersion ?? snapshot?.schemaVersion ?? "Unavailable"}</dd>
            </div>
            <div className="metadata-grid__wide">
              <dt>Daemon instance</dt>
              <dd className="mono path-value">{diagnostics?.daemonInstanceId ?? hello?.instanceId ?? "Unavailable"}</dd>
            </div>
          </dl>
        </section>

        {loading ? <div className="loading-state" role="status">Loading diagnostics…</div> : null}
        {error ? (
          <div className="notice notice--error" role="alert">
            <Icon name="warning" />
            <div>
              <strong>{error.message}</strong>
              {error.action ? <p>{error.action}</p> : null}
            </div>
            <button className="button button--secondary" type="button" onClick={() => setAttempt((value) => value + 1)}>
              Retry
            </button>
          </div>
        ) : null}

        {diagnostics ? (
          <>
            <section className="document-section" aria-labelledby="paths-diagnostics-title">
              <h2 id="paths-diagnostics-title">Local paths</h2>
              <dl className="path-list">
                <div><dt>Data</dt><dd>{diagnostics.dataPath}</dd></div>
                <div><dt>Runtime</dt><dd>{diagnostics.runtimePath}</dd></div>
                <div><dt>Logs</dt><dd>{diagnostics.logPath}</dd></div>
              </dl>
              <p className="document-note">
                Diagnostic exports are sanitized and never include terminal output, tokens, or complete agent arguments.
              </p>
            </section>
            <section className="document-section" aria-labelledby="issues-diagnostics-title">
              <h2 id="issues-diagnostics-title">Recent issues</h2>
              {diagnostics.recentIssues.length ? (
                <ul className="diagnostic-issue-list">
                  {diagnostics.recentIssues.map((issue, index) => (
                    <li key={`${issue.code}:${index}`}>
                      <strong>{issue.message}</strong>
                      <code>{issue.code}</code>
                      {issue.action ? <p>{issue.action}</p> : null}
                    </li>
                  ))}
                </ul>
              ) : <p className="document-note">No recent daemon issues.</p>}
            </section>
          </>
        ) : null}
      </div>
    </main>
  );
}
