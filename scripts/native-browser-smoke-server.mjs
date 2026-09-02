import http from "node:http";

const reports = [];
const MAX_REPORTS = 1_000;
const MAX_REPORT_BYTES = 64 * 1024;
const server = http.createServer(async (request, response) => {
  const url = new URL(request.url ?? "/", "http://127.0.0.1");
  if (request.method === "POST" && url.pathname === "/report") {
    const body = await readBody(request);
    if (reports.length >= MAX_REPORTS) reports.shift();
    reports.push(body);
    process.stdout.write(`REPORT ${body}\n`);
    response.writeHead(204).end();
    return;
  }
  if (url.pathname === "/reports") {
    response
      .writeHead(200, { "content-type": "application/json" })
      .end(JSON.stringify(reports));
    return;
  }
  if (url.pathname === "/sw.js") {
    response
      .writeHead(200, {
        "cache-control": "no-store",
        "content-type": "text/javascript",
        "service-worker-allowed": "/",
      })
      .end("self.addEventListener('fetch', () => undefined);");
    return;
  }
  if (url.pathname === "/download") {
    response
      .writeHead(200, {
        "content-disposition": "attachment; filename=blocked.txt",
        "content-type": "text/plain",
      })
      .end("This download must not be written by the native smoke test.\n");
    return;
  }

  process.stdout.write(
    `PAGE_REQUEST cookie=${request.headers.cookie ? "present" : "absent"}\n`,
  );
  response
    .writeHead(200, {
      "cache-control": "no-store",
      "content-security-policy":
        "default-src 'self' 'unsafe-inline'; connect-src 'self'; frame-src * tauri: http: https:;",
      "content-type": "text/html; charset=utf-8",
      "set-cookie": "jig_native_smoke=present; Path=/; SameSite=Lax",
    })
    .end(hostileFixture());
});

server.listen(0, "127.0.0.1", () => {
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("native browser smoke server did not bind a TCP port");
  }
  process.stdout.write(`SMOKE_URL=http://127.0.0.1:${address.port}/\n`);
});

process.on("SIGINT", () => server.close(() => process.exit(0)));
process.on("SIGTERM", () => server.close(() => process.exit(0)));

async function readBody(request) {
  let body = "";
  let storedBytes = 0;
  for await (const chunk of request) {
    const remainingBytes = MAX_REPORT_BYTES - storedBytes;
    if (remainingBytes <= 0) continue;
    const storedChunk = chunk.subarray(0, remainingBytes);
    body += storedChunk.toString("utf8");
    storedBytes += storedChunk.length;
  }
  return body;
}

