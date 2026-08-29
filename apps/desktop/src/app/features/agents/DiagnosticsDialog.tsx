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
  const [copied, setCopied] = useState(false);
  const text = formatAgentDiagnostics(agent, diagnostics);

  async function copy() {
    await navigator.clipboard.writeText(text);
    setCopied(true);
  }

  return (
    <Dialog title={`${agent.displayName} diagnostics`} open onClose={onClose}>
      <pre id={textId} className="diagnostics">
        {text}
      </pre>
      <p id={liveId} className="form__hint" role="status" aria-live="polite">
        {copied ? "Diagnostics copied. Secrets are not included." : "Environment values are omitted."}
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
