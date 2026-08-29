# Canvas workspace override

This page intentionally departs from the dark-first application shell because
the requested spatial workspace is modeled after a light macOS canvas. The
underlying terminal surfaces remain dark and high contrast; dialogs and legacy
project/session views continue to use the master system until migrated.

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
| Selection/action | `--canvas-accent` | `#ef4b55` |
| Connection | `--canvas-connector` | `#8b8e93` |
| Focus ring | `--canvas-focus` | `#1b63d9` |

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

- At 1024px and above, the workspace sidebar is persistent.
- Below 1024px, the sidebar becomes an overlay opened from the toolbar.
- Below 768px, new nodes open near the viewport origin and node widths clamp to
  the available canvas; the canvas itself remains pannable.
- Zoom controls remain reachable in the lower-right corner and never obscure a
  focused node.
