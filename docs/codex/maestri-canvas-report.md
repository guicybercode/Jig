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
| `1e75081` | Navegador nativo de `main` integrado ao canvas por projeto, seleção primária e obstruções; migração conserva documentos v1. URLs persistidas são sanitizadas; harness nativo passa a reprovar ausência de bridge e timeout. |
| `839e319`, `a5a8cf6`, `b518513` | Draft por terminal com revisão; composer montado, snapshots explícitos e envio pela mesma fila de input do teclado, respeitando modos xterm. Confirmação antiga não apaga texto novo; trocar PTY invalida escrita pendente. |
| `1f1209f`, `7263d10` | Integração S2: preparar checkout isolado não inicia processo; daemon usa a saga no fluxo público `create → start` e fornece `worktree.list`. Helper Gemini atualizado para o novo construtor. |
| `7d86463`, `9e2a070` | Backend S3 reconciliado por S2: prompts/contexto global ou por projeto em SQLite, revisão otimista, handlers reais e cliente tipado; biblioteca com draft separado por escopo. |
| `645047b` | Probe de executáveis: retry limitado para `ETXTBSY`; diagnóstico só contém tipo/errno, sem caminho, argv ou mensagem original. A causa da falha histórica do CI não foi comprovada. |
| `dd84a49` | Lista de worktrees atualizada após criação/start isolado e remoção, protegida contra respostas antigas; cliente estável para operações da biblioteca. |
| `1150de6` | Biblioteca montada no canvas e paleta; inserção explícita em draft sem enviar; escolha de worktree isolada no diálogo do canvas; fixtures de refresh corrigidas e E2E offline da biblioteca. |

Os commits foram enviados individualmente para
`origin/refactor/canvas-only-shell`, com autoria Git do usuário e sem trailers
de coautoria Codex/OpenAI. Nenhuma sessão foi iniciada ao duplicar cartões e
nenhuma operação de stop/delete do daemon é chamada ao remover uma seleção.

## Validação e limites

- `pnpm --filter @cli-master/desktop check`: TypeScript, **253 testes** e
  build de produção passaram no macOS com Node 25.9. O build ainda informa
  bundle principal maior que 500 kB; isso não foi corrigido nesta entrega.
- `pnpm --filter @cli-master/desktop test:e2e`: **12 testes** passaram no
  Chromium/macOS, com build novo. Incluem ponteiro, teclado, persistência após
  reload, drafts por terminal e inserção da biblioteca offline, com janela
  compacta. Os testes anteriores também cobrem dimensões de 360/640/1440 px.
- `cargo test -p cli-master-core -p cli-master-storage -p cli-master-daemon`:
  **167 testes** passaram após integrar conhecimento; Clippy desses crates
  com todos os targets e `-D warnings` e formatação passaram. A integração da
  saga foi verificada antes com **119 testes** de session/storage. Não são
  contagens somáveis: há cobertura sobreposta e checkpoints diferentes.
- `cargo test -p cli-master-agents`: **62 testes** passaram no patch de probe;
  Clippy de todos os targets passou. Usam processos/arquivos reais, sem login
  em CLI de fornecedor. O harness de smoke do navegador tem **5 testes Node**.
- A build Rust usou `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
  CARGO_PROFILE_TEST_DEBUG=0` por espaço limitado. Apenas artefatos gerados
  desta worktree foram removidos quando o disco esgotou; fontes e dados do
  usuário foram preservados. Os artefatos são regeneráveis.
- Capturas desktop/compacta do canvas, composer e biblioteca foram
  inspecionadas. Asserções geométricas verificam largura, campos e ações
  acessíveis na janela compacta. A revisão UI/UX orientou foco, Escape, IME e
  preservação de rascunho; não estabelece fidelidade visual integral.
- E2E no navegador verifica cartões locais/drafts. Testes de componente
  verificam seleção de terminais com IPC do projeto simulado. Isso **não**
  prova PTY em janela Tauri ou login no Gemini.
- No commit `9e2a070`, [CI Linux/macOS](https://github.com/guicybercode/Jig/actions/runs/34003494040)
  e [Packaging Linux/macOS](https://github.com/guicybercode/Jig/actions/runs/34003494127)
  passaram. Em `dd84a49`, [Packaging](https://github.com/guicybercode/Jig/actions/runs/34004462439)
  passou nos dois sistemas; [CI](https://github.com/guicybercode/Jig/actions/runs/34004462407)
  falhou em dois fixtures AppShell sem o handler `listWorktrees`. O ajuste
  está em `1150de6`, validado localmente; conferir CI/Packaging do novo HEAD.
  Jobs antigos cancelados por pushes seguintes não são evidência de falha.
- Smoke interativo do navegador/PTY em pacotes Linux/macOS continua aberto,
  inclusive permissões de mídia, subframes, redirects e downloads. Compilar
  ou testar strings de proteção não demonstra toda a fronteira nativa.

M02 e M06 avançaram, mas ainda faltam copiar/colar, grupos persistidos,
alinhamento, undo/redo, busca global e atalhos configuráveis. M46 continua
parcial; esta revisão visual não prova fidelidade de todas as telas/estados.

M19 agora tem texto/draft/envio explícito funcional; anexos, menções vivas e
capacidades específicas dos agentes continuam pendentes. A biblioteca salva
em SQLite, mas edições ainda não salvas só sobrevivem a ocultar/reabrir e
trocar escopo enquanto o canvas está montado. Ir para Settings/Diagnostics ou
fechar o app pode descartá-las. O draft inserido no terminal usa a persistência
do canvas. Snapshots são texto, não referências vivas nem permissões de agente.

## Próximas integrações reais

1. Validar CI no HEAD publicado e smoke nativo nas duas plataformas. A base
   do navegador já está integrada; automação e portais completos continuam
   pendentes. Não reimplementar outro host nem promover M37 a concluído.
2. S2: integrar os próximos contratos publicados de arquivos/notas/workspace/
   floors. Isolamento por sessão já usa o caminho público; isso ainda não é
   o modelo de floor compartilhado por várias sessões.
3. S3: biblioteca e composer já estão ligados. Próximos incrementos são o
   inspetor de regras/skills e organização (pin/archive/workflow), com contratos
   e migrations coordenados. [Coordenação com S3](xirp-integration-request.md)
   registra hashes e acordos. Não confundir publicação na branch S3 com
   montagem/verificação no canvas desta branch.
4. Continuar as linhas obrigatórias da matriz, incluindo lift/dock, conteúdo,
   portais/dispositivos, automações, continuidade, ambientes e capacidades
   de plataforma. A entrega atual não reduz esse escopo.

As worktrees das sessões adicionais já existem. Os paths efetivamente
observados são `/Users/eguimacs/cli-master-runtime` e
`/Users/eguimacs/cli-master-xirp`; os nomes propostos no documento de goals
não autorizam recriá-las nem apagar seu trabalho.
