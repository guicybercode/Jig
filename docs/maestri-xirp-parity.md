# Paridade Maestri e referências XIRP

Levantamento em **2026-09-05**, sobre o commit `0ab3505`. Este é um inventário
de trabalho, não uma declaração de paridade. O objetivo permanece: experiência
de canvas do Maestri, suas capacidades funcionais e recursos escolhidos do
XIRP, com Linux e macOS como plataformas de primeira classe. Commits posteriores
ao baseline devem ser auditados antes de atualizar os estados desta matriz.

O [changelog do Maestri][m-changelog] foi percorrido de **0.13
(2026-03-18) a 0.45.3 (2026-09-04)**, inclusive point releases. As tabelas
agrupam resultados funcionais por domínio; correções repetidas viram critérios
de regressão. As páginas oficiais por domínio complementam o changelog.
Documentação viva pode conter comportamento posterior a uma versão: não
inferir a versão de introdução quando ela não estiver publicada.

O [changelog do XIRP][x-changelog] chega a **0.25.0 (2026-09-03)**. O overview
está menos atualizado sobre agentes que o changelog. Catálogo, Scaffolder e
Insights são capacidades do **Spotify Portal**, não recursos locais que se
possa atribuir automaticamente ao XIRP. Adaptações desses recursos aparecem
apenas como propostas opcionais, fora das entregas selecionadas.
[XIRP e contexto opcional][x-intro]

## Como usar o inventário

- **I — implementado no caminho indicado:** código executável e testes
  relacionados foram localizados. Não equivale a validação visual ou do pacote.
- **P — parcial:** parte existe; a coluna de evidência explica a lacuna.
- **A — ausente:** sem implementação encontrada nos contratos, handlers,
  modelos, migrações e telas examinados.
- **L — limite de plataforma:** exige alternativa ou capacidade condicional;
  continua no inventário e não conta como concluído.
- **P0/P1/P2/P3** indicam ordem de entrega, não exclusão do escopo.
- **S1**: canvas/frontend e integração final; **S2**: domínio/backend Maestri;
  **S3**: prompts, regras, skills, workflow, arquivo e contexto local do XIRP.

Estados refletem o baseline acima. Antes de iniciar uma linha, conferir o
código atual e atualizar a evidência. Não promover uma linha por existir um
botão, tipo, mock, método anunciado ou biblioteca sem ligação ao fluxo real.
Esta inspeção não executou a suíte nem validou pacotes nas duas plataformas.

## Evidências locais de referência

