# Native browser security smoke

This development-only harness opens a hostile local page in the real Tauri
child webview. It exercises the native boundary without loading or changing a
saved canvas. It is evidence for development; it does not replace the packaged
macOS and Linux acceptance in [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md).

## Run it

From the repository root, start the loopback-only fixture server:

```sh
node scripts/native-browser-smoke-server.mjs
```

Copy the printed `SMOKE_URL`, then launch the Tauri app in another terminal:

```sh
VITE_NATIVE_BROWSER_SMOKE_URL=http://127.0.0.1:<port>/ \
  pnpm --filter @cli-master/desktop tauri dev --no-watch
```

Keep the fixture server running, close the app, and launch it a second time
with the same URL. Stop the server with `Ctrl-C` after collecting both runs.

## Pass criteria

- The first document request in each app run reports `cookie=absent`.
- Every `ephemeral-state` report has `previousStorage: null` and
  `serviceWorkers: 0`.
- `tauri-internals` reports `available: true`, so the command checks are
  exercising the real injected Tauri bridge.
- Every `command-result` reports `outcome: "rejected"`. This includes daemon,
  browser-host, dialog, opener, and event commands.
- Camera, microphone, display capture, geolocation, and notifications report
  unavailable, denied, or rejected; none may report `RESOLVED`.
- `popup` reports `blocked: true`.
- With keyboard focus inside the page, pressing a physical `Escape` returns
  focus to the trusted browser controls. The fixture's synthetic Escape report
  only exercises the route and is not a substitute for this manual check.
- The app and fixture output contain no credentials, cookies, or full OAuth
  callback URLs.

Treat a timeout, a fixture error, or any unexpected `RESOLVED` result as a
failure. Save the commit, OS version, WebKit version, and complete report with
the release evidence.

## Production exclusion

The host is guarded by `import.meta.env.DEV` and must be removed by the
production build. After building, this command must print no matches:

```sh
rg "Native browser security smoke|native-browser-smoke|VITE_NATIVE_BROWSER_SMOKE_URL" \
  apps/desktop/dist
```
