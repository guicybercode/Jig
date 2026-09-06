# Relatório de integração S2 — runtime Maestri

Data: **2026-09-05**. Branch: **feat/maestri-runtime**.
Worktree: `/Users/eguimacs/cli-master-runtime`.
Baseline de criação: `0ac8dd7d49eefee16e5efbf389994f551bc584f5`, obtido de
`origin/refactor/canvas-only-shell`. Integração final pertence à S1 nessa branch.

O objetivo completo permanece na [matriz de runtime](../maestri-runtime-parity.md).
A integração de worktrees está publicada e a primeira fatia de arquivos/editor
está publicada com testes locais aprovados. Floors, landing, canvas durável, presets, continuidade nativa,
comunicação, rotinas e ambientes remotos permanecem em desenvolvimento. Integração desktop não foi presumida.

## Commits disponíveis

| Commit | Alteração | Evidência local |
| --- | --- | --- |
| `7693468` | Saga separa preparação de início; associação SQLite transacional; subdiretórios; compensação e cancelamento de tokens. | Suite session/storage executada; após ajustes finais, 29 testes de create/prepare/remove passaram. Clippy session/storage sem warnings. |
| `3c25a85` | Daemon liga new_worktree, snapshot/listagem, preparo/remoção e recovery à saga compartilhando SessionManager e Storage. | 118 testes core/daemon passaram, incluindo 9 fluxos novos pelo socket real; Clippy dos quatro pacotes sem warnings. |
| `910ed7d`, `1a74bbb` | Integra documentação e persistência knowledge da S3; dispatch bloqueante sai do leitor assíncrono. | Contratos core, 9 testes storage knowledge, 2 socket knowledge, 9 socket worktree e 15 testes IPC frontend passaram. |
| `b271825` | ADR 0006 define arquivos/editor, revisão e política de salvamento. | Decisão de arquitetura; não representa implementação funcional. |
| `45d817a` | Preserva o canvas e o navegador nativo publicados pela S1 até `1e75081`. | Typecheck frontend e testes daemon lib/IPC/knowledge/worktree passaram; CI e Packaging Linux/macOS aprovados. |
| `c98cf25` | Probe retenta ETXTBSY com o limite existente e preserva classificação/errno sem dados sensíveis. | 56 testes agents e Clippy passaram no macOS; duas regressões específicas de ETXTBSY aguardam Linux CI. |
| `1225931` | Serviço file.list/read/write, salvamento atômico com revisão, metadados preservados e cliente IPC tipado. | 13 contratos file, 8 casos internos de disco/falha, 12 socket file, 5 ACL macOS; 51 testes IPC frontend passaram. |

Os commits usam a identidade Git configurada `guicybercode`, sem trailers
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
canvas, estilo ou bridge Tauri foi alterado por S2; as mudanças S1 foram
preservadas na integração. O cliente IPC ganhou métodos tipados de arquivo.

Erros relevantes: `worktree_confirmation_invalid`, `worktree_in_use`,
`worktree_dirty`, `worktree_not_active`, `worktree_identity_changed`,
`session_directory_unavailable`, `session_still_running`, `git_unavailable`.
As respostas têm mensagens/ações; não transportar env ou argumentos em logs.

**Refresh explícito:** este incremento não implementa emissão global de
`worktree.updated`/`worktree.removed`. S1 deve reler snapshot/list após mutação
ou reconexão. A existência dos nomes no catálogo não prova entrega de eventos.

## Verificação executada e limites

