# Canvas-only shell

This page specializes the light-first application system for the spatial
workspace. The canvas is the only shell: its sidebar, action chrome, dialogs,
palette, banners, settings, diagnostics, and status bar all use the light
palette. Only xterm terminal surfaces remain dark and high contrast.

## Product model

- The workspace is an infinite-feeling, pannable board for terminal and note
  nodes rather than a fixed dashboard grid.
- Connections represent collaboration context between nodes. They never pipe
  shell bytes or implicitly execute commands.
- Dragging is optional: every node can also be moved and connected from visible
  controls that work with keyboard and a single pointer.

## Visual direction

- Canvas: warm white with a 16px fine grid and a 128px major grid.
- Sidebar: translucent off-white surface with a solid fallback and a restrained
  shadow only where it separates from the canvas.
- Project rows: separate selection, rename, and remove buttons. Both project
  actions are always visible and keyboard reachable rather than hover-revealed.
- Nodes: white terminal cards and pale-yellow notes with 8px corners, one-pixel
  borders, compact macOS-style headers, and soft elevation.
- Selection: coral-red outline plus a visible `Selected` label; never color
  alone.
- Connections: neutral graphite curves by default, coral-red only while
  selected or being created.
- Toolbar: centered floating control group with 40px targets and local SVG
  outline icons.

## Canvas tokens

| Role | Token | Value |
|---|---|---|
| Canvas background | `--canvas-background` | `#fbfbfa` |
| Fine grid | `--canvas-grid-fine` | `rgb(80 88 96 / 7%)` |
| Major grid | `--canvas-grid-major` | `rgb(80 88 96 / 10%)` |
| Chrome surface | `--canvas-chrome` | `rgb(250 250 249 / 94%)` |
| Card | `--canvas-card` | `#ffffff` |
| Note | `--canvas-note` | `#fff6b7` |
| Primary text | `--canvas-text` | `#26282b` |
| Secondary text | `--canvas-text-muted` | `#6d7075` |
| Border | `--canvas-border` | `#d9dadc` |
| Selection outline | `--canvas-accent` | `#ef4b55` |
| Action text/hover | `--canvas-accent-ink` | `#9d242d` |
| Action fill | `--canvas-accent-fill` | `#c8323c` |
| Connection | `--canvas-connector` | `#8b8e93` |
| Control border | `--canvas-control-border` | `#85878b` |
| Focus ring | `--canvas-focus` | `#1b63d9` |

The coral selection outline passes non-text contrast on white, but it does not
pass normal-text contrast with white text. Use `--canvas-accent-ink` for coral
text and `--canvas-accent-fill` behind white labels. Use the stronger control
border wherever a field or option needs its boundary to identify the control.

## Interaction constraints

- Node headers are drag handles; text areas and terminal surfaces never start a
  drag.
- Arrow-key movement uses 8px increments, or 1px with Alt/Option.
- Connect mode is two-step: choose `Connect`, then choose a compatible target.
  Escape cancels without mutating the graph.
- A node action menu exposes `Move`, `Connect`, and `Delete`, so dragging is not
  the only path.
- Notes autosave locally after edits and announce `Saved` without stealing
  focus.
- Connection lines are behind nodes and use `pointer-events: none`; removal is
  available from the selected node's connection list.

## Responsive behavior

- At `48rem` and above, the workspace sidebar is persistent and may be
  collapsed without losing the navigation trigger.
- Below `48rem`, the sidebar is a modal drawer with a backdrop, focus trap,
  Escape dismissal, and focus restoration to its trigger.
- At `68rem` and below, navigation/palette/create actions occupy the first
  chrome row; the canvas toolbar moves to a second row and the workspace context
  follows it, so the groups never overlap as the workspace narrows.
- Below `48rem`, new nodes open near the viewport origin and terminal node width
  clamps to the available canvas; the canvas itself remains pannable.
- Zoom controls remain reachable in the lower-right corner and never obscure a
  focused node.