| Código | Fonte no repositório | O que a inspeção permite afirmar |
| --- | --- | --- |
| E1 | [canvas-state.ts](../apps/desktop/src/app/features/canvas/canvas-state.ts), [CanvasWorkspace.tsx](../apps/desktop/src/app/features/canvas/CanvasWorkspace.tsx), [useCanvasState.ts](../apps/desktop/src/app/features/canvas/useCanvasState.ts) | Canvas contém terminais e notas simples; documento em `localStorage`, seleção única, limites finitos de posição e zoom. |
| E2 | [CanvasConnections.tsx](../apps/desktop/src/app/features/canvas/CanvasConnections.tsx) | Curvas SVG persistidas no documento local; não autorizam comunicação entre agentes. |
| E3 | [sessions.rs](../crates/daemon/src/sessions.rs), [server.rs](../crates/daemon/src/server.rs), [LiveTerminal.tsx](../apps/desktop/src/app/features/terminal/LiveTerminal.tsx) | PTY local ligado ao daemon. `session.create` recusa isolamento diferente de `current`; snapshot devolve worktrees vazios. |
| E4 | [saga.rs](../crates/session/src/saga.rs), [create.rs](../crates/session/src/create.rs), [remove.rs](../crates/session/src/remove.rs), [testes da saga](../crates/session/tests/create_saga.rs) | Saga e segurança de worktrees existem, mas o caminho do daemon não usa a saga. A criação da saga também inicia processo, diferentemente do contrato `create` seguido de `start`. |
| E5 | [method.rs](../crates/core/src/wire/method.rs), [request.rs](../crates/core/src/wire/request.rs), [catalog.json](../protocol/catalog.json), [dispatcher](../crates/daemon/src/server.rs) | Catálogo v1 tem projetos, agentes, sessões, status/diff Git, remoção de worktree e diagnóstico. Nem todo nome anunciado tem handler. |
| E6 | [projects.rs](../crates/daemon/src/projects.rs), [model.rs](../crates/core/src/model.rs), [repository.rs](../crates/git/src/repository.rs) | Projetos locais e raiz Git opcional. Não há floors, pastas de workspaces, catálogo de software, roles ou rotinas. |
| E7 | [builtins.rs](../crates/agents/src/builtins.rs), [registry.rs](../crates/agents/src/registry.rs), [seed do daemon](../crates/daemon/src/sessions.rs) | Adaptadores incluem Gemini; daemon mantém seed próprio Shell/Codex/Claude/OpenCode. Não confundir adaptador disponível com opção funcional na aplicação. |
| E8 | [git_inspection.rs](../crates/daemon/src/git_inspection.rs), [Git](../crates/git/src/lib.rs) | Status/diff são operações de inspeção; não há fluxo completo de commit, push, branches, stash e landing. |
| E9 | [CommandPalette.tsx](../apps/desktop/src/app/features/commands/CommandPalette.tsx), [useGlobalShortcuts.ts](../apps/desktop/src/app/hooks/useGlobalShortcuts.ts), [CanvasSidebar.tsx](../apps/desktop/src/app/features/navigation/CanvasSidebar.tsx) | Navegação, paleta e rail existem; não têm o conjunto de indexadores e preferências das referências. |
| E10 | [TerminalSurface.tsx](../apps/desktop/src/app/features/terminal/TerminalSurface.tsx), [terminal-runtime.ts](../apps/desktop/src/app/features/terminal/terminal-runtime.ts), [replay.rs](../crates/session/src/replay.rs) | xterm recebe fluxo/replay sem guardar bytes em estado React. Busca, configuração e comportamento internacional precisam de verificação específica. |
| E11 | [migrações](../crates/storage/migrations), [recovery.rs](../crates/storage/src/recovery.rs), [KNOWN_ISSUES.md](KNOWN_ISSUES.md), [backup-and-recovery.md](backup-and-recovery.md) | SQLite e recuperação conservadora existem; reiniciar daemon perde handles PTY. Procedimento de backup não é gerenciador de snapshots na UI. |
| E12 | [CI](../.github/workflows/ci.yml), [packaging](../.github/workflows/packaging.yml), [Tauri](../apps/desktop/src-tauri), [aceitação runtime](../crates/e2e/tests/acceptance.rs) | Jobs Linux/macOS e empacotamento declarados. Estado verde da execução e smoke tests dos artefatos ainda precisam ser registrados. |

## Maestri: canvas e conteúdo

Os requisitos resumidos nesta tabela vêm das páginas oficiais indicadas.
Critérios de execução e prioridades são propostas para Jig.

