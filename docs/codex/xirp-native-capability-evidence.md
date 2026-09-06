# Native initial objective and continuity evidence

Checked 2026-09-05 by reading the local adapters and running only the installed
CLI --help/--version commands. No agent conversation, login or private
configuration/transcript read was performed. This is interface evidence, not
proof of a successful native resume or fork.

| Capability | codex-cli 0.153.4 | Claude Code 2.1.261 |
| --- | --- | --- |
| Initial objective | `codex [OPTIONS] [PROMPT]` | `claude [options] [prompt]` |
| Explicit resume | `codex resume [OPTIONS] [SESSION_ID] [PROMPT]` | `claude --resume <ID> [prompt]` |
| Explicit fork | `codex fork [OPTIONS] [SESSION_ID] [PROMPT]` | `claude --resume <ID> --fork-session [prompt]` |
| Predetermined conversation ID | No option identified in installed help | `--session-id <uuid>` advertised; creation not exercised |
| Model override | `--model <MODEL>` | `--model <model>` |
| Approval/permission modes | `--ask-for-approval on-request|never` | `--permission-mode manual|acceptEdits|plan|auto|dontAsk|bypassPermissions` |
| Sandbox | `--sandbox read-only|workspace-write|danger-full-access` | No equivalent flag identified in installed help |
| Effort | No dedicated flag identified; generic --config exists | `--effort low|medium|high|xhigh|max` |

The [official Codex command reference](https://learn.chatgpt.com/docs/developer-commands?surface=cli)
confirms positional objectives, resume/fork and model/sandbox overrides. It
also describes untrusted approval mode, absent from this installed help's
accepted values; documentation alone must not populate the version-specific
picker. The reference supports explicit --cd when choosing a different cwd.

The [official Claude CLI reference](https://code.claude.com/docs/en/cli-reference)
confirms the corresponding flags and explains manual as an alias of default
since 2.1.200. Its additional ultracode effort is absent from installed help.
Model availability and permissions were not exercised. --print changes the
execution mode and is not an appropriate shortcut for obtaining interactive
conversation IDs.

## Current repository gaps and S2 integration boundary

- Codex/Claude still use the generic BuiltInAdapter. Its internal capabilities
  describe interactive/PTY/version-probe/extra-args support, without typed
  objective, native identity, model or permission capabilities.
- The active daemon SessionRegistry::start_id builds through
  StoredAgent::command_for_cwd directly. Merely extending AgentAdapter's builder
  does not affect this execution path. Spawn remains SessionManager-owned.
- CommandSpec.startup_input exists with a 4096-byte bound, but no code in
  crates/session reads it in this branch. Storing a value there does not deliver
  an initial objective. Session/create/start wire and stored-session metadata
  also lack objective, per-session options and native conversation identity.
- AgentRecord/AgentDetection wire do not expose verified capability versions.
  Display names and adapter keys must not be used to infer CLI behavior.

S2 should define a typed new/resume/fork launch intent and bounded objective,
route the actual start path through the native adapter, and retain structured
argv. A prompt beginning with '-' requires verified argument termination so
text cannot become options. Deterministic child-argv tests can prove local
transport and redaction; they do not prove a vendor accepted the objective.

NativeConversationId must remain distinct from the local UUIDv7 SessionId.
Avoid binding by PID, PTY, display name or a changing 'last conversation'.
Claude permits requesting a creation UUID, but successful creation/association
still needs observation. This review did not establish Codex ID acquisition.
Only expose resume/fork once both identity and native support are trustworthy.

The [Claude session documentation](https://code.claude.com/docs/en/sessions)
explains that simultaneous resumes of one ID interleave transcript messages.
Managed resume therefore needs exclusivity; fork needs another native identity
as well as another local session. Real continuation acceptance remains open.