PR de integração: [#44](https://github.com/guicybercode/Jig/pull/44), em draft.
CI e Packaging de `45d817a` passaram em Linux/macOS, nos runs
[CI 34003098853](https://github.com/guicybercode/Jig/actions/runs/34003098853) e
[Packaging 34003098946](https://github.com/guicybercode/Jig/actions/runs/34003098946).
Os commits posteriores de probe/arquivos exigem novas execuções; a aprovação
anterior não comprova esses novos caminhos.

Host local: macOS. Na primeira integração de worktrees, passaram:

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
CI e Packaging da integração anterior estão aprovados conforme os runs
acima; o serviço novo de arquivos aguarda sua própria execução Linux/macOS. O editor/canvas S1 ainda precisa consumir e
verificar estes contratos. Os IDs M14/M34/M35 continuam parciais no escopo total.

## Acordo com XIRP/S3

Resposta publicada em
`/Users/eguimacs/cli-master-xirp/docs/codex/xirp-coordination-reply.md`:

- S3 possui novos módulos knowledge e componentes isolados.
- **0004_knowledge_documents.sql reservada para S3**; worktrees não criam migração.
- Acordados `knowledge.list/save/delete`, kind prompt/context, UUIDv7,
  escopo opcional de projeto, revisão inteira; update/delete exigem revisão,
  conflito retorna `knowledge_conflict`. Publicados em `1a74bbb`.
- S3 pode publicar registro aditivo de lib.rs/migração/wire/mirrors/dispatch na
  própria branch junto de implementação/testes; S2 revisa e integra o commit.
- `knowledge.updated` só pode ser anunciado como funcional com emissão real.
- S2 possui file service, workspace/floor/canvas e revisão dos contratos.
  S3 possui módulos organização/workflow e composição de rascunhos/contexto;
  entrega inicial usa SessionManager/adapters, sob responsabilidade S2.
- **0005_organization.sql reservada para S3**; workspace S2 usa 0006 depois
  de integrar 0005. `organization.get/save` aprovados como proposta; não
  anunciados no catálogo sem handlers e testes. Pin/archive nunca alteram PTY.
- `knowledge.discover/read` aprovados para a próxima entrega S3 com IDs opacos
  de scan/entry, allowlist conhecida e raízes globais somente leitura. A fatia
  inicial de arquivo fornece leitura segura sob alvos registrados; adaptar
  capacidades globais requer integração explícita e ainda não foi concluído.

## Arquivos/editor publicados

`1225931` publica os três métodos sob alvos cadastrados:

| Método | Request → response |
| --- | --- |
| `file.list` | `{target,pathBase64,limit?,afterNameBase64?}` → `{entries,nextAfterNameBase64?,observedAtMs}` |
| `file.read` | `{target,pathBase64}` → `{pathBase64,text,revision,sizeBytes,modifiedAtMs?,observedAtMs}` |
| `file.write` | `{target,pathBase64,text,expectedRevision}` → `{pathBase64,revision,sizeBytes,modifiedAtMs?,writtenAtMs}` |

`target` identifica projeto, sessão ou worktree; a raiz é resolvida pelo daemon.
`pathBase64` preserva bytes Unix relativos, sem reconstruir caminhos a partir
de displayName. Texto é UTF-8 de até 128 KiB, sem NUL, com BOM/CRLF preservados.
A revisão opaca `v1:` é derivada de conteúdo e identidade/metadados. Listagem
pagina 100 por padrão/200 no máximo, enumera no máximo 10.000 nomes e limita
cada resposta a 512 KiB. O cliente expõe `listFiles`, `readFile`, `writeFile`.

O save mantém proprietário/grupo/permissões e verifica metadados por descritor.
macOS preserva também `com.apple.provenance` com comparação exata; outros
xattrs, ACLs, flags e hardlinks não suportados são recusados. A exceção FFI
Darwin está isolada em file-metadata, revisada em SAFETY.md e coberta por 5
casos reais. Workspace/core/daemon/session continuam com unsafe proibido.

A publicação final usa a mesma proteção de mutação que a remoção de worktree
ou metadados do alvo. Duas gravações do daemon não confirmam a mesma revisão;
edições externas observadas geram `file_conflict`, preservando a versão em
disco. A comparação seguida de rename ainda é otimista diante de um editor
externo não cooperativo: não há CAS atômico portátil entre processos.
`file_durability_uncertain` com `writeApplied:true` exige reler antes de tentar
novamente. O cliente preserva esses metadados e não faz um segundo write.

A execução local final incluiu 97 testes core, 68 daemon, 56 agents e 5 ACL,
mais 51 testes frontend IPC. Passaram typecheck, Clippy dos quatro pacotes,
Rustdoc com warnings negados, rustfmt e verificação de versões. Debug info
foi desativada somente nos comandos de validação para reduzir uso de disco.
APFS rejeitou a fixture de nome UTF-8 inválido com EILSEQ; os demais nomes Unix
foram exercitados pelo socket. Esse caso de bytes inválidos é obrigatório no
socket Linux e também é coberto pelos contratos puros. A nova CI Linux/macOS
ainda precisa confirmar este incremento; a evidência local é macOS.

M29/M30 continuam parciais no produto: criação/movimentação/exclusão de arquivo,
watcher, integração de editor S1 e reader global S3 ainda não estão prontos.
O método interno compartilhável não comprova reúso já concluído pelo scanner.
Não há evento file.changed anunciado sem transporte. S1 deve manter o buffer
em erro, reler após conflito/reconexão e consumir o identificador retornado.

## Próxima etapa

Concluir operações de gerenciamento de arquivos conforme ADR 0006; depois,
workspace/floor e canvas durável com revisão e importação explícita do
localStorage pela S1. Organização pin/archive/workflow pertence à S3, com
migração 0005 reservada, antes da próxima migração S2. A matriz mantém presets,
continuidade, comunicação, rotinas e ambientes remotos como trabalho restante.

## Reconciliação com S1 após arquivos

A branch incorpora S1 até `dd84a49`, incluindo biblioteca/compositor de prompts,
refresh de worktrees e o tratamento de prazo de probe em `645047b`. Preserva-se
o diagnóstico saneado da S1 e o timeout explícito após esgotar o prazo; as
regressões reais de ETXTBSY da S2 permanecem no Linux. Dois testes AppShell
agora fornecem respostas explícitas de listWorktrees, inclusive a lista vazia
após remoção, em acordo com o novo refresh da S1.

Após resolver os conflitos, passaram 97 testes core, 68 daemon e 63 agents;
Clippy dos quatro pacotes sem warnings. O check frontend completo passou:
278 testes, typecheck e build Vite. Essa execução não substitui o smoke nativo
nem a nova CI Linux/macOS. O serviço de arquivos e os controles S1 coexistem;
o editor de arquivos ainda precisa ser montado pela S1.
