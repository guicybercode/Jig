import type { AgentDiagnosticsReport, AgentRecord, LaunchTestStatus } from "./agentTypes";

const REDACTED = "<redacted>";

/** Defense-in-depth for copyable text; the Rust diagnostics boundary also redacts. */
function sanitizeDiagnosticText(value: string): string {
  return value
    .replace(
      /-----BEGIN [^-\r\n]*PRIVATE KEY-----[\s\S]*?(?:-----END [^-\r\n]*PRIVATE KEY-----|$)/gi,
      "<redacted private key>",
    )
    .replace(
      /\b(?:authorization|proxy-authorization|cookie|set-cookie)\s*:\s*[^\r\n]*/gi,
      (header) => `${header.slice(0, header.indexOf(":"))}: ${REDACTED}`,
    )
    .replace(/\b(?:bearer|basic|digest|negotiate)\s+\S+/gi, (match) => {
      const scheme = match.slice(0, match.indexOf(" "));
      return `${scheme} ${REDACTED}`;
    })
    .replace(
      /((?:^|\b|--)(?:token|secret|password|passphrase|cookie|credential|auth|authorization|api[_-]?key|access[_-]?key|private[_-]?key|signing[_-]?key)\s*[:=]\s*)(?:"[^"]*"|'[^']*'|[^\s]+)/gim,
      `$1${REDACTED}`,
    )
    .replace(
      /\b(?:sk-[A-Za-z0-9._-]+|sk_(?:live|test)_[A-Za-z0-9._-]+|rk_live_[A-Za-z0-9._-]+|gh[pousr]_[A-Za-z0-9._-]+|github_pat_[A-Za-z0-9._-]+|xox[baprs]-[A-Za-z0-9._-]+|glpat-[A-Za-z0-9._-]+|npm_[A-Za-z0-9._-]+|pypi-[A-Za-z0-9._-]+)\b/gi,
      REDACTED,
    )
    .replace(/\b(?:AKIA|ASIA)[A-Z0-9]{12,}\b/g, REDACTED)
    .replace(/\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b/g, REDACTED)
    .replace(/\/(?:Users|home)\/[^/\s]+/g, "~")
    .replace(/[A-Za-z]:\\Users\\[^\\\s]+/gi, "~");
}

function formatLaunchTest(status: LaunchTestStatus): string {
  switch (status.status) {
    case "success":
      return "success";
    case "not_found":
      return "not_found";
    case "not_executable":
      return `not_executable candidate=${status.candidate}`;
    case "timeout":
      return "timeout";
    case "failed":
      return `failed ${sanitizeDiagnosticText(status.message)}`;
    default: {
      const exhaustive: never = status;
      return exhaustive;
    }
  }
}

/** Builds a copyable diagnostics report without environment values or tokens. */
export function formatAgentDiagnostics(
  agent: AgentRecord,
  diagnostics?: AgentDiagnosticsReport,
): string {
  const installed = diagnostics?.installed ?? agent.installed;
  const path = sanitizeDiagnosticText(
    diagnostics?.path ?? agent.resolvedPath ?? "unresolved",
  );
  const version = sanitizeDiagnosticText(
    diagnostics?.version ?? agent.version ?? "unknown",
  );
  const warning = diagnostics?.warning ?? agent.warning;
  const launchTest = diagnostics
    ? formatLaunchTest(diagnostics.launchTest)
    : installed
      ? "not_run"
      : "not_found";
  const searched =
    diagnostics?.searchedPaths.length
      ? diagnostics.searchedPaths.map(sanitizeDiagnosticText).join("\n  ")
      : "(none recorded)";

  const lines = [
    `Agent: ${sanitizeDiagnosticText(agent.displayName)}`,
    `id: ${agent.id}`,
    `source: ${agent.source}`,
    `enabled: ${agent.enabled}`,
    `installed: ${installed}`,
    `executable: ${sanitizeDiagnosticText(agent.executable)}`,
    `path: ${path}`,
    `version: ${version}`,
    `requiresPty: ${agent.requiresPty}`,
    `argsCount: ${agent.defaultArgs.length}`,
    `envKeys: ${agent.envKeys.length > 0 ? agent.envKeys.join(", ") : "(none)"}`,
    `launchTest: ${launchTest}`,
    "searchedPaths:",
    `  ${searched}`,
  ];

  if (warning) {
    lines.push(`warning: ${sanitizeDiagnosticText(warning)}`);
  }

  return lines.map(sanitizeDiagnosticText).join("\n");
}
