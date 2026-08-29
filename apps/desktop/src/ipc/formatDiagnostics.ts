import type { AgentDiagnosticsReport, AgentRecord, LaunchTestStatus } from "./agentTypes";

function redactSecrets(value: string): string {
  return value.replace(/\bsk-[A-Za-z0-9_-]+\b/g, "<redacted>");
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
      return `failed ${redactSecrets(status.message)}`;
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
  const path = diagnostics?.path ?? agent.resolvedPath ?? "unresolved";
  const version = diagnostics?.version ?? agent.version ?? "unknown";
  const warning = diagnostics?.warning ?? agent.warning;
  const launchTest = diagnostics
    ? formatLaunchTest(diagnostics.launchTest)
    : installed
      ? "not_run"
      : "not_found";
  const searched =
    diagnostics?.searchedPaths.length
      ? diagnostics.searchedPaths.join("\n  ")
      : "(none recorded)";

  const lines = [
    `Agent: ${agent.displayName}`,
    `id: ${agent.id}`,
    `source: ${agent.source}`,
    `enabled: ${agent.enabled}`,
    `installed: ${installed}`,
    `executable: ${agent.executable}`,
    `path: ${path}`,
    `version: ${version}`,
    `requiresPty: ${agent.requiresPty}`,
    `args: ${agent.defaultArgs.length > 0 ? agent.defaultArgs.join(" ") : "(none)"}`,
    `envKeys: ${agent.envKeys.length > 0 ? agent.envKeys.join(", ") : "(none)"}`,
    `launchTest: ${launchTest}`,
    "searchedPaths:",
    `  ${searched}`,
  ];

  if (warning) {
    lines.push(`warning: ${redactSecrets(warning)}`);
  }

  return lines.map((line) => redactSecrets(line)).join("\n");
}
