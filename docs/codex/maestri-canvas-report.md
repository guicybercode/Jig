# S1 — canvas Maestri e integração XIRP

Atualizado em 2026-09-05. A meta completa continua ativa; esta entrega não
constitui paridade integral com Maestri nem integração com Spotify Portal.
O inventário completo está em [maestri-xirp-parity.md](../maestri-xirp-parity.md).

## Entrega verificada

| Commit | Resultado |
| --- | --- |
| `9f3462f` | Seleção múltipla transitória; movimento em grupo preservando distâncias nos limites; duplicação do subgrafo sem copiar sessão viva; remoção oculta cartões de sessões. |
| `0ac8dd7` | Mudanças de seleção não gravam/publicam documento; falha de armazenamento é indicada e edições permanecem na memória. |
| `a23587d` | Gemini no cadastro público do daemon com ID UUIDv7 estável; detecção reutiliza o adapter e verifica permissão de execução. |
| `472926c` | Preset Gemini no diálogo de terminal e na persistência do canvas. |
| `6774865` | Inventário de paridade e dois goals com ownership para sessões paralelas. |
| `e2da3c2` | Shift+click/Shift+Space, seleção total, duplicação/remoção/arraste em grupo; busca por título, nota, agente, branch e caminho. Cópia mantém referência ao agente original, incluindo sua configuração, sem persistir seu ambiente no canvas. |
| `e75e6cb` | Notas e drafts disponíveis sem daemon; operações de sessão continuam bloqueadas. Reconciliação offline não apaga sessões ocultadas. |
| `180513f` | Reutilizada a compatibilidade de testes Node 25 já existente na main, com spies de storage compatíveis. |
| `ace4e8f` | Canvas ocupa toda a janela compacta; título e seleção não se sobrepõem; cinco testes de navegador novos. |

Os commits foram enviados individualmente para
`origin/refactor/canvas-only-shell`, com autoria Git do usuário e sem trailers
de coautoria Codex/OpenAI. Nenhuma sessão foi iniciada ao duplicar cartões e
nenhuma operação de stop/delete do daemon é chamada ao remover uma seleção.

## Validação e limites

- `pnpm --filter @cli-master/desktop check`: TypeScript, **118 testes** e
  build de produção passaram no macOS com Node 25.9. O build ainda informa
  bundle principal maior que 500 kB; isso não foi corrigido nesta entrega.
- `pnpm --filter @cli-master/desktop exec playwright test --workers=2`:
  **9 testes** passaram no Chromium/macOS, com build novo. Incluem ponteiro,
  teclado, persistência após reload e dimensões de 360/640/1440 px.
- `cargo test -p cli-master-daemon`: **37 testes** passaram; Clippy do daemon
  com todos os targets e `-D warnings` e formatação passaram. Os testes usam
  arquivos, permissões Unix e SQLite reais; não exigem login no Gemini.
- A build Rust usou `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
  CARGO_PROFILE_TEST_DEBUG=0` por espaço limitado. Apenas artefatos gerados
  desta worktree foram removidos quando o disco esgotou; fontes e dados do
  usuário foram preservados. Os artefatos são regeneráveis.
- Capturas desktop/compacta foram inspecionadas. Asserções geométricas
  reproduziram e depois verificaram a correção da largura compacta.
- E2E no navegador verifica cartões locais/drafts. Testes de componente
  verificam seleção de terminais com IPC do projeto simulado. Isso **não**
  prova PTY em janela Tauri, empacotamento nativo, login no Gemini ou Linux.
  A matriz Linux/macOS de CI e os smoke tests nativos continuam necessários.

M02 e M06 avançaram, mas ainda faltam copiar/colar, grupos persistidos,
alinhamento, undo/redo, busca global e atalhos configuráveis. M46 continua
parcial; esta revisão visual não prova fidelidade de todas as telas/estados.

## Próximas integrações reais

1. Integrar o navegador já entregue em `origin/main` pelo PR 40. O baseline
   desta sessão não o contém. A implementação inclui host WKWebView/WebKitGTK,
   navegação, foco, redaction de URL, proteção contra obstruções e smoke
   harness nativo. Reutilizar esse trabalho antes de implementar M37 do zero.
   A análise de merge encontrou conflitos em AppShell, CanvasWorkspace e
   canvas-state com seus testes. Manter shell claro/canvas-only, isolamento
   por projeto, IDs ocultados, referência de agente, seleção múltipla e modo
   offline. A migração v2 deve manter v1, e somente o nó primário ativa uma
   superfície nativa; busca/menus/movimentos em grupo precisam ocultá-la.
2. S2: integração real daemon/worktree, preservando `create` seguido de
   `start`. A saga interna ainda não está conectada ao caminho público.
   Contratos de arquivos/notas/floors seguem o inventário.
3. S3: prompts/contexto, regras/skills e organização em módulos próprios.
   [Coordenação com S3](xirp-integration-request.md) define inserção explícita
   em draft editável; o composer e sua montagem real ainda serão entregues.
4. Continuar as linhas obrigatórias da matriz, incluindo lift/dock, conteúdo,
   portais/dispositivos, automações, continuidade, ambientes e capacidades
   de plataforma. A entrega atual não reduz esse escopo.

As worktrees das sessões adicionais já existem. Os paths efetivamente
observados são `/Users/eguimacs/cli-master-runtime` e
`/Users/eguimacs/cli-master-xirp`; os nomes propostos no documento de goals
não autorizam recriá-las nem apagar seu trabalho.