| ID | Capacidade / fonte | Estado e evidência | Próximo resultado verificável | Dono / prioridade |
| --- | --- | --- | --- | --- |
| M01 | Canvas espacial, inserir, mover, redimensionar, pan/zoom, minimapa. [Canvas][m-canvas] | P — E1, E10; espaço limitado e poucos tipos. | Navegar por uma composição grande com mouse/trackpad, sem misturar scroll do terminal e câmera. | S1 / P0 |
| M02 | Seleção múltipla, duplicação, copiar/colar, grupos, alinhamento, distribuição e tidy. [Canvas][m-canvas] | A/P — E1 só seleciona um nó. | Transformações mantêm relações e agrupamento, com undo/redo por workspace. | S1 / P1 |
| M03 | Lift, dock lateral, foco e snapping. [Canvas][m-canvas] | A — E1. | Painel destacado continua interativo enquanto a câmera se move; redimensionamento chega à PTY. | S1 / P1 |
| M04 | Workspaces com identidade, diretório, organização em pastas/grupos, rail, alternância e IDE. [Workspaces][m-workspaces] | P — E6, E9; registro de projetos e rail, sem modelo completo. | Configuração persiste; terminais herdados resolvem diretório correto e overrides permanecem explícitos. | S2 contrato + S1 / P1 |
| M05 | Regras do projeto, edição/sincronização de arquivos de instruções e importação de workspace. [Workspaces][m-workspaces] | A — E5/E6. | Revisão antes de importar/escrever; conflitos externos não apagam arquivos do usuário. | S3 regras; S2 importação; S1 / P1 |
| M06 | Paleta para elementos, ações, arquivos e atenção; atalhos configuráveis. [Batuta][m-batuta], [Atalhos][m-shortcuts] | P — E9. | Busca com escopo por projeto e global, navegação por teclado e remapeamento sem colisões silenciosas. | S1 / P1 |
| M07 | Notas Markdown com fonte/preview, imagens, nomes estáveis e arquivos externos. [Notas][m-notes] | P — E1: texto simples em armazenamento do navegador. | Arquivos duráveis, escrita atômica, detecção de edição externa e preview seguro; remover nó externo preserva arquivo. | S2 + S1 / P0 |
| M08 | Notas encadeadas e leitura/escrita por agente. [Notas][m-notes], [Conexões][m-connections] | P visual / A funcional — E1/E2. | Agente autorizado vê conteúdo atualizado; ciclos e limites de leitura tratados pelo daemon. | S2 + S1 / P1 |
| M09 | Fichários com páginas, reordenação, extração, identidade e acesso conjunto pelo agente. [Fichários][m-ficharios] | A — E1/E5. | Uma nota pertence a um fichário; mover páginas não perde conteúdo ou relações. | S2 + S1 / P2 |
| M10 | Texto, desenho, formas, pincel, reconhecimento de traços, preenchimento e cantos. [Changelog][m-changelog] | A — E1. | Ferramentas reais com seleção, edição e persistência; política de undo consistente. | S1 / P2 |
| M11 | Bloqueio de notas/páginas, blur, padrões de elementos, temas/fontes, materiais e fundo. [Changelog][m-changelog] | A/P — E1/E10; estilos fixos não são preferências. | Preferências propagam entre projetos; bloqueio é respeitado por UI e operações de agente. | S1 + S2 / P2 |
| M12 | Partituras: salvar composição, biblioteca, escopo, preview, import/export e conflitos de roles. [Partituras][m-partituras] | A — E1/E5. | Arquivo versionado recria composição; revisão mostra comandos; omite estado de execução e dados privados por padrão. | S2 + S1 / P2 |
| M13 | Arquivos/imagens/PDF/vídeo no canvas e clipboard entre aplicações. [Árvore de arquivos][m-files], [Changelog][m-changelog] | A — E1. | Preview funciona nos pacotes Linux/macOS; abrir/remover um nó não destrói o original. | S1 + S2 / P2 |

## Maestri: agentes, terminal e execução

