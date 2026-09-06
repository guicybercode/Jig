# Goals para duas sessões adicionais

Copie um dos blocos abaixo para cada nova sessão. A sessão atual (S1) mantém
o canvas, a aparência Maestri e a integração final. Consulte a
[matriz de paridade](../maestri-xirp-parity.md) para o inventário completo,
fontes, baseline e critérios de conclusão. Os goals não reduzem o objetivo ao
primeiro incremento entregue.

## Organização antes de iniciar

Cada sessão trabalha em **branch e worktree próprios**. A pasta
`/Users/eguimacs/cli-master` pertence à sessão principal; abrir três sessões
no mesmo diretório compartilha arquivos e índice Git e não isola commits.

| Sessão | Branch proposta | Worktree proposta | Responsabilidade |
| --- | --- | --- | --- |
| S1 | `refactor/canvas-only-shell` | `/Users/eguimacs/cli-master` | Canvas, shell, UI final, bridge Tauri e integração serial |
| S2 | `feat/maestri-runtime` | `/Users/eguimacs/cli-master-runtime` | Domínio/IPC, SQLite, Git, sessões, automações, ambientes |
| S3 | `feat/xirp-local-workflows` | `/Users/eguimacs/cli-master-xirp-workflows` | Prompts, regras/skills, pin/arquivo/workflow e contexto |

Os nomes são propostos, não evidência de worktrees já criados. A sessão que
iniciar verifica `git status`, `git branch --show-current`,
`git worktree list` e o remote. Se nome/diretório já existir, inspeciona e
reutiliza quando for dela; não apaga nem sobrescreve trabalho existente.
Após fetch, cria a worktree a partir do commit mais recente de
`origin/refactor/canvas-only-shell`. Não presume que `origin/main` contenha
o canvas atual. Integra esta documentação quando estiver publicada na origem.

### Contratos e arquivos compartilhados

S2 coordena as alterações de `crates/core/src/wire`, `protocol/catalog.json`,
`apps/desktop/src/ipc`, migrações/registro de migrações e dispatch do daemon.
S3 propõe os tipos/operações do seu domínio e recebe um commit de contrato
que pode integrar à sua branch. Métodos somente entram no catálogo público
junto de implementação funcional, ou por um mecanismo explícito de
capacidades documentado em ADR. Um handler fictício não fecha a integração.

S1 coordena mudanças em `AppShell`, estado global, navegação, paleta, canvas,
estilos compartilhados, dependências frontend e Tauri. S3 entrega componentes
isolados com callbacks tipados; S1 os encaixa no canvas. Integração de UI real
continua sendo requisito de conclusão, mesmo quando pertence a outra sessão.

Antes de cruzar uma fronteira, publicar no relatório da sessão: arquivo,
contrato proposto, erro/evento, persistência, dependência e commit. Arquivos
novos de S3 preferem os namespaces `workflows` e `knowledge`;
`crates/core/src/catalog.rs` já existe para outro propósito.
`lib.rs`, manifests e registros são compartilhados: combinar um
incremento curto com S2, em vez de dois conjuntos concorrentes de mudanças.

### Commits, push e integração

1. Fazer um commit pequeno por mudança coerente e verificada. Selecionar os
   arquivos explicitamente; não incluir edits de outras sessões.
2. Executar testes proporcionais antes do commit e registrar o resultado.
   Mudança de contrato inclui teste de espelhos e round-trip pelo daemon.
3. Fazer push da própria branch após cada commit pequeno validado. No primeiro
   envio, usar `git push -u origin feat/maestri-runtime` ou
   `git push -u origin feat/xirp-local-workflows`, conforme a sessão.
4. Usar a identidade Git já configurada pelo usuário. Não adicionar
   `Co-authored-by` do Codex/OpenAI nem alterar autoria para a ferramenta.
   Conferir `git show -s --format=full HEAD` antes do push.
5. S1 integra os commits publicados serialmente, testa o estado combinado e
   faz push da branch de integração. Worktrees não compartilham índice,
   mas ainda podem mudar os mesmos arquivos; resolver conflitos pelo contrato,
   nunca escolhendo um lado inteiro sem análise.
6. Push rejeitado exige fetch e integração normal; não usar force push nem
   reescrever commits publicados. Não remover worktrees automaticamente.

O repo roda CI em push para `main` e em pull requests no baseline. Para uma
branch de trabalho receber a matriz de CI, abrir um draft PR quando possível;
push sozinho não prova CI executada. S1 coordena a integração dos PRs.

## Goal da sessão 2 — backend Maestri

