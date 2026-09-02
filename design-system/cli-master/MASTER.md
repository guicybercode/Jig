# CLI Master design system

This file is the visual source of truth for the desktop application. Page-level
overrides belong in `pages/<page-name>.md` and must document why they differ.

## Product direction

CLI Master is a dense, keyboard-first developer control center. It should feel
closer to a terminal multiplexer or code editor than a consumer chat product.
The application shell is light-first, restrained, fast, and readable during
long work sessions. The xterm viewport is the only dark surface; dialogs,
navigation, status, settings, diagnostics, and connection states all use the
light shell palette.

Design dials:

- Variance: 4/10 — balanced, with predictable placement.
- Motion: 2/10 — feedback only; no decorative choreography.
- Density: 8/10 — compact desktop controls with usable hit areas.

## Color tokens

Use semantic tokens in components. Do not hard-code status colors or surface
values outside the token layer.

| Role | Token | Value |
|---|---|---|
| App background | `--color-background` | `#fbfbfa` |
| Sidebar | `--color-sidebar` | `#f4f4f2` |
| Raised surface | `--color-surface` | `#ffffff` |
| Elevated surface | `--color-surface-raised` | `#fafaf9` |
| Input surface | `--color-inset` | `#f4f4f2` |
| Primary text | `--color-text` | `#26282b` |
| Secondary text | `--color-text-muted` | `#6d7075` |
| Disabled text | `--color-text-disabled` | `#85878b` |
| Border | `--color-border` | `#d9dadc` |
| Strong control border | `--color-border-strong` | `#85878b` |
| Action fill | `--color-accent` | `#c8323c` |
| Action hover/text | `--color-accent-strong` | `#9d242d` |
| Text on action | `--color-on-accent` | `#ffffff` |
| Running/success | `--color-success` | `#287a45` |
| Idle/warning | `--color-warning` | `#7b5b00` |
| Failed/destructive | `--color-danger` | `#b42335` |
| Unknown | `--color-unknown` | `#6550a7` |
| Focus halo | `--color-focus-ring` | `#1b63d9` |

Status always includes a visible label or icon in addition to color. Coral is
the selection and action family, but `#ef4b55` is not used for small text or as
a fill behind white text; use the darker semantic action tokens for those
pairs. Blue belongs to keyboard focus, and green remains reserved for running
or a successful operation.

## Typography

Do not fetch fonts from the network. Desktop startup and terminal metrics must
not depend on a remote font.

- UI: `-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`.
- Terminal, paths, branches, IDs, and counters: `ui-monospace, SFMono-Regular,
  Menlo, Monaco, Consolas, "Liberation Mono", monospace`.
- UI body: `0.875rem` with `1.45` line height.
- Compact labels: `0.75rem`, never smaller for essential content.
- Section title: `0.8125rem`, 600 weight, modest letter spacing.
- View title: `1rem`, 600 weight.
- Use tabular numbers for counts, PIDs, timestamps, and terminal dimensions.

## Spacing and shape

Use the four-pixel rhythm.

| Token | Value | Typical use |
|---|---|---|
| `--space-1` | `0.25rem` | icon/text gap |
| `--space-2` | `0.5rem` | control padding |
| `--space-3` | `0.75rem` | group padding |
| `--space-4` | `1rem` | section gap |
| `--space-6` | `1.5rem` | empty-state spacing |
| `--space-8` | `2rem` | major separation |

- Small radius: `4px`; normal radius: `6px`; dialogs: `8px`.
- Compact controls are 32px high. Icon-only controls still expose at least a
  32px square hit area; primary dialog actions are at least 36px high.
- Prefer borders and surface contrast to large shadows.
- Grid children must use `min-width: 0` so paths and terminal content cannot
  force horizontal overflow.

## Layout

The application has one persistent hierarchy:

```text
canvas-only application shell
├── canvas sidebar (modal drawer below 48rem)
├── primary workspace
│   ├── persistent workspace action chrome
│   └── canvas, settings, diagnostics, or connection state
│       └── terminal and note nodes (canvas only)
└── local status bar
```

- There is no separate application header, session sidebar, single-session
  view, or fixed terminal grid.
- The sidebar owns workspace navigation and project metadata actions. Global
  navigation, palette, and create actions remain in workspace chrome.