| ID | Capacidade / fonte | Estado e evidência | Próximo resultado verificável | Dono / prioridade |
| --- | --- | --- | --- | --- |
| M14 | Terminais interativos, nomes, presets, comandos customizados e diretório por terminal. [Terminais][m-terminals] | I/P — E3/E7/E10; capacidade local implementada, configuração incompleta. | Executar todos os presets apresentados na UI e custom argv sem shell interpolado. | S2 + S1 / P0 |
| M15 | Roles reutilizáveis, instruções por agente, descoberta/importação e badges. [Terminais][m-terminals] | A — E5/E7. | Provisionar sem sobrescrever regras existentes; arquivo inseguro é rejeitado; role acompanha identidade e ambiente. | S2 + S3 descoberta + S1 / P1 |
| M16 | Temas de terminal, aparência automática, importação e preferências de fonte. [Terminais][m-terminals] | P — E10, sem biblioteca/preferências completas. | Tema muda em terminais ativos/inativos e permanece após reload; seleção continua legível. | S1 / P1 |
| M17 | Atenção, notificações e navegação até agente que aguarda resposta. [Terminais][m-terminals] | P — E3/E9: lifecycle não prova solicitação de autorização. | Estado de atenção vem de sinal verificável; clicar na notificação revela sessão correta. | S2 sinais + S1 / P1 |
| M18 | Busca, Smart Copy, scroll lock, anexos, IME, RTL e teclado internacional. [Atalhos][m-shortcuts], [Changelog][m-changelog] | P — E10; PTY básica, sem evidência do conjunto. | Testar streaming com seleção ativa, busca, paste multilinha e composição sem envio acidental. | S1 + S2 anexos / P1 |
| M19 | Composer com drafts por terminal, menções, imagens/arquivos e passagem de teclas. [Composer][m-composer] | A — E1/E5. | Enviar uma vez; destino vê bytes/arquivos corretos; rascunho sobrevive à alternância de projeto/floor. | S1 + S2 / P1 |
| M20 | Conexões com estilos, inspeção, navegação entre floors e cable ties. [Conexões][m-connections] | P — E2: desenho e relações locais apenas. | Relação funcional sobrevive a reload; edição visual não altera permissões implicitamente. | S1 + S2 / P1 |
| M21 | Pedidos entre agentes, respostas, notas/portais encadeados e ferramentas de CLI. [Conexões][m-connections] | A — E5. | Dois agentes diferentes trocam pedido/resposta com correlação, prazo e cancelamento; negar recurso não conectado. | S2 / P1 |
| M22 | Maestro: recrutar, trocar role, conectar, dispensar e criar workspaces/floors. [Maestro][m-maestro] | A — E5. | Comandos de um agente autorizado usam o mesmo domínio e SessionManager; agente comum não ganha acesso gerencial. | S2 + S1 / P2 |
| M23 | Rotinas agendadas, sequência de prompts, pause/resume, execução manual e histórico. [Rotinas][m-routines] | A — E5/E11. | Scheduler durável trata reinício, sobreposição e alvo indisponível; cada disparo tem resultado rastreável. | S2 + S1 / P2 |
| M24 | Rotinas com preparação, skip ocupado, execução única e CLI. [Changelog][m-changelog] | A — E5. | Separar execução de programa de envio de prompt; registrar skip/erro sem forjar conclusão. | S2 + S1 / P2 |
| M25 | Sessões persistentes local/tmux e ambientes remotos; reconectar sem duplicar. [Ambientes][m-environments] | P — E3/E11: fechar janela preserva daemon; reiniciar daemon não reanexa PTY. | Distinguir reconexão do cliente, reinício do daemon e retomada nativa de conversa; testar cada caso. | S2 / P1 |
| M26 | Restaurar conversa de agentes que suportam resume; unload/reload e recuperação. [Changelog][m-changelog] | A/P — E11. | Persistir identificador nativo verificado por adaptador; reinício não cria conversa inesperada nem sinaliza PID antigo. | S2 / P1 |
| M27 | SSH, Docker, Sandbox e runtime custom; herança/override e arquivos remotos. [Ambientes][m-environments] | A — E3/E5. | Executável+argv, resolução de cwd no host correto, uploads e ferramentas no mesmo ambiente. | S2 + S1 / P3 |
| M28 | Descoberta de hosts/conexões, tmux remoto e provisionamento de roles/skills. [Ambientes][m-environments] | A — E5/E7. | Preservar configuração SSH do usuário; falhas de conexão/reload têm estados explícitos e cleanup limitado ao que Jig criou. | S2 + S1 / P3 |

## Maestri: arquivos, Git e floors

