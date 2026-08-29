import { useId, useState } from "react";

import { Dialog } from "../../components/Dialog";
import { Icon } from "../../components/Icon";
import { formatAgentDiagnostics } from "../../../ipc/formatDiagnostics";
import type { AgentDiagnosticsReport, AgentRecord } from "../../../ipc/agentTypes";

interface DiagnosticsDialogProps {
  readonly agent: AgentRecord;
  readonly diagnostics?: AgentDiagnosticsReport;
  readonly onClose: () => void;
}

/** Shows copyable diagnostics without environment values or tokens. */
export function DiagnosticsDialog({
  agent,
  diagnostics,
  onClose,
}: DiagnosticsDialogProps) {
  const textId = useId();
  const liveId = useId();
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");
  const text = formatAgentDiagnostics(agent, diagnostics);

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopyState("copied");
    } catch {
      setCopyState("failed");
    }
  }

  return (
    <Dialog title={`${agent.displayName} diagnostics`} open onClose={onClose}>
      <pre id={textId} className="diagnostics">
        {text}
      </pre>
      <p id={liveId} className="form__hint" role="status" aria-live="polite">
        {copyState === "copied"
          ? "Diagnostics copied. Secrets are not included."
          : copyState === "failed"
            ? "Diagnostics copy failed. No unsanitized fallback was copied."
            : "Environment values and argument contents are omitted."}
      </p>
      <div className="dialog__actions">
        <button className="button button--secondary" type="button" onClick={onClose}>
          Close
        </button>
        <button className="button button--primary" type="button" onClick={() => void copy()}>
          <Icon name="copy" />
          <span>Copy diagnostics</span>
        </button>
      </div>
    </Dialog>
  );
}
