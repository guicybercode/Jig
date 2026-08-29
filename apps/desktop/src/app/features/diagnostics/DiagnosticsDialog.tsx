import { useEffect, useId, useState } from "react";

import { Icon } from "../../components/Icon";
import { exportNativeDiagnostics } from "./loadDiagnostics";
import type { DiagnosticsLoader, DiagnosticsReport } from "./types";

interface DiagnosticsDialogProps {
  readonly onClose: () => void;
  readonly load: DiagnosticsLoader;
  readonly exportReport?: () => Promise<string>;
}

/** Shows a sanitized diagnostics snapshot that is safe to share. */
export function DiagnosticsDialog({
  onClose,
  load,
  exportReport = exportNativeDiagnostics,
}: DiagnosticsDialogProps) {
  const titleId = useId();
  const [report, setReport] = useState<DiagnosticsReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">(
    "idle",
  );

  useEffect(() => {
    let cancelled = false;
    void load()
      .then((next) => {
        if (!cancelled) {
          setReport(next);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setError("Diagnostics could not be loaded safely.");
        }
      });
    return () => {
      cancelled = true;
    };
  }, [load]);

  async function copyReport() {
    if (!report) {
      return;
    }
    try {
      const text = await exportReport();
      await navigator.clipboard.writeText(text);
      setCopyState("copied");
    } catch {
      setCopyState("failed");
    }
  }

  return (
    <div className="dialog-backdrop">
      <div
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <header className="dialog__header">
          <div>
            <p className="dialog__eyebrow">Support</p>
            <h2 id={titleId}>Diagnostics</h2>
          </div>
          <button
            className="button button--secondary"
            type="button"
            onClick={onClose}
          >
            Close
          </button>
        </header>
        <p className="dialog__lede">
          The Copy action exports a sanitized snapshot without tokens, cookies,
          environment variables, prompts, terminal output, or your home path.
        </p>
        {error ? (
          <p className="dialog__error" role="alert">
            {error} Open the packaged desktop app if you need local paths and
            Git status.
          </p>
        ) : null}
        {report ? <DiagnosticsSummary report={report} /> : null}
        {!report && !error ? (
          <p className="dialog__loading">Collecting sanitized diagnostics…</p>
        ) : null}
        <footer className="dialog__footer">
          <button
            className="button button--primary"
            type="button"
            onClick={() => {
              void copyReport();
            }}
            disabled={!report}
          >
            <Icon name="copy" />
            <span>Copy sanitized diagnostics</span>
          </button>
          <span className="dialog__copy-status" role="status">
            {copyState === "copied"
              ? "Copied. Environment variables were not included."
              : copyState === "failed"
                ? "Diagnostics export failed. Retry; do not copy the on-screen paths."
                : null}
          </span>
        </footer>
      </div>
    </div>
  );
}

function DiagnosticsSummary({ report }: { readonly report: DiagnosticsReport }) {
  return (
    <dl className="diagnostics-grid">
      <DiagnosticItem label="App version" value={report.appVersion} />
      <DiagnosticItem label="OS" value={`${report.os}/${report.arch}`} />
      <DiagnosticItem label="Data directory" value={report.dataDir} />
      <DiagnosticItem label="Config directory" value={report.configDir} />
      <DiagnosticItem label="Database" value={report.databasePath} />
      <DiagnosticItem
        label="Git"
        value={
          report.gitAvailable
            ? (report.gitVersion ?? "available")
            : "not found"
        }
      />
      <DiagnosticItem label="Daemon" value={report.daemon.status} />
      <DiagnosticItem label="SQLite" value={report.sqlite.status} />
      <DiagnosticItem
        label="Sessions / worktrees"
        value={`${report.sessionCount} / ${report.worktreeCount}`}
      />
      <DiagnosticItem
        label="Detected agents"
        value={
          report.agents
            .filter((agent) => agent.detected)
            .map((agent) => agent.key)
            .join(", ") || "none"
        }
      />
      <DiagnosticItem
        label="Resolved executables"
        value={
          report.executables
            .map((item) =>
              item.path ? `${item.name}: ${item.path}` : `${item.name}: missing`,
            )
            .join(" · ") || "none"
        }
      />
      <DiagnosticItem
        label="Recent errors"
        value={
          report.recentErrors
            .slice(-3)
            .map((item) => `${item.code}: ${item.message}`)
            .join(" · ") || "none"
        }
      />
    </dl>
  );
}

function DiagnosticItem({
  label,
  value,
}: {
  readonly label: string;
  readonly value: string;
}) {
  return (
    <div className="diagnostics-item">
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}