| ID | Capacidade / fonte | Estado e evidência | Próximo resultado verificável | Dono / prioridade |
| --- | --- | --- | --- | --- |
| M29 | File tree: lista/ícones, navegação, pesquisa, preview e gerenciamento de arquivos. [Arquivos][m-files] | A — E1/E5. | Árvore real com operações limitadas à raiz autorizada, incluindo symlinks e nomes não UTF-8. | S2 + S1 / P1 |
| M30 | Editor: sintaxe, find/replace, multicursor, indentação, linhas e envio de trecho. [Arquivos][m-files] | A — E1/E5. | Editar/salvar no disco com detecção de conflito, limite de arquivo e proteção de alterações não salvas. | S2 I/O + S1 / P1 |
| M31 | Tabs, definições, busca em conteúdo, pinning, Markdown e configuração do editor. [Changelog][m-changelog] | A — E1/E5. | Busca cancelável e paginada, resultados abrem linha correta; defaults e tabs restaurados. | S1 + S2 / P2 |
| M32 | Status/diff, contexto Git, stage/unstage/discard e commit. [Arquivos][m-files] | P — E8: status/diff backend; ações de escrita ausentes. | UI usa handlers reais; validar escopo do nó e confirmar descarte com estado atualizado. | S2 + S1 / P0 |
| M33 | Histórico/gráfico, fetch/pull/push, branches, merge e stash. [Arquivos][m-files] | A — E8. | Git real com erro acionável, cancelamento e exclusão mútua; credenciais seguem Git do usuário. | S2 + S1 / P1 |
| M34 | Floors: canvas isolado, branch, clonar layout, visão geral e diretórios corretos. [Floors][m-floors] | P infraestrutura / A produto — E4/E6. | Primeiro ligar worktree ao daemon sem iniciar processo em `session.create`; depois modelo floor para várias sessões. | S2 + S1 / P0/P1 |
| M35 | Landing com preview, conflitos, destino e remoção segura. [Floors][m-floors] | P infraestrutura — E4; landing ausente. | Provar branch/CWD reais; dirty e divergência impedem perda; token de remoção invalida se o estado muda. | S2 + S1 / P1 |
| M36 | Hooks setup/run/teardown ordenados e contexto do floor. [Floors][m-floors] | A — E5. | Cada comando estruturado passa pelo proprietário da execução, com timeout, resultado e rollback documentado. | S2 + S1 / P2 |

## Maestri: portais, compartilhamento e plataforma

| ID | Capacidade / fonte | Estado e evidência | Próximo resultado verificável | Dono / prioridade |
| --- | --- | --- | --- | --- |
| M37 | Navegador embutido, navegação, armazenamento e automação via conexão. [Portais][m-portals] | A — E1/E5. | Nó contém navegador real; app e conteúdo web mantêm fronteira de privilégios; UI e agente veem a mesma página. | S1 bridge + S2 autorização / P3 |
| M38 | Navegador: screenshots, DOM, console, JavaScript, cliques e formulários. [Portais][m-portals] | A — E5. | Contrato de automação com erros e alvo explícitos; revogar conexão revoga ferramentas. | S2 + S1 bridge / P3 |
| M39 | Portais: viewport/user-agent, devtools, downloads, mídia, mute, retry e certificados locais. [Changelog][m-changelog] | A — E5. | Fluxos nativos testados nos dois sistemas; autorização de mídia/certificado por origem, nunca global. | S1 bridge + S2 / P3 |
| M40 | Android/emulador, iOS Simulator, gestos, árvore de elementos e lifecycle de app. [Portais][m-portals] | A/L — E5/E12. | Android local nos dois sistemas; iOS condicionado a host macOS/Xcode. Provar interação além de imagem estática. | S2 runtime + S1 / P3 |
| M41 | Wire: pareamento, funções, revogação, capacidades, feed e fluxo de terminal. [Wire][m-wire] | A — E5: Unix IPC local não implementa Wire. | ADR de transporte separado, TLS e validação de origem; leitura/escrita autorizadas; eventos e reconexão reais. | S2 + S1 / P3 |
| M42 | Controle remoto de canvas, notas, floors/hooks, rotinas, arquivos e Git. [Wire][m-wire] | A — E5. | Adaptador reutiliza domínio e anuncia somente capacidades implementadas; mapear timestamps sem alterar IPC epoch-ms. | S2 + S1 / P3 |
| M43 | Ombro: janela auxiliar, resumos e notas usando modelo local. [Ombro][m-ombro] | A/L — E5/E12. | ADR antes de introduzir inferência: manter Jig coordenador. Alternativa local equivalente no Linux ainda precisa de desenho e medição. | S2 + S1 / P3 |
| M44 | Recuperação de workspace e diagnóstico exportável. [Troubleshooting][m-troubleshooting] | P — E11/E12; diagnóstico sanitizado existe. | Backup restaurável da composição, notas/anexos e metadados; recuperação de dados corrompidos não inicia vazia silenciosamente. | S2 + S1 / P1 |
| M45 | Prevent sleep, safe launch, backups automáticos e restauração. [Changelog][m-changelog] | A/P — E11. | Ciclo dormir/acordar e crash testado; inibição de suspensão por API nativa com liberação garantida. | S2 + S1 bridge / P2 |
| M46 | Aparência e interação fiéis ao canvas de referência. [Introdução][m-intro], [Canvas][m-canvas] | P — E1/E2/E9. | Comparação visual em tamanho equivalente, incluindo seleção, menus, terminal real, nó focado, zoom e estado vazio. | S1 / P0 contínuo |
| M47 | Compatibilidade: Gemini, Hermes, Antigravity, Pi, Copilot e Cursor. [Changelog][m-changelog] | A/P — E7; suporte customizado não comprova roles/resume/menções. | Registrar capacidades por CLI e testar provisionamento, envio e retomada individualmente. | S2 / P2 |
| M48 | Atualização da aplicação e preferências associadas. [Changelog][m-changelog] | A/P — E12 prova configuração de build, não atualização. | Atualização verificável por plataforma, sem perder sessões ou dados e com falha recuperável. | S1 bridge + S2 release / P3 |

