# Sessão 06 — experiência de projetos e sessões

## Resultado

A interface da Beta v0.1 agora cobre o fluxo frontend completo de cadastro de
projetos, navegação, criação e administração de sessões. A experiência é densa,
dark-first, responsiva e operável por teclado. Nenhum byte de terminal entra no
estado React; a área de terminal ficou apenas como ponto de integração para a
sessão responsável por xterm.js.

O frontend chama somente o cliente IPC tipado de produção. Fakes existem apenas
nos testes. A integração produtiva de cadastro/criação ainda depende do daemon e
da ponte Tauri implementarem os métodos v1 já previstos em `ARCHITECTURE.md`; o
estado atual do backend expõe somente `system.hello` e um `state.snapshot` vazio.

## Componentes

- `App` monta error boundary, provider de workspace e shell.
- `AppShell` compõe header contextual, project rail, painel de sessões, área
  principal, status bar, command palette e dialogs globais.
- `ProjectSidebar` apresenta projetos recentes, branch, indisponibilidade,
  renomeação, remoção segura e acessos a Settings/Diagnostics.
- `SessionSidebar` apresenta agente, status textual, branch, worktree, dirty
  state, última atividade, término de processo, falha de inicialização e
  worktrees retidos após a exclusão dos metadados da sessão.
- `NewSessionDialog` implementa Details → Review/Create, custom agent,
  subdiretório relativo, modo current/new worktree, preview autoritativo de
  branch, argumentos estruturados do agente customizado, consequências e
  bloqueio de submissão duplicada.
- `SessionWorkspace` oferece Open, Start, Stop Process, Restart, Rename, Copy
  Path, Open in System, Git Status, Delete Session e Remove Worktree.
- `Dialog`, `CommandPalette`, `StatusBadge`, `ErrorNotice` e `Icon` formam os
  primitives acessíveis e sem dependências adicionais.
- `DiagnosticsView`, grid e Settings completam os destinos globais.

## Decisões de estado e IPC

- `WorkspaceProvider` é o único dono do snapshot público e da seleção; forms e
  estados efêmeros permanecem locais aos dialogs.
- Projetos, agentes, sessões e worktrees vêm de um único snapshot autoritativo.
  Listas filtradas e entidades selecionadas são derivadas, não copiadas.
- Um único subscription router valida sequência e payloads. Eventos atrasados
  ou repetidos não sobrescrevem estado mais novo.
- Troca de projeto é síncrona sobre dados normalizados. Um revision guard impede
  que respostas de `project.add` ou `session.create` roubem a seleção depois de
  uma navegação rápida.
- `session.output` e `session.output_gap` são descartados antes do reducer. O
  futuro terminal controller recebe bytes imperativamente, fora de React.
- Todos os `invoke`/`listen` e o opener nativo estão centralizados em
  `src/ipc/client.ts`; os DTOs oficiais e decoders de runtime ficam em
  `src/ipc/types.ts` e `src/ipc/schema.ts`.
- O wire segue as formas Rust v1 atuais: IDs UUID, `AgentRecord` plano,
  detecção de executável separada, `GitTarget` tagged com IDs camelCase,
  worktree com state/timestamps e preparação de remoção `ready | blocked`.
- Remoção de worktree nunca oferece bypass para conteúdo dirty ou uso ativo.
  Worktrees retidos continuam acessíveis no painel mesmo após `session.delete`.
- Erros preservam `code`, `message`, `action` e detalhes seguros do daemon.
  Incompatibilidade de contrato vira erro fatal; desconexão oferece retry.

## Atalhos

| Ação | macOS | Linux |
|---|---|---|
| Command palette | `Cmd+K` | `Ctrl+K` |
| Nova sessão | `Cmd+T` | `Ctrl+T` |
| Grid | `Cmd+Shift+G` | `Ctrl+Shift+G` |
| Foco da sessão | `Cmd+1..9` | `Ctrl+1..9` |

