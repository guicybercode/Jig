import { useEffect, type CSSProperties } from "react";

import { BrowserSurface } from "./BrowserSurface";
import { defaultBrowserRuntime } from "./browser-runtime";

const SMOKE_NODE_ID = "native-browser-smoke";
const SMOKE_HOST_STYLE: CSSProperties = {
  position: "fixed",
  inset: 0,
  display: "flex",
  padding: "1rem",
  background: "#e8ebf0",
};

interface NativeBrowserSmokeProps {
  readonly url: string;
}

/** Development-only host used to exercise the real child-webview boundary. */
export function NativeBrowserSmoke({ url }: NativeBrowserSmokeProps) {
  useEffect(() => {
    void defaultBrowserRuntime.focus({ nodeId: SMOKE_NODE_ID }).catch(
      () => undefined,
    );
  }, []);

  return (
    <main style={SMOKE_HOST_STYLE} data-browser-viewport="true">
      <BrowserSurface
        nodeId={SMOKE_NODE_ID}
        url={url}
        accessibleLabel="Native browser security smoke"
        active
        runtime={defaultBrowserRuntime}
        onActivate={() => undefined}
        onNavigate={() => undefined}
        onOpenExternal={(address) =>
          defaultBrowserRuntime.openExternal(address)
        }
      />
    </main>
  );
}