### Equivalentes Linux/macOS e limites reais

Estas são escolhas propostas de implementação; não são afirmações sobre uma
versão Linux do Maestri. A presença de um link Linux no site não prova entrega
de todas as capacidades. O [overview do XIRP][x-intro] declara beta macOS.

| Área | macOS | Linux | Regra para considerar entregue |
| --- | --- | --- | --- |
| Terminal/canvas | Tauri + xterm; otimização acelerada onde houver suporte | Mesma UI; testar WebKitGTK, compositor e escala | Comportamento medido nas duas plataformas; Metal não é requisito de portabilidade. |
| Floors | Worktree Git como base; clone otimizado pode ser opcional | Worktree Git como base; cópia/reflink apenas se suportado | Isolamento funcional não depende de APFS; documentar que worktree não copia mudanças sujas ou arquivos ignorados. [Floors][m-floors] |
| iOS | Detectar Xcode/runtime disponível | Simulator local não disponível | Expor indisponibilidade local; uma futura conexão com host Mac é outra capacidade, não simulação de suporte local. [Portais][m-portals] |
| Android | SDK/ADB/emulador compatíveis | SDK/ADB/emulador compatíveis | Instalação e disponibilidade verificadas; nenhum pacote presume hardware presente. |
| Ombro | Foundation Models é específico de versões/hardware Apple | Exige alternativa de inferência local ou cliente de agente local | Não chamar um badge de status de equivalente a resumo. [Ombro][m-ombro] |
| Busca do sistema | Spotlight pode ser integração opcional | Launcher/search-provider conforme desktop, além da busca interna | Busca interna completa nos dois sistemas; não alegar Spotlight no Linux. [Workspaces][m-workspaces] |
| Portais | WebKit nativo | WebKitGTK | Suporte real a armazenamento, downloads e automação; diferenças devem aparecer nas capacidades. |
| Atalhos | Modificadores adaptados a macOS | Evitar conflitos com terminal/gerenciador de janelas | Ação lógica comum; atalhos visíveis, remapeáveis e verificados por teclado. |
| Distribuição | `.app`/`.dmg`; assinatura/notarização pendentes | AppImage; dependências WebKitGTK documentadas | Artefato instalado e smoke test, além de CI declarada. E12. |

WSL, Win32 e ConPTY não entram: o usuário mantém Linux/macOS. Marca, cobrança,
licenciamento e serviços operados pelo fornecedor não são capacidades a
duplicar; recursos equivalentes de uso continuam mapeados. Compatibilidade
com o app Maestri Remote ou seus formatos não pode ser anunciada antes de
testes contra contratos/arquivos reais. Arquivo exportado próprio deve ter
identificação e versão próprias enquanto essa compatibilidade não existir.

## XIRP e adaptações locais selecionadas

O canvas continua sendo a superfície principal. As referências abaixo entram
por nós, painéis, paleta e ações contextuais; não exigem trocar o produto por
uma home administrativa. Não há conexão automática com Spotify Portal,
telemetria externa ou envio de transcript decorrente desta proposta.

