import type { ApiError } from "../../../ipc/types";

interface ErrorBannerProps {
  error: ApiError | null;
}

/** Renders daemon-provided errors without constructing Git or process commands. */
export function ErrorBanner({ error }: ErrorBannerProps) {
  if (!error) {
    return null;
  }

  return (
    <div className="error-banner" role="alert">
      <strong>{error.code}</strong>
      <p>{error.message}</p>
      {error.action ? <p>{error.action}</p> : null}
    </div>
  );
}