- The spatial canvas gives terminal and note nodes the largest share of the
  available area.
- At narrow widths, the sidebar becomes an explicit modal drawer. Never hide
  the navigation trigger, command palette, or primary create action.
- Keep project, session, and terminal selection when changing views.

## Components and states

### Buttons

- Use semantic `<button>` elements.
- Primary buttons use the accent token; secondary buttons use a surface and
  border; destructive buttons use danger only in a confirmed destructive flow.
- Hover and pressed feedback may change color, opacity, or border. It must not
  move surrounding layout.
- Disabled buttons normally use the native `disabled` attribute and remain
  legible. Use guarded `aria-disabled` only when the control must stay
  focusable so its unavailable reason can be announced.

### Navigation rows

- Use list semantics for project navigation. Keep the project-selection button
  separate from its sibling rename and remove buttons; never nest controls.
- Rename and remove controls stay visible, have descriptive accessible names,
  and remain keyboard reachable without relying on hover.
- Active rows use a left indicator plus contrast/weight, not color alone.
- Preserve full names through wrapping or a keyboard-accessible tooltip when
  truncation is unavoidable.

### Status

- Render icon/shape, text, and semantic color together.
- Announce lifecycle changes in one polite, atomic live region outside xterm.
- Never place terminal output in an ARIA live region.

### Forms and dialogs

- Every field has a visible label and persistent helper text when its meaning is
  not obvious.
- Show errors next to the field, connect them with `aria-describedby`, and move
  focus to the first invalid field after submit.
- Destructive operations explain what is removed and what remains on disk.
- Closing a dialog restores focus to its trigger.

### Terminals

- xterm owns terminal rendering and focus. PTY bytes never enter React state.
- A live terminal establishes an explicit dark token island (`#080b10`
  background, `#f1f5f9` foreground, cyan cursor/focus) so light shell tokens do
  not change xterm rendering.
- The active tile has a visible border and title; focus is never color-only.
- Resize only after measured dimensions change. Avoid remounting a live terminal
  when selecting, moving, or focusing its canvas node.
- Do not intercept terminal control sequences such as Ctrl+C for app commands.

## Keyboard and focus

- Provide a skip link to the primary workspace.
- Every action available by pointer is available by keyboard.
- Use a 2px or larger focus outline with at least 3:1 state contrast.
- Do not remove focus rings. Sticky toolbars and dialogs must not obscure the
  focused control.
- Respect native macOS `Cmd` conventions. On Linux, avoid stealing terminal
  `Ctrl` chords while xterm is focused; provide documented alternatives.
- Ignore global shortcuts during IME composition.

## Motion

- Use motion only to clarify opening, closing, selection, or progress.
- Prefer opacity and transform; never animate terminal dimensions.
- Keep feedback near 120–180ms and make it interruptible.
- Under `prefers-reduced-motion: reduce`, remove non-essential transitions.
- Do not add GSAP or another animation dependency for Beta v0.1.

## Iconography

- Use a single local SVG stroke style for navigation and actions.
- Do not use emoji as structural icons.
- Decorative icons next to visible text use `aria-hidden="true"`.
- Icon-only buttons require an accessible name and exposed pressed/expanded
  state when applicable.

## Forbidden patterns

- Consumer-chat bubbles or vendor-branded visual imitation.
- A second dark shell, header, project/session rail, or fixed terminal-grid
  navigation alongside the canvas.
- Remote font dependency, gradients, glassmorphism, glow-heavy decoration, or
  animated backgrounds.
- Shell output simulated with styled text instead of xterm and a real PTY.
- Clickable `div` elements, placeholder-only fields, hidden focus outlines, or
  status conveyed only by color.
- Updating React or a global store for every terminal output chunk.
- Destructive Git actions hidden behind a generic confirmation.

## Pre-delivery checks

- Test keyboard-only operation and focus restoration for every dialog.
- Verify text contrast at 4.5:1 and meaningful non-text UI at 3:1.
- Verify reduced-motion behavior.
- Verify at 375px, 768px, 1024px, and 1440px widths without horizontal page
  scrolling.
- Confirm status has text/icon plus color and no terminal output is announced.
- Confirm project removal never implies deleting its directory.
- Confirm active terminals keep buffers while canvas selection and navigation
  change.