| ID | Capacidade / origem | Estado e evidência | Resultado proposto | Dono / prioridade |
| --- | --- | --- | --- | --- |
| X01 | Projetos locais, pastas não Git, diretórios com vários repos, pin e importação de filhos. [Projetos][x-projects] | P — E6/E9. | Distinguir pasta de repositório; desabilitar apenas operações Git inaplicáveis. | S2 base + S3 descoberta + S1 / P1 |
| X02 | Sessões gerais, objetivo inicial, anexos e opções do agente. [Sessões][x-sessions] | P — E3: projeto obrigatório, PTY disponível. | Sessão geral sem projeto fictício; objetivo entregue ao agente; controles específicos e verificáveis. | S2 + S1 / P1 |
| X03 | Troca de agente, fork de conversa, shell vinculado e retomada. [Sessões][x-sessions] | A/P — E3/E7/E11. | Continuar conversa só quando adaptador suporta; shell compartilha worktree e tem lifecycle próprio. | S2 + S1 / P2 |
| X04 | Status de atividade/espera, grade de terminais, minimapa e busca. [Sessões][x-sessions] | P — E9/E10. | Agrupamento/tiling dentro do canvas, sem encerrar sessão ao ocultar painel. | S2 status + S1 / P1 |
| X05 | Preferências locais, atalhos, hooks, notificações, configurações nativas e diagnóstico. [Settings][x-settings] | P — E7/E9/E10. | Painel mostra somente opções implementadas; arquivos de credenciais ficam fora do editor. | S2 + S3 regras + S1 / P1 |
| X06 | Skills e regras globais/de projeto descobertas e consultáveis. [Projetos][x-projects] | A — E5. | Mostrar origem, escopo, precedência e conteúdo; edição explícita com proteção contra conflitos/symlinks. | S3 + S2 contrato / P1 |
| X07 | Status de workflow, arquivo de sessões, prompts salvos, uso/custo, revisão de PR, Doctor e cleanup. [Changelog][x-changelog] | A/P — E3: só lifecycle; E8 inspeção Git. | Workflow separado de processo; métricas medidas ou indisponíveis; cleanup mantém confirmação por estado. | S3 workflow/arquivo/prompts; S2 cleanup/métricas + S1 / P1/P2 |
| X08 | Cursor e controles específicos de agentes. [Changelog][x-changelog] | A/P — E7. | Consolidar registry real antes de novos presets; testar versão/flags instalados e comportamento no daemon. | S2 + S1 / P1 |
| X09 | Contexto de Workspace: entidades, links, documentos, decisões, wiki e sessões. [Workspaces][x-workspaces] | A — E5/E11. | Coleção local ligada a projetos e notas; exportação explícita e rastreabilidade de origem. | S3 + S2 contrato + S1 / P1 |
| X10 | Exposição de contexto via MCP e compartilhamento manual de sessão. [Workspaces][x-workspaces] | A — E5. | Primeiro contexto local autorizado; conector Portal opcional permanece separado e depende de conta/API reais. | S3 + S2 / P3 |

### Propostas opcionais do Portal — fora dos goals atuais

Os itens O01–O03 não foram selecionados para entrega e não entram no critério
de conclusão do objetivo. São referências para uma futura decisão de produto.

| ID | Referência | Estado | Possível adaptação futura |
| --- | --- | --- | --- |
| O01 | Catálogo de software com identidade, ownership, relações, tags e busca. [Portal Catalog][p-catalog] | A — E6 não modela entidades. | Descritores locais vinculados a repositórios/documentação. |
| O02 | Templates com formulário, passos, dry-run, logs e resultados. [Portal Scaffolder][p-scaffolder] | A — E5. | Geração local revisável e versionada, separada dos presets de canvas. |
| O03 | Indicadores de uso e adoção. [Portal Insights][p-insights] | A — E11. | Métricas locais com definições explícitas; não confundir com uso/custo nativo de X07. |

O01–O03 são adaptações propostas, não compatibilidade com plugins do Backstage.
Instalar o Portal inteiro, comprar serviços, ligar contas ou copiar dashboards
empresariais não é pré-requisito para estas entregas locais. Uma integração
real deverá declarar credenciais, escopos e quais dados saem da máquina.

