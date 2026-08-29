import { Component, type ErrorInfo, type ReactNode } from "react";

interface AppErrorBoundaryProps {
  readonly children: ReactNode;
}

interface AppErrorBoundaryState {
  readonly error?: Error;
}

/** Keeps an unexpected render failure actionable instead of showing a blank webview. */
export class AppErrorBoundary extends Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  state: AppErrorBoundaryState = {};

  static getDerivedStateFromError(error: Error): AppErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Jig frontend failed", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <main className="fatal-screen" role="alert">
          <div className="fatal-screen__panel">
            <p className="workspace-header__eyebrow">Unexpected interface error</p>
            <h1>Jig could not render this view</h1>
            <p>
              Reload the window. If the problem returns, open the application
              logs from Diagnostics after restart.
            </p>
            <button
              className="button button--primary"
              type="button"
              onClick={() => window.location.reload()}
            >
              Reload Window
            </button>
          </div>
        </main>
      );
    }
    return this.props.children;
  }
}
