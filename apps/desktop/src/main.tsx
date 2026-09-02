import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { NativeBrowserSmoke } from "./app/features/browser/NativeBrowserSmoke";

const nativeBrowserSmokeUrl = import.meta.env.DEV
  ? import.meta.env.VITE_NATIVE_BROWSER_SMOKE_URL
  : undefined;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {nativeBrowserSmokeUrl ? (
      <NativeBrowserSmoke url={nativeBrowserSmokeUrl} />
    ) : (
      <App />
    )}
  </React.StrictMode>,
);