```text
Trabalhe no projeto Jig/CLI Master para entregar o backend completo das
capacidades Maestri da matriz docs/maestri-xirp-parity.md, preservando Linux e
macOS e o frontend de canvas que a sessão 1 está construindo. O objetivo é
funcionamento real de ponta a ponta, não apenas tipos, endpoints ou mocks.
Mantenha este goal ativo até cumprir o escopo e verificar seus fluxos.

Leia AGENTS.md, ARCHITECTURE.md, os ADRs e docs/codex/parallel-goals.md.
Confirme o estado atual: o inventário de 2026-09-05 é um ponto de partida.
Você não está sozinho no repositório: não reverta edições de outras sessões.
Use branch feat/maestri-runtime, criada do último commit de
origin/refactor/canvas-only-shell, e worktree
/Users/eguimacs/cli-master-runtime, após verificar se já existem. Não trabalhe
no índice Git da sessão principal e não apague worktrees de outras sessões.

Seu domínio: crates/core (sem I/O), crates/storage, crates/agents, crates/git,
crates/session, crates/daemon, testes backend/fake-agent/e2e e documentação
backend nova. Coordene catálogo IPC, mirrors TypeScript e protocolo com S1/S3.
Reserve módulos novos workflows/knowledge e metadados de workflow para S3.
Tauri e componentes React são de
S1; entregue contratos/eventos e casos de aceitação para essa integração.

Comece pelas falhas reais de integração: session.create hoje recusa
new_worktree; state.snapshot devolve worktrees vazios; existem métodos
anunciados sem dispatch. Ligue a saga existente respeitando create como
preparação de metadados e start como início do processo. A saga atual cria e
inicia, portanto não basta chamá-la diretamente do handler create. Preserve
relative_directory e rollback, recuperação e tokens de remoção. Consolide
o registry de agentes em vez de manter seeds divergentes no daemon.

Depois execute o escopo backend de M04–M09, M12–M15, M17–M28, M29–M45,
M47–M48 e as dependências backend X01–X10. Implemente em incrementos verticais,
seguindo as prioridades da matriz, sem abandonar os itens posteriores:
- notas e documentos duráveis, grafo e permissões;
- arquivos e Git completos com segurança e estados de erro;
- floors/worktrees, landing, hooks e compartilhamento entre sessões;
- roles, entrega de prompts/anexos, comunicação CLI e recrutamento;
- rotinas e histórico, preferências, backup/restore e recuperação;
- retomada de conversa verificada por adaptador e persistência tmux;
- ambientes remotos, portais/dispositivos e controle remoto por contratos;
- contratos para prompts, regras/skills, arquivo e contexto da sessão 3.

SessionManager é o único dono de PTY, spawn de agentes, transições e sinais.
Use CommandSpec com executável+argv+cwd+env; não use sh -c nem shell
interpolado. Hooks não podem criar um executor de agentes paralelo.
SQLite pertence a storage, tipos puros a core. Cada método nasce no contrato
Rust e atualiza protocol/catalog.json, methods.ts e domain.ts em sincronia.
Não adicione comandos Tauri de domínio. Timestamps continuam epoch-ms.

Mantenha lifecycle do processo separado de workflow do trabalho e de atenção.
S3 é dona do workflow/arquivo/pin; combine os contratos de metadados com ela.
Um silêncio no terminal não é prova de pedido de aprovação. Não retome ou
sinalize PID persistido de outro daemon. Remover metadados não remove diretório
do projeto/worktree; remoção de worktree usa preparo+token vinculado ao estado.

Decisões cross-cutting exigem ADR: persistência e revisão de documentos,
execução de hooks, floor com várias sessões, exposição remota e inferência
local opcional. Preserve o produto local-first e as capacidades Linux/macOS.
Documente limites reais de hardware/plataforma e alternativas; não declare
equivalência por desabilitar um recurso. Não proxyar tráfego dos agentes.

Cada incremento inclui teste com diretórios temporários reais, SQLite real,
Git real e programas curtos. Prove o caminho pelo socket do daemon e o efeito
observável, incluindo falhas, concorrência e restart. Rode gates do README
antes da integração; confirme CI Linux/macOS e resultados do runtime. Registre
o que não pôde ser testado, sem chamar isso de aprovado.

Faça commit e push de cada pequena alteração coerente e validada na sua
branch. Use o autor Git do usuário, sem Codex/OpenAI como Co-authored-by.
Não force push. Publique um draft PR para obter CI quando aplicável. Mantenha
docs/codex/maestri-runtime-report.md com IDs atendidos, contratos, commits,
testes e dependências de S1/S3. A conclusão exige integração consumida pela UI,
evidência de Linux/macOS e nenhum item obrigatório backend deixado como stub.
```

## Goal da sessão 3 — fluxos locais inspirados em XIRP

