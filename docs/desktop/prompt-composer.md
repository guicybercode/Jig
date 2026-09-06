# Prompt Composer

Open the composer from a terminal card's pencil button, the canvas toolbar,
or `Ctrl+Shift+P` on Linux / `Cmd+Shift+P` on macOS. It follows the selected
terminal and does not replace or unmount its terminal surface.

Each card retains its own draft in the existing local canvas document.
Escape, changing selection/project, and reopening the application preserve
the draft. A storage failure is shown by the canvas; in-memory edits remain
available, but a failed save cannot survive closing the application.

## Sending

Enter or **Send prompt** explicitly submits to the named, live terminal.
Shift+Enter inserts a newline. This is terminal input: in a shell it may
execute a command, and in a CLI it interacts with that CLI's current screen.
The application does not infer whether the program is asking for approval.
Drafting never creates, starts, stops or restarts a session.

With an empty draft, unmodified Enter, Tab and arrows go to the live terminal.
Shift+Tab leaves the editor. When the terminal is unavailable, Tab keeps its
normal focus-navigation behavior. IME composition and repeated submission keys
cannot accidentally send a prompt.

Composer and keyboard input share one FIFO. Encoding uses the current xterm
paste/cursor modes, not an agent-name guess. The payload is one bounded IPC
write, including Return. Embedded terminal control characters and input over
the wire's 64 KiB byte limit are rejected without truncation. Errors preserve
the draft. There is no automatic retry; inspect the terminal before resending.

A successful acknowledgment clears only the submitted draft revision. Editing
while sending, closing/reopening the pane, or selecting another terminal cannot
clear a newer draft. A queued write whose session/PTY changed is rejected.

## Context and remaining parity

Connected notes and redacted browser addresses can be explicitly inserted as
labeled text snapshots. They are not live references and do not grant agent
access to files, browser controls or other terminals.

This implements the text/draft/input portion of
[Maestri's composer](https://www.themaestri.app/en/docs/prompt-composer).
Live mentions, image/file attachments, SSH delivery, slash-command discovery
and Maestro operations remain separate requirements in M19 and related rows
of the parity matrix; this increment does not mark that scope complete.

Verification includes project-owned terminal wrapper/transport tests, revision
reducer tests, canvas integration tests and real offline browser workflows.
These do not replace packaged Linux/macOS PTY and native CLI acceptance.
