# CLI Master design system

This file is the visual source of truth for the desktop application. Page-level
overrides belong in `pages/<page-name>.md` and must document why they differ.

## Product direction

CLI Master is a dense, keyboard-first developer control center. It should feel
closer to a terminal multiplexer or code editor than a consumer chat product.
The interface is dark-first, restrained, fast, and readable during long work
sessions.

Design dials:

- Variance: 4/10 — balanced, with predictable placement.
- Motion: 2/10 — feedback only; no decorative choreography.
- Density: 8/10 — compact desktop controls with usable hit areas.

## Color tokens

Use semantic tokens in components. Do not hard-code status colors or surface
values outside the token layer.

| Role | Token | Value |
|---|---|---|
| App background | `--color-background` | `#0b0f14` |
| Sidebar | `--color-sidebar` | `#0f141b` |
| Raised surface | `--color-surface` | `#151b23` |
| Elevated surface | `--color-surface-raised` | `#1b2430` |
| Input/terminal surface | `--color-inset` | `#080b10` |
| Primary text | `--color-text` | `#f1f5f9` |
| Secondary text | `--color-text-muted` | `#a8b3c2` |
| Disabled text | `--color-text-disabled` | `#687386` |
| Border | `--color-border` | `#2b3543` |
| Strong border | `--color-border-strong` | `#465368` |
| Action/focus | `--color-accent` | `#38bdf8` |
| Action hover | `--color-accent-strong` | `#7dd3fc` |
| Text on action | `--color-on-accent` | `#07131a` |
| Running/success | `--color-success` | `#4ade80` |
| Idle/warning | `--color-warning` | `#fbbf24` |
| Failed/destructive | `--color-danger` | `#fb7185` |
| Unknown | `--color-unknown` | `#c4b5fd` |
| Focus halo | `--color-focus-ring` | `#bae6fd` |

Status always includes a visible label or icon in addition to color. Blue/cyan
belongs to actions and keyboard focus; green remains reserved for running or a
successful operation.

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

The persistent hierarchy is:

```text
application header
├── project/session sidebar
└── workspace main
    ├── session toolbar
    ├── single terminal or terminal grid
    └── optional Git panel
status bar
```

- The sidebar is navigation and metadata, not the home for primary actions.
- The workspace gives terminals the largest share of available area.
- One terminal fills the workspace; two use two columns when width permits;
  three emphasize the active terminal; four use a 2×2 grid.
- At narrow widths, collapse secondary metadata before reducing terminal
  usability. Never hide the active session or primary create action.
- Keep project, session, and terminal selection when changing views.

## Components and states

### Buttons

- Use semantic `<button>` elements.
- Primary buttons use the accent token; secondary buttons use a surface and
  border; destructive buttons use danger only in a confirmed destructive flow.
- Hover and pressed feedback may change color, opacity, or border. It must not
  move surrounding layout.
- Disabled buttons use the native `disabled` attribute and remain legible.

### Navigation rows

- Use `ul > li > button` for project and session lists.
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
- The active tile has a visible border and title; focus is never color-only.
- Resize only after measured dimensions change. Avoid remounting a live terminal
  solely because single/grid mode changed.
- Do not intercept terminal control sequences such as Ctrl+C for app commands.

## Keyboard and focus

- Provide a skip link to the active terminal.
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
- Confirm active terminals keep buffers and focus across single/grid changes.