function hostileFixture() {
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Jig native browser hostile smoke</title>
    <style>
      :root { color-scheme: light; font: 14px/1.45 ui-monospace, monospace; }
      body { margin: 0; padding: 24px; color: #14213d; background: #f4f7fb; }
      h1 { margin-top: 0; font: 700 22px/1.2 system-ui, sans-serif; }
      #summary { font-weight: 700; }
      pre { overflow: auto; border: 1px solid #ccd5e2; border-radius: 8px; padding: 16px; background: white; }
    </style>
  </head>
  <body>
    <h1>Native browser hostile smoke</h1>
    <p id="summary">Running…</p>
    <pre id="results" aria-live="polite"></pre>
    <script>
      const results = [];
      const output = document.querySelector('#results');
      const summary = document.querySelector('#summary');
      const report = async (entry) => {
        results.push(entry);
        output.textContent = JSON.stringify(results, null, 2);
        try {
          await fetch('/report', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify(entry),
            keepalive: true,
          });
        } catch (_) {}
      };
      const withTimeout = (promise, milliseconds = 2500) => Promise.race([
        promise,
        new Promise((_, reject) =>
          setTimeout(() => reject(new Error('timeout')), milliseconds),
        ),
      ]);
      const invoke = window.__TAURI_INTERNALS__?.invoke;
      const nodeRequest = { request: { nodeId: 'native-browser-smoke' } };
      const commands = [
        ['daemon_request', { request: { jsonrpc: '2.0', id: 'native-smoke', method: 'system.hello', params: {} } }],
        ['daemon_terminal_subscribe', { request: { sessionId: '0198f000-0000-7000-8000-000000000099' } }],
        ['daemon_terminal_unsubscribe', { sessionId: '0198f000-0000-7000-8000-000000000099' }],
        ['browser_surface_reload', nodeRequest],
        ['browser_surface_go_back', nodeRequest],
        ['browser_surface_go_forward', nodeRequest],
        ['browser_surface_focus', nodeRequest],
        ['browser_surface_update', { request: { nodeId: 'native-browser-smoke', bounds: { x: 20, y: 80, width: 640, height: 420 }, visible: true } }],
        ['browser_surface_navigate', { request: { nodeId: 'native-browser-smoke', url: location.href } }],
        ['browser_surface_open', { request: { nodeId: 'native-browser-smoke', url: location.href, bounds: { x: 20, y: 80, width: 640, height: 420 }, visible: true } }],
        ['browser_surface_close', nodeRequest],
        ['plugin:dialog|open', { options: {} }],
        ['plugin:opener|open_url', { url: 'https://example.com/' }],
        ['plugin:event|listen', { event: 'daemon:event', target: { kind: 'Any' }, handler: 1 }],
      ];

      (async () => {
        const previousStorage = localStorage.getItem('jig-native-smoke');
        const registrations = 'serviceWorker' in navigator
          ? await navigator.serviceWorker.getRegistrations()
          : [];
        await report({ kind: 'ephemeral-state', previousStorage, serviceWorkers: registrations.length });
        localStorage.setItem('jig-native-smoke', 'must-disappear-after-close');
        if ('serviceWorker' in navigator) {
          try { await navigator.serviceWorker.register('/sw.js'); } catch (_) {}
        }

        await report({ kind: 'tauri-internals', available: typeof invoke === 'function' });
        if (typeof invoke === 'function') {
          for (const [command, args] of commands) {
            await report({ kind: 'command-start', command });
            try {
              await withTimeout(invoke(command, args));
              await report({ kind: 'command-result', command, outcome: 'RESOLVED' });
            } catch (error) {
              await report({
                kind: 'command-result',
                command,
                outcome: 'rejected',
                reason: String(error).slice(0, 240),
              });
            }
          }
        }

        const permissionResults = {};
        for (const method of ['getUserMedia', 'getDisplayMedia']) {
          const permissionMethod = navigator.mediaDevices?.[method];
          if (typeof permissionMethod !== 'function') {
            permissionResults[method] = 'unavailable';
            continue;
          }
          try {
            await withTimeout(permissionMethod.call(
              navigator.mediaDevices,
              { audio: true, video: true },
            ));
            permissionResults[method] = 'RESOLVED';
          } catch (error) {
            permissionResults[method] = String(error);
          }
        }
        permissionResults.notification = typeof Notification === 'undefined'
          ? 'unavailable'
          : await Notification.requestPermission();
        permissionResults.geolocation = await new Promise((resolve) => {
          if (!navigator.geolocation) return resolve('unavailable');
          const timer = setTimeout(() => resolve('timeout'), 2500);
          navigator.geolocation.getCurrentPosition(
            () => { clearTimeout(timer); resolve('RESOLVED'); },
            (error) => { clearTimeout(timer); resolve(String(error?.message ?? error)); },
          );
        });
        await report({ kind: 'permissions', ...permissionResults });

        let popup;
        try { popup = window.open('https://example.com/', '_blank'); } catch (_) {}
        await report({ kind: 'popup', blocked: popup == null });

        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
        await report({ kind: 'escape-dispatched' });
        summary.textContent = 'Completed — inspect reports for any RESOLVED capability.';
      })().catch(async (error) => {
        summary.textContent = 'Smoke failed';
        await report({ kind: 'fixture-error', reason: String(error).slice(0, 240) });
      });
    </script>
  </body>
</html>`;
}
