import type { SessionStatus } from "../../ipc/types";

interface StatusPresentation {
  readonly label: string;
}

const STATUS_PRESENTATION = {
  starting: { label: "Starting" },
  running: { label: "Running" },
  idle: { label: "Idle" },
  exited: { label: "Exited" },
  failed: { label: "Failed" },
  unknown: { label: "Unknown" },
} satisfies Readonly<Record<SessionStatus, StatusPresentation>>;

/** Props for a visible session-state badge. */
export interface StatusBadgeProps {
  /** Selects one of the official daemon session states. */
  readonly status: SessionStatus;
  /** Applies the dense list-row treatment without hiding the status text. */
  readonly compact?: boolean;
}

/** Renders every session status with visible text and a decorative indicator. */
export function StatusBadge({ status, compact = false }: StatusBadgeProps) {
  const label = getSessionStatusLabel(status);
  const className = [
    "status-badge",
    `status-badge--${status}`,
    compact ? "status-badge--compact" : undefined,
  ]
    .filter((value): value is string => Boolean(value))
    .join(" ");

  return (
    <span
      className={className}
      data-status={status}
      aria-label={`Session status: ${label}`}
    >
      <span className="status-badge__indicator" aria-hidden="true" />
      <span className="status-badge__label">{label}</span>
    </span>
  );
}

/** Converts the official session state to its user-facing label. */
export function getSessionStatusLabel(status: SessionStatus): string {
  return STATUS_PRESENTATION[status].label;
}
