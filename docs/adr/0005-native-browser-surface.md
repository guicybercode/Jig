# ADR 0005: Native browser surface in the canvas

## Status

Accepted for the first canvas browser increment.

## Context

The spatial canvas needs a general-purpose browser beside terminal and note
cards. A browser card must load ordinary HTTPS sites, move and resize with the
canvas, and expose an explicit way to share its address with connected cards.

An HTML iframe cannot provide that experience reliably. Many sites prohibit
framing, and the same-origin policy prevents the application from observing
navigation safely. Proxying pages to bypass those controls would turn CLI
Master into an intermediary for remote traffic, which is outside the product
boundary.

## Decision

The Tauri process owns one ephemeral native child webview at a time through a
small `BrowserSurfaceManager`. Activating another browser card replaces the
current surface. The daemon, core wire protocol, and SQLite schema do not gain
browser operations.

React owns browser-card metadata: node ID, requested HTTP(S) URL, title,
position, size, and connections. Native handles, cookies, page content, and
history never enter React state or canvas persistence.

The main application webview sends validated lifecycle and geometry commands
to Tauri. Remote browser webviews receive no Tauri capability. Tauri validates
the caller, URL, node identifier, and bounds again, and blocks navigation to
local application origins and non-HTTP(S) schemes. The surface is hidden when
its visible DOM slot cannot be represented safely, including obscured and
non-100% zoom states.

Connections do not create autonomous browser control. A user gesture may copy
the sanitized current URL into a connected note or terminal. Terminal handoff
does not include Enter, so the user can review the text before submitting it.

## Alternatives considered

- A general iframe was rejected because framing policy and same-origin limits
  make it an unreliable browser and would expand the trusted webview's CSP.
- A separate system browser remains a fallback, but it does not satisfy the
  in-canvas requirement.
- Bundling Chromium or CEF was rejected for its packaging, memory, security,
  and maintenance cost.
- Browser ownership in the daemon was rejected because browser surfaces are a
  desktop presentation concern, not durable session or process coordination.

## Consequences

The implementation depends on Tauri's unstable child-webview API and the
platform WebKit engine (WKWebView on macOS and WebKitGTK on Linux). Packaged
smoke tests are required on both platforms; web-only Playwright tests are not
sufficient.

Native surfaces do not obey DOM transforms, clipping, or z-index. Their bounds
must be synchronized from the rendered slot, and lifecycle cleanup must close
or hide them during canvas transitions and overlays. Limiting the first
increment to one active surface bounds memory use and z-order complexity.

Future agent-driven browsing requires a separate structured, permissioned tool
protocol with auditability. A visual canvas connection and an opaque PTY are
not that protocol.