## Ordem e critérios de aceitação

1. **P0: realidade do produto.** Fechar a ligação daemon/worktree e manter o
   canvas fiel e operável. Exercitar `create → start → subscribe → stop`,
   reconexão do desktop e remoção em duas etapas através do socket real.
2. **P1: trabalho completo local.** Notas duráveis, arquivos/editor/Git,
   roles/conexões/composer, projetos e preferências; prompts e regras locais
   acrescentam contexto sem competir com o canvas.
3. **P2: automação e composição.** Floors completos/hooks, rotinas, fichários,
   partituras, desenho e continuidade das sessões. Cada fluxo deve ter persistência,
   cancelamento e comportamento de erro demonstrados.
4. **P3: ambientes e controle externo.** Remotos, portais, dispositivos,
   Wire e assistente local exigem ADRs e testes de capacidade por plataforma.
   A prioridade posterior não remove essas linhas do objetivo.

Para cada entrega, registrar no PR/relatório: IDs desta matriz, commit,
comando de teste e resultado, captura visual quando relevante, evidência
Linux/macOS e lacunas restantes. Teste de Rust isolado não prova integração
Tauri; browser com IPC mockado não prova execução do daemon.

Regressões transversais derivadas da inspeção das referências: preservar
rascunhos/seleção/foco ao alternar contexto; não duplicar prompts ou processos;
não perder arquivos em autosave/recovery; validar escopo de arquivo/Git/role;
tratar output volumoso, clone lento, suspensão, DPI e arquivo corrompido.
Os casos exatos entram na suíte do recurso quando ele for implementado.

O objetivo só pode terminar quando todas as linhas obrigatórias tiverem
evidência do fluxo real e cada limite de plataforma tiver resultado explícito.
Não reduzir o inventário ao que ficou pronto em uma rodada. Os dois goals
para execução paralela estão em [parallel-goals.md](codex/parallel-goals.md).

[m-changelog]: https://www.themaestri.app/en/changelog
[m-intro]: https://www.themaestri.app/en/docs/intro
[m-workspaces]: https://www.themaestri.app/en/docs/workspaces
[m-canvas]: https://www.themaestri.app/en/docs/canvas
[m-shortcuts]: https://www.themaestri.app/en/docs/shortcuts
[m-batuta]: https://www.themaestri.app/en/docs/batuta-search
[m-terminals]: https://www.themaestri.app/en/docs/terminals
[m-composer]: https://www.themaestri.app/en/docs/prompt-composer
[m-notes]: https://www.themaestri.app/en/docs/notes
[m-ficharios]: https://www.themaestri.app/en/docs/ficharios
[m-connections]: https://www.themaestri.app/en/docs/connections
[m-maestro]: https://www.themaestri.app/en/docs/maestro
[m-partituras]: https://www.themaestri.app/en/docs/partituras
[m-files]: https://www.themaestri.app/en/docs/file-tree
[m-floors]: https://www.themaestri.app/en/docs/floors
[m-portals]: https://www.themaestri.app/en/docs/portals
[m-routines]: https://www.themaestri.app/en/docs/routines
[m-environments]: https://www.themaestri.app/en/docs/environments
[m-ombro]: https://www.themaestri.app/en/docs/ombro
[m-wire]: https://www.themaestri.app/en/docs/wire
[m-troubleshooting]: https://www.themaestri.app/en/docs/troubleshooting
[x-intro]: https://backstage.spotify.com/docs/xirp
[x-projects]: https://backstage.spotify.com/docs/xirp/projects
[x-sessions]: https://backstage.spotify.com/docs/xirp/sessions
[x-settings]: https://backstage.spotify.com/docs/xirp/settings
[x-workspaces]: https://backstage.spotify.com/docs/xirp/workspaces
[x-changelog]: https://backstage.spotify.com/docs/xirp/changelog
[p-catalog]: https://backstage.spotify.com/docs/portal/core-features-and-plugins/catalog
[p-scaffolder]: https://backstage.spotify.com/docs/portal/core-features-and-plugins/scaffolder
[p-insights]: https://backstage.spotify.com/docs/portal/core-features-and-plugins/insights
