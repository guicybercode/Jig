import { useEffect, useState } from "react";

import type {
  ApiErrorData,
  DiagnosticsSnapshot,
  HelloResponse,
  StateSnapshot,
} from "../../../ipc/types";
import { Icon } from "../../components/Icon";
import { errorData } from "../../utils";

interface DiagnosticsViewProps {
  readonly hello?: HelloResponse;
  readonly snapshot?: StateSnapshot;
  readonly onOpenCanvas: () => void;
  readonly onLoad: () => Promise<DiagnosticsSnapshot>;
  readonly onRetryConnection: () => void;
}

/** Loads sanitized daemon diagnostics only while the diagnostics view is open. */
export function DiagnosticsView({
  hello,
  snapshot,
  onOpenCanvas,
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
        if (current) setDiagnostics(result);
      })
      .catch((failure: unknown) => {
        if (current) setError(errorData(failure));
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [attempt, onLoad]);

  return (
    <main
      id="workspace"
      className="canvas-settings canvas-diagnostics"
      tabIndex={-1}
    >
      <div className="canvas-settings__content">
        <div className="canvas-diagnostics__toolbar">
          <button
            className="canvas-settings__back"
            type="button"
            onClick={onOpenCanvas}
          >
            <Icon name="chevronRight" /> Back to canvas
          </button>
          <button
            className="canvas-diagnostics__reconnect"
            type="button"
            onClick={onRetryConnection}
          >
            <Icon name="refresh" /> Reconnect
          </button>
        </div>

        <header className="canvas-settings__header">
          <p>Local system</p>
          <h1>Diagnostics</h1>
          <span>
            Connection health and sanitized paths for this installation.
          </span>
        </header>

        <div className="canvas-settings__grid">
          <section
            className="canvas-settings__section canvas-settings__section--wide"
            aria-labelledby="connection-diagnostics-title"
          >
            <div className="canvas-settings__section-heading">
              <span aria-hidden="true">
                <Icon name="diagnostics" />
              </span>
              <div>
                <h2 id="connection-diagnostics-title">Connection</h2>
                <p>Desktop and local service compatibility.</p>
              </div>
            </div>
            <dl className="canvas-diagnostics__metrics">
              <div>
                <dt>Protocol</dt>
                <dd>
                  {diagnostics?.protocolVersion ??
                    hello?.protocolVersion ??
                    "Unavailable"}
                </dd>
              </div>
              <div>
                <dt>Service</dt>
                <dd>
                  {diagnostics?.daemonVersion ??
                    hello?.daemonVersion ??
                    "Unavailable"}
                </dd>
              </div>
              <div>
                <dt>Schema</dt>
                <dd>
                  {diagnostics?.schemaVersion ??
                    snapshot?.schemaVersion ??
                    "Unavailable"}
                </dd>
              </div>
              <div className="canvas-diagnostics__metric--wide">
                <dt>Service instance</dt>
                <dd className="mono">
                  {diagnostics?.daemonInstanceId ??
                    hello?.instanceId ??
                    "Unavailable"}
                </dd>
              </div>
            </dl>
            {loading ? (
              <div className="canvas-diagnostics__loading" role="status">
                <span className="spinner" aria-hidden="true" /> Loading
                diagnostics…
              </div>
            ) : null}
          </section>

          {error ? (
            <section
              className="canvas-diagnostics__error"
              role="alert"
              aria-labelledby="diagnostics-error-title"
            >
              <span aria-hidden="true">
                <Icon name="warning" />
              </span>
              <div>
                <h2 id="diagnostics-error-title">
                  Diagnostics are unavailable
                </h2>
                <p>{error.message}</p>
                {error.action ? <small>{error.action}</small> : null}
              </div>
              <button
                type="button"
                onClick={() => setAttempt((value) => value + 1)}
              >
                Try again
              </button>
            </section>
          ) : null}

          {diagnostics ? (
            <>
              <section
                className="canvas-settings__section canvas-settings__section--wide"
                aria-labelledby="paths-diagnostics-title"
              >
                <div className="canvas-settings__section-heading">
                  <span aria-hidden="true">
                    <Icon name="folder" />
                  </span>
                  <div>
                    <h2 id="paths-diagnostics-title">Local paths</h2>
                    <p>Private application directories on this computer.</p>
                  </div>
                </div>
                <dl className="canvas-diagnostics__paths">
                  <div>
                    <dt>Data</dt>
                    <dd>{diagnostics.dataPath}</dd>
                  </div>
                  <div>
                    <dt>Runtime</dt>
                    <dd>{diagnostics.runtimePath}</dd>
                  </div>
                  <div>
                    <dt>Logs</dt>
                    <dd>{diagnostics.logPath}</dd>
                  </div>
                </dl>
                <p className="canvas-settings__note">
                  Diagnostics never include terminal output, tokens, or
                  complete agent arguments.
                </p>
              </section>

              <section
                className="canvas-settings__section canvas-settings__section--wide"
                aria-labelledby="issues-diagnostics-title"
              >
                <div className="canvas-settings__section-heading">
                  <span aria-hidden="true">
                    <Icon
                      name={
                        diagnostics.recentIssues.length ? "warning" : "check"
                      }
                    />
                  </span>
                  <div>
                    <h2 id="issues-diagnostics-title">Recent issues</h2>
                    <p>Sanitized startup and lifecycle events.</p>
                  </div>
                </div>
                {diagnostics.recentIssues.length ? (
                  <ul className="canvas-diagnostics__issues">
                    {diagnostics.recentIssues.map((issue, index) => (
                      <li key={`${issue.code}:${index}`}>
                        <div>
                          <strong>{issue.message}</strong>
                          {issue.action ? <p>{issue.action}</p> : null}
                        </div>
                        <code>{issue.code}</code>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="canvas-diagnostics__empty">
                    No recent issues. Everything looks healthy.
                  </p>
                )}
              </section>
            </>
          ) : null}
        </div>
      </div>
    </main>
  );
}