O listener ignora IME composition, eventos já tratados, repeat e qualquer
atalho originado dentro de `[data-terminal-root]`. Palette, menus e atalhos usam
o mesmo command registry para manter labels, disponibilidade e ações alinhados.

## Acessibilidade e layout

- foco visível de 2px, skip link e ordem DOM compatível com a ordem visual;
- dialogs nativos com fallback, contenção/restauração de foco e Escape;
- destructive confirmations começam com foco em Cancel e não confirmam por
  Enter implícito;
- labels visíveis, hints persistentes, erros inline e foco no primeiro campo
  inválido;
- status combina indicador, texto e cor; output de terminal não usa live region;
- navegação modal off-canvas abaixo de 768px, com focus trap, Escape,
  restauração de foco e fechamento automático ao ampliar a janela;
- três painéis em janelas largas;
- motion mínimo e removido com `prefers-reduced-motion`.

## Testes e gates

Cobertura frontend adicionada para:

- adicionar e trocar projeto;
- abrir o modal de sessão, validação e submissão única;
- todos os status, processo encerrado e erro de inicialização;
- command palette e atalhos completos em macOS/Linux, inclusive exclusão do
  terminal;
- erro IPC acionável, daemon desconectado e retry;
- roteamento que não atualiza React para output do terminal;
- distinção explícita entre Stop Process, Delete Session e Remove Worktree;
- remoção segura `ready | blocked` e cleanup de worktree retido;
- fixtures/decoders e transporte golden de request/event do contrato IPC.

Gates executados ao final desta sessão:

```text
pnpm lint
pnpm --filter @cli-master/desktop test
pnpm --filter @cli-master/desktop build
pnpm --filter @cli-master/desktop check
```

O gate desta entrega executou 26 testes em 4 arquivos, seguido do build Vite de
produção.

## Limitações conhecidas

1. A ponte Tauri atual ainda expõe somente o comando de scaffold `greet`; não
   existe `daemon_request` nem relay de eventos.
2. O daemon atual aceita apenas `system.hello` e `state.snapshot`, e o snapshot
   retorna coleções vazias. Métodos `project.*`, `session.*`, `agent.*`,
   `git.*`, `worktree.*` e `diagnostics.*` ainda retornam `method_not_found`.
3. Disponibilidade/movimentação do projeto ainda não faz parte de `Project` no
   wire Rust. O frontend aceita apenas `availability` e
   `availabilityMessage` como extensão opcional; detecção de agentes,
   `lastActivityAtMs`/`errorCode` e state/timestamps de worktree já seguem os
   DTOs oficiais.
4. Não há picker nativo de diretório; o fluxo aceita colar/digitar um path
   absoluto e delega a validação real ao daemon antes de salvar.
5. A área de terminal e o grid reservam somente os hosts de integração. xterm,
   PTY replay e resizing pertencem à sessão específica de terminais.
6. `session.start` possui request oficial, mas ainda não é despachado pelo
   daemon atual. Base branch customizada e argumentos adicionais por sessão não
   pertencem ao `SessionCreateRequest` v1; a UI mostra previews derivados pelo
   daemon e oferece argumentos somente na definição shell-free de agente
   customizado.
7. “Open in System” usa a capability existente do opener. Abrir diretamente um
   terminal gráfico exige uma capability específica por plataforma e permanece
   indisponível.
8. `ARCHITECTURE.md` ainda descreve um `allowDirty` legado para remoção de
   worktree. O contrato Rust e esta UI rejeitam explicitamente bypasses
   `dirty`/`force`; essa documentação arquitetural deve ser corrigida em uma
   sessão de backend/documentação.

Essas limitações não são mascaradas com mocks ou persistência no browser. Até a
ponte/daemon aterrissarem, o aplicativo mostra erros acionáveis e mantém a
experiência testável pela interface tipada injetada nos testes.
