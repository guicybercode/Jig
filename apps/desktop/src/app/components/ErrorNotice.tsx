import type { ApiErrorData } from "../../ipc/types";
import { Icon } from "./Icon";

/** Props for an actionable application error notice. */
export interface ErrorNoticeProps {
  /** Supplies the official IPC error fields shown to the user. */
  readonly error: ApiErrorData;
  /** Overrides the default error heading. */
  readonly title?: string;
  /** Offers a recovery action when the failed operation can be repeated. */
  readonly onRetry?: () => void;
  /** Labels the optional recovery action. */
  readonly retryLabel?: string;
  /** Offers a non-destructive way to dismiss the notice. */
  readonly onDismiss?: () => void;
}

/** Renders an assertive, actionable error without relying on color alone. */
export function ErrorNotice({
  error,
  title = "Something went wrong",
  onRetry,
  retryLabel = "Retry",
  onDismiss,
}: ErrorNoticeProps) {
  return (
    <section className="error-notice" role="alert" aria-atomic="true">
      <span className="error-notice__icon" aria-hidden="true">
        <Icon name="warning" />
      </span>
      <div className="error-notice__content">
        <h2>{title}</h2>
        <p>{error.message}</p>
        {error.action ? (
          <p className="error-notice__action">{error.action}</p>
        ) : null}
        <code className="error-notice__code">Error: {error.code}</code>
      </div>
      {onRetry || onDismiss ? (
        <div className="error-notice__controls">
          {onRetry ? (
            <button
              className="button button--secondary"
              type="button"
              onClick={onRetry}
            >
              <Icon name="refresh" />
              <span>{retryLabel}</span>
            </button>
          ) : null}
          {onDismiss ? (
            <button
              className="button"
              type="button"
              onClick={onDismiss}
            >
              <Icon name="close" />
              <span>Dismiss</span>
            </button>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