```text
Trabalhe no projeto Jig/CLI Master para entregar os recursos locais do XIRP
selecionados em docs/maestri-xirp-parity.md, mantendo Linux/macOS e o canvas
Maestri como superfície principal. O escopo central é X06, X09, organização
de X01, regras de M05/M15 e prompts/arquivo/workflow de X07. Entregue
funcionalidades operantes e persistentes. Não conclua o goal com protótipos
ou dados estáticos.

Leia AGENTS.md, ARCHITECTURE.md, ADRs e docs/codex/parallel-goals.md.
Confira as fontes oficiais citadas na matriz: Portal é opcional no XIRP.
Catálogo empresarial, Scaffolder e Insights do Portal são propostas opcionais
O01–O03, fora deste goal. Não os use para substituir os recursos locais pedidos.

Você não está sozinho no repositório: não reverta o trabalho de outras
sessões. Use branch feat/xirp-local-workflows, criada do último commit de
origin/refactor/canvas-only-shell, e worktree
/Users/eguimacs/cli-master-xirp-workflows, verificando primeiro se já existem.
Não faça commits no índice Git da sessão principal.

Seu ownership preferencial:
- novos módulos workflows/knowledge em crates/core/src/ (tipos puros);
- novos módulos workflows/knowledge em crates/storage/src/ (SQL);
- novos módulos workflows/knowledge em crates/daemon/src/ (handlers);
- novos testes workflows/knowledge em cada crate relevante;
- apps/desktop/src/app/features/workflows/ e knowledge/, com testes próprios;
- docs/codex/xirp-workflows-report.md e documentação do domínio novo.

Não reutilize crates/core/src/catalog.rs para outro catálogo. S2 coordena
wire, mirrors, dispatch, lib.rs, manifests, migrações e executor de processos.
S1 coordena AppShell, estado global, navegação, canvas, estilos globais e Tauri.
Combine contratos pequenos primeiro e integre os commits correspondentes;
entregue componentes isolados e callbacks tipados para S1. Não crie um segundo
banco, catálogo IPC, cliente Tauri ou proprietário de sessão por conveniência.

Entregue, nesta ordem, sem limitar o objetivo à primeira etapa:
1. Prompts salvos globais/de projeto: criar, editar, excluir e buscar;
   selecionar insere rascunho no composer, sem envio oculto. Conteúdo persiste
   no armazenamento oficial e sobrevive ao reinício.
2. Descoberta local de regras/skills e contexto de projeto: origem, escopo,
   precedência, leitura segura, consulta e edição explícita com conflito.
   Arquivos de credenciais ficam fora da indexação/edição. Trate symlinks,
   limites, encoding e mudanças externas. Importar pasta pai não transforma
   todas as pastas automaticamente em repositórios.
3. Organização persistida de projetos/sessões: pin, arquivo/restauração,
   busca, filtros e agrupamento. Arquivar metadados não encerra processo nem
   remove diretório. Registre workflow backlog/in_progress/in_review/done/
   blocked em campo próprio; não o derive nem misture com SessionStatus.
4. Coleções de contexto com documentos, referências, decisões e wiki local;
   ligação a notas reais e projeto; busca e exportação explícita. Exponha ações
   para abrir projeto ou iniciar sessão pela API oficial da S2.
5. Integre esses recursos ao canvas e à paleta com a S1. Componentes próprios
   precisam de loading, vazio, erro e teclado. Não mexa no canvas, shell ou
   estilos globais em paralelo; forneça callbacks e contratos pequenos.
6. Registre extensões posteriores documentadas no XIRP, como uso/custo e
   contexto MCP. Métricas precisam de fonte nativa verificável; não infira
   tokens/custo/atividade cognitiva de bytes PTY ou tempo de processo.
   Não dependa de conta Portal para concluir os fluxos locais deste goal.

Não ligue contas Spotify/GitHub, não publique transcripts, não instale serviços
pagos e não envie telemetria por padrão. Conectores externos são uma etapa
explícita separada, com contrato/conta/escopos disponíveis e preview do envio.
O frontend continua visualmente Maestri; não substitua o canvas por um portal
administrativo. Siga design tokens e componentes existentes.

Escreva ADR para separação workflow/processo, escopos de prompts e persistência
do contexto. Teste parser/validação contra fixtures reais, SQLite/restart,
arquivamento/restauração, leitura insegura e conflitos de edição. Frontend
testa o cliente IPC do projeto. Não basta teste com mock para declarar fluxo
persistido; prove daemon+efeito no disco e UI. SessionManager continua o único
proprietário de processos; nenhum destes módulos pode abrir PTY ou dar spawn.

Faça commits pequenos e push após cada alteração coerente validada na sua
branch, com autor Git do usuário e sem Codex/OpenAI como Co-authored-by.
Não force push. Abra draft PR para CI quando aplicável. Atualize
docs/codex/xirp-workflows-report.md com IDs, contratos, commits, testes,
dependências e limitações. Mantenha o goal até os recursos selecionados estarem
integrados ao canvas, persistirem e passarem pelas verificações Linux/macOS.
```

## Resultado esperado das três sessões

Um produto integrado, com evolução registrada na matriz, commits pequenos
publicados e evidência de runtime. Os relatórios de S2/S3 devem permitir que S1
integre sem adivinhar contratos ou reinterpretar sucesso. Documentação,
roadmap e branches publicadas são entregáveis intermediários; paridade
funcional e visual verificada continua sendo o objetivo final.
