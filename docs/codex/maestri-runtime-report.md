# Relatório de integração S2 — runtime Maestri

Data: **2026-09-05**. Branch: **feat/maestri-runtime**.
Worktree: `/Users/eguimacs/cli-master-runtime`.
Baseline de criação: `0ac8dd7d49eefee16e5efbf389994f551bc584f5`, obtido de
`origin/refactor/canvas-only-shell`. Integração final pertence à S1 nessa branch.

O objetivo completo permanece na [matriz de runtime](../maestri-runtime-parity.md).
Esta entrega resolve o primeiro caminho backend. Floors, landing, editor,
canvas durável, presets, continuidade nativa, comunicação, rotinas e ambientes
remotos permanecem em desenvolvimento. Integração desktop não foi presumida.

## Commits disponíveis

| Commit | Alteração | Evidência local |
| --- | --- | --- |
| `7693468` | Saga separa preparação de início; associação SQLite transacional; subdiretórios; compensação e cancelamento de tokens. | Suite session/storage executada; após ajustes finais, 29 testes de create/prepare/remove passaram. Clippy session/storage sem warnings. |
| `3c25a85` | Daemon liga new_worktree, snapshot/listagem, preparo/remoção e recovery à saga compartilhando SessionManager e Storage. | 118 testes core/daemon passaram, incluindo 9 fluxos novos pelo socket real; Clippy dos quatro pacotes sem warnings. |

Ambos usam a identidade Git configurada `guicybercode`, sem trailers
`Co-authored-by`. Publicados em `origin/feat/maestri-runtime`.

## Contratos prontos para integração

| Método | Request → response | Comportamento observado |
| --- | --- | --- |
| `session.create` | `{ projectId, name, agentId, isolation: "current" | "new_worktree", relativeDirectory? } → Session` | Prepara metadata/Git sem PTY ou processo. Retorna unknown sem pid; subdiretório cadastrado e relativeDirectory preservados. |
| `session.start` | `{ sessionId } → Session` | Início explícito por SessionManager no cwd persistido. Revalida raiz/identidade Git e invalida remoção preparada. |
| `session.list` / `state.snapshot` | Contratos existentes | Sessões incluem branch/worktreeId/worktreePath; snapshot inclui worktrees duráveis. |
| `worktree.list` | **Novo:** `{ projectId?: UUID } → { worktrees: Worktree[] }` | Todas as worktrees ou filtro por projeto; estados parciais permanecem visíveis. |
| `worktree.prepare_remove` | `{ worktreeId } → ready + confirmationToken/expiresAtMs ou blocked + isDirty/blockers` | Inspeção real de Git e uso por sessões, inclusive cwd sem associação direta. |
| `worktree.remove` | `{ worktreeId, confirmationToken } → {}` | Reinspeção e confirmação vinculada ao estado; dirty/ignored/uso impedem exclusão. |
| `session.delete` | `{ sessionId } → {}` | Remove apenas metadados parados; preserva diretório e branch. Saída sem assinante também pode ser excluída. |

Rust wire, `protocol/catalog.json`, `ipc/methods.ts` e `ipc/domain.ts`
foram sincronizados. Os dois últimos são artefatos aditivos: o baseline havia
removido esses caminhos citados em AGENTS. Nenhum componente React, estado de
canvas, estilo ou bridge Tauri foi alterado.

Erros relevantes: `worktree_confirmation_invalid`, `worktree_in_use`,
`worktree_dirty`, `worktree_not_active`, `worktree_identity_changed`,
`session_directory_unavailable`, `session_still_running`, `git_unavailable`.
As respostas têm mensagens/ações; não transportar env ou argumentos em logs.

**Refresh explícito:** este incremento não implementa emissão global de
`worktree.updated`/`worktree.removed`. S1 deve reler snapshot/list após mutação
ou reconexão. A existência dos nomes no catálogo não prova entrega de eventos.

## Verificação executada e limites

PR de integração: [#44](https://github.com/guicybercode/Jig/pull/44), em draft.
CI e Packaging Linux/macOS iniciados; resultados ainda pendentes.

Host local: macOS. Passaram:

- `CARGO_INCREMENTAL=0 cargo test -p cli-master-core -p cli-master-daemon --locked`
  — 118 testes, dos quais 9 novos em `worktree_ipc.rs`.
- Testes session/storage e 29 testes finais de sagas, conforme primeiro commit.
- `CARGO_INCREMENTAL=0 cargo clippy -p cli-master-core -p cli-master-storage -p cli-master-session -p cli-master-daemon --all-targets --locked -- -D warnings`.
- `cargo fmt --all -- --check`, `git diff --check` e `bash scripts/check-versions.sh`.
- Rustdoc dos quatro pacotes com `RUSTDOCFLAGS=-Dwarnings`.
- Typecheck frontend e 8 testes dos contratos/client IPC existentes.

Os casos socket exercitam start/stop/restart concorrentes, troca de raiz por
symlink entre create/start, token após edição externa, sessão sem vínculo
usando o checkout, saída espontânea e restart com PID canário de outro manager.
A matriz CI Linux/macOS e o pacote desktop ainda exigem resultado externo;
não foram chamados de aprovados. O editor/canvas S1 ainda precisa consumir e
verificar estes contratos. Os IDs M14/M34/M35 continuam parciais no escopo total.

## Acordo com XIRP/S3

Resposta publicada em
`/Users/eguimacs/cli-master-xirp/docs/codex/xirp-coordination-reply.md`:

- S3 possui novos módulos knowledge e componentes isolados.
- **0004_knowledge_documents.sql reservada para S3**; worktrees não criam migração.
- Acordados `knowledge.list/save/delete`, kind prompt/context, UUIDv7,
  escopo opcional de projeto, revisão inteira; update/delete exigem revisão,
  conflito retorna `knowledge_conflict`. Ainda não são contratos publicados.
- S3 pode publicar registro aditivo de lib.rs/migração/wire/mirrors/dispatch na
  própria branch junto de implementação/testes; S2 revisa e integra o commit.
- `knowledge.updated` só pode ser anunciado como funcional com emissão real.
- S2 possui file service, workspace/floor/canvas e organização/workflow.
  S3 possui composição de rascunhos/contexto; entrega usa SessionManager/adapters.

## Próxima etapa

Implementar serviço local `file.list/read/write` com alvos registrados,
leitura limitada, caminhos Unix preservados, escrita atômica e revisão de
conteúdo, documentando limites de concorrência externa. S3 reutiliza a leitura
segura. Em seguida, workspace/floor e canvas durável com revisão e importação
explícita do localStorage pela S1. A matriz mantém os incrementos posteriores;
esta ordem não reduz o objetivo aos serviços de arquivos.
