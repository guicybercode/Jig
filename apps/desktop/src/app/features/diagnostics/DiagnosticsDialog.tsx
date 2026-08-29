import { useEffect, useState } from "react";

import { Dialog } from "../../components/Dialog";
import { Icon } from "../../components/Icon";
import type { DiagnosticsLoader, DiagnosticsResponse } from "./types";

interface DiagnosticsDialogProps {
  readonly open: boolean;
  readonly onClose: () => void;
  readonly load: DiagnosticsLoader;
}

/** Shows daemon-generated diagnostics and copies only its sanitized export. */
export function DiagnosticsDialog({
  open,
  onClose,
  load,
}: DiagnosticsDialogProps) {
  const [report, setReport] = useState<DiagnosticsResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">(
    "idle",
  );

  useEffect(() => {
    if (!open) {
      return undefined;
    }
    let cancelled = false;
    setReport(null);
    setError(null);
    setCopyState("idle");
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
  }, [load, open]);

  async function copyReport() {
    if (!report?.exportText) {
      return;
    }
    try {
      await navigator.clipboard.writeText(report.exportText);
      setCopyState("copied");
    } catch {
      setCopyState("failed");
    }
  }

  return (
    <Dialog title="Diagnostics" open={open} onClose={onClose}>
      <p className="dialog__lede">
        This snapshot omits environment values, command arguments, prompts, and
        terminal output. Home-directory prefixes are replaced before the daemon
        creates the clipboard export.
      </p>
      {error ? (
        <p className="dialog__error" role="alert">
          {error}
        </p>
      ) : null}
      {report ? <DiagnosticsSummary report={report} /> : null}
      {!report && !error ? (
        <p className="dialog__loading">Collecting sanitized diagnostics…</p>
      ) : null}
      <div className="dialog__actions">
        <button
          className="button button--primary"
          type="button"
          onClick={() => {
            void copyReport();
          }}
          disabled={!report?.exportText}
        >
          <Icon name="copy" />
          <span>Copy sanitized diagnostics</span>
        </button>
        <button
          className="button button--secondary"
          type="button"
          onClick={onClose}
        >
          Close
        </button>
      </div>
      <p className="dialog__copy-status" role="status" aria-live="polite">
        {copyState === "copied"
          ? "Copied. Sensitive runtime values were not included."
          : copyState === "failed"
            ? "Diagnostics copy failed. The raw on-screen response was not copied."
            : null}
      </p>
    </Dialog>
  );
}

function DiagnosticsSummary({
  report,
}: {
  readonly report: DiagnosticsResponse;
}) {
  return (
    <dl className="diagnostics-grid">
      <DiagnosticItem label="Daemon version" value={report.daemonVersion} />
      <DiagnosticItem
        label="Protocol / schema"
        value={`${report.protocolVersion} / ${report.schemaVersion}`}
      />
      <DiagnosticItem
        label="Daemon instance"
        value={report.daemonInstanceId}
      />
      <DiagnosticItem label="Data directory" value={report.dataPath} />
      <DiagnosticItem label="Runtime directory" value={report.runtimePath} />
      <DiagnosticItem label="Log directory" value={report.logPath} />
      <DiagnosticItem
        label="Executable search"
        value={report.effectivePath.join(" · ") || "not reported"}
      />
      <DiagnosticItem
        label="Recent issues"
        value={
          report.recentIssues
            .slice(-3)
            .map((issue) => `${issue.code}: ${issue.message}`)
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
