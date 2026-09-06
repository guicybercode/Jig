# Local rule and skill discovery sources

Verified 2026-09-05. This inventory describes discovery candidates, not a claim
that a running agent loaded or executed every listed file. Native configuration,
trust, scope and version may change effective behavior. The app does not parse
credential-bearing settings to infer enabled plugins or custom discovery paths.

| Format | Verified paths and precedence | Source |
| --- | --- | --- |
| Codex instructions | `CODEX_HOME` (default `~/.codex`): first nonempty `AGENTS.override.md`, then `AGENTS.md`. At each directory from repository root to cwd, same order before configured fallback names; more specific directories follow root instructions. Default aggregate bound 32 KiB. | [AGENTS.md](https://learn.chatgpt.com/docs/agent-configuration/agents-md) |
| Codex skills | `.agents/skills` from cwd up to repo root, `~/.agents/skills`, `/etc/codex/skills`, plus bundled system skills. Duplicate names remain separate candidates. Symlinked skill directories are supported. | [Build skills](https://learn.chatgpt.com/docs/build-skills) |
| Claude instructions/rules | User `~/.claude/CLAUDE.md`, project `CLAUDE.md` or `.claude/CLAUDE.md`, personal `CLAUDE.local.md`. User `~/.claude/rules/*.md` and project `.claude/rules/**/*.md`; user rules precede project rules. Parent instructions load at launch, descendants can load on demand. Imports exist, but discovery must not follow arbitrary imported files. | [Memory](https://code.claude.com/docs/en/memory) |
| Claude skills | `~/.claude/skills/<name>/SKILL.md` and `.claude/skills/<name>/SKILL.md`, including parent/nested project scopes. Personal overrides project name collisions; plugin names are namespaced. Skill directories may be symlinks; aliases to the same target are deduplicated by the CLI. | [Skills](https://code.claude.com/docs/en/skills) |
| Cursor rules | Project `.cursor/rules/**/*.mdc`, with path/manual/relevance conditions. Plain `.md` files are ignored there. `AGENTS.md` is supported. Global user rules are application settings rather than standalone rule files; no invented home-file location. | [Rules](https://cursor.com/docs/rules) |
| Cursor skills | Project `.agents/skills`, `.cursor/skills` and user `~/.agents/skills`, `~/.cursor/skills`. Skill roots can contain nested groups and project subdirectories; nested candidates have directory scope. | [Skills](https://cursor.com/docs/skills) |

## Local reader decisions to verify

The inspector will show provider/format, global/project origin, relative scope,
source path and a precedence explanation. A candidate is not automatically
marked active: editor settings, custom fallback names, plugins, conditional
frontmatter and selected session cwd can affect native loading.

No credential/config JSON, arbitrary Markdown imports, support scripts or
transcripts enter the inventory. The reader only opens an allowlisted rule or
`SKILL.md`, requires regular UTF-8 files and applies byte/count/depth limits.
Symlinked skill directories need an explicit verified target policy and race
protection. Rule edits will use S2's common file service once its contract is
published, rather than adding a second generic editor.

Directory discovery and content read are local operations. They never invoke a
CLI, execute scripts, install skills, change native configuration or contact
Portal. Missing, changed, unreadable, oversized and unsafe candidates must be
reported as such rather than silently fabricated as empty files.
