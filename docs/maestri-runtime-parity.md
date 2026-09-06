# Evidências e sequência de integração do runtime Maestri

Auditoria inicial de S2 em **2026-09-05**, no baseline
`0ac8dd7d49eefee16e5efbf389994f551bc584f5`, branch
`feat/maestri-runtime`, worktree `/Users/eguimacs/cli-master-runtime`.
Mudanças em andamento somente promovem um estado após registrar o commit e
a execução que o comprova no [relatório de integração](codex/maestri-runtime-report.md).

Este documento complementa a matriz central `docs/maestri-xirp-parity.md`,
mantida por S1. Os IDs M01–M48 e X01–X10 continuam sendo os dela; as descrições
completas, fontes por recurso e critérios visuais não são duplicados aqui.
Na auditoria, a matriz e `docs/codex/parallel-goals.md` foram lidos na worktree
principal, `/Users/eguimacs/cli-master`, onde ainda não estavam no baseline
de S2. Publicar este recorte não substitui integrar esses documentos centrais.

## Fontes e interpretação

O [changelog oficial](https://www.themaestri.app/en/changelog), consultado em
2026-09-05, chega a 0.45.3, de 2026-09-04. Ele amplia o escopo até controle
remoto de rotinas/floors, sessões persistentes e retomada de conversas.
As páginas de [arquivos](https://www.themaestri.app/en/docs/file-tree),
[workspaces](https://www.themaestri.app/en/docs/workspaces),
[floors](https://www.themaestri.app/en/docs/floors) e
[notas](https://www.themaestri.app/en/docs/notes) também foram consultadas.
As opções de implementação e os testes abaixo são propostas para Jig.
Não se infere equivalência Linux a partir de links de download no site.

### Como ler a evidência

- **A:** implementação funcional não encontrada no baseline examinado.
- **P:** infraestrutura parcial ou integração incompleta.
- **V:** fluxo comprovado por teste executado e evidência identificada.
- **L:** limite de plataforma ainda exige resultado explícito.
- **S1:** interface, Tauri e integração visual; **S2:** runtime/contratos;
  **S3:** knowledge/workflows locais. Uma dependência de S1/S3 permanece aberta.

Nenhuma linha recebe V nesta auditoria estática. Teste existente sem execução
é evidência de cobertura pretendida. Compilar um tipo, anunciar um método,
passar teste direto da saga ou renderizar uma UI com IPC mockado não comprova
o caminho desktop → socket → domínio → efeito real.

| Evidência | Arquivos no baseline | Limite observado |
| --- | --- | --- |
| R1 | `crates/daemon/src/sessions.rs`, `server.rs` | PTY e `current` estão ligados ao daemon; `new_worktree` retorna `worktree_sessions_unavailable`; snapshot usa `worktrees: Vec::new()`. |
| R2 | `crates/session/src/{saga,create,remove,recover,token}.rs`, `tests/{create_saga,remove_saga,recovery}.rs` | Saga cria e inicia em uma operação interna; persistência e remoção segura existem, mas os métodos públicos de worktree não têm dispatch. |
| R3 | `crates/core/src/wire/{method,request,response,event_name}.rs`, `protocol/catalog.json` | Catálogo existente não contém files/workspace/floor/canvas/knowledge/routine/environment. Eventos anunciados precisam de auditoria de emissão. |
| R4 | `crates/storage/migrations/0001_initial.sql` até `0003_recovery_metadata.sql`, `src/{migrate,settings,recovery}.rs` | SQLite guarda projetos/agentes/sessões/worktrees/settings; não há documento canvas ou modelos posteriores. |
| R5 | `crates/daemon/src/projects.rs`, `crates/daemon/tests/daemon_ipc.rs` | Cadastro aceita pasta não Git e subdiretório; teste `project_registration_accepts_a_plain_folder` existe. Organização e importação de filhos não existem. |
| R6 | `crates/daemon/src/git_inspection.rs`, `crates/daemon/tests/git_inspection.rs`, `crates/git/src` | Inspeção status/diff por alvo registrado; não implementa o conjunto de ações de escrita Git. |
| R7 | `crates/agents/src/{builtins,registry}.rs`, `crates/daemon/src/sessions.rs` | Registry e seed próprio do daemon divergem; Gemini no adapter não implica preset executável no produto. |
| R8 | `crates/session/src/{manager,replay,runtime}.rs`, `crates/storage/src/recovery.rs` | Handles e replay são memória do daemon; reinício reconcilia metadados, sem retomar conversa nativa/tmux. |
| R9 | `apps/desktop/src/app/features/canvas/{canvas-state,useCanvasState}.ts` | Documento local contém notas/terminais/conexões; não existe serviço durável no daemon. Frontend é somente leitura nesta sessão. |
| R10 | `crates/daemon/tests/{daemon_ipc,git_inspection,event_stream}.rs`, `crates/e2e/tests/acceptance.rs` | Existem casos reais de IPC e testes da saga; o caso de duas sessões isoladas usa saga diretamente. Não prova isolamento pela API pública. |
| R11 | `docs/adr/0001-session-ownership.md` até `0004-git-worktree-safety.md`, `AGENTS.md` | Ownership, epoch-ms, revisão de contrato e token por estado são invariantes. Floors compartilhados/hook/arquivo precisam de decisões adicionais. |
| R12 | `.github/workflows/ci.yml`, `docs/{PACKAGING,backup-and-recovery,KNOWN_ISSUES}.md` | Matriz Linux/macOS e procedimentos existem; auditoria não executou CI/pacote nem restauração visual. |

## Evidência posterior ao baseline

Em 2026-09-05, `7693468` e `3c25a85` na branch `feat/maestri-runtime`
ligam preparação/start, snapshot/listagem e remoção segura ao socket real.
Os 118 testes core/daemon passaram no macOS, incluindo 9 novos de worktree;
sagas/storage, Clippy e contratos frontend também foram verificados conforme
[relatório S2](codex/maestri-runtime-report.md). Isso comprova o incremento de
runtime M14/M34/M35; **os IDs completos continuam P**, pois floors/landing e
integração desktop não estão concluídos. Linux/CI/pacote seguem sem resultado.
Nenhum recurso posterior ganha V com essa execução.

## Recorte de runtime por ID central

A coluna de fonte remete à capacidade de mesmo ID na matriz central. Linhas
agrupadas mantêm todos os IDs para auditoria sem redefinir seu escopo.

| IDs / fonte central | Estado no baseline / evidência | Próximo efeito de runtime comprovável | Dependência de integração |
| --- | --- | --- | --- |
| M01, M02, M03, M10, M46 — canvas | P: R9 | Persistir composição versionada, identidade e relações sem guardar PTY. | S1 implementa interação/visual; S2 oferece armazenamento sem determinar gestos. |
| M04 — workspaces | P: R4/R5 | Diretório/identidade/ordenação por workspace após restart; herança de cwd explícita. | S1 rail/edição; decisão de identidade workspace versus project antes de migrar. |
| M05 — instruções/importação | A: R3 | Importar pacote revisado sem sobrescrever arquivos externos; revisão de conteúdo em escrita. | S3 regras, S2 serviço de arquivo/importação, S1 preview. |
| M06 — paleta/busca | P: R9 | Consultas de arquivos e documentos limitadas ao escopo autorizado. | S1 busca/atalhos; S3 índices de contexto. |
| M07 — notas | P: R9, persistência A: R4 | Markdown real, leitura após restart, escrita atômica e conflito externo preservado. | S1 edita/preview; distinguir nota gerenciada de referência a arquivo externo. |
| M08 — acesso a notas | A: R3 | Resolução autorizada de grafo com leitura limitada, atualização e revogação. | Conexão visual S1 não concede autorização sozinha. |
| M09 — fichários | A: R3/R4 | Mover página entre agrupamentos sem duplicar conteúdo/identidade. | S1 páginas; depende de notas e grafo duráveis. |
| M11, M16 — preferências/bloqueio | P: R4/R9 | Preferências por escopo e revisão; bloqueio impede escrita também pela API de agente. | S1 aparência/atalhos; settings genérico não prova painel funcional. |
| M12 — partituras | A: R3/R4 | Exportação própria versionada e importação revisável, referências remapeadas, sem dados de execução. | S1 seleção/preview; não anunciar leitura de `.maestri` sem fixture real. |
| M13 — mídia | A: R3 | Arquivos referenciados com ownership e leitura limitada; remover nó preserva fonte externa. | S1/Tauri preview/clipboard Linux e macOS. |
| M14 — terminais/presets | P: R1/R7 | Usar definições reais do registry em create/start e editar argv sem divergência de seed. | S1 seletor; testar cada preset anunciado. |
| M15 — roles | A: R3/R7 | Provisionamento por adaptador com revisão e origem, sem substituir arquivos inseguros/existentes. | S3 descoberta e conteúdo; S1 seleção. |
| M17 — atenção | P lifecycle: R8 | Registrar sinais verificáveis de espera/atenção separados de status do processo. | S1 notificações/navegação; silêncio PTY não é evidência de aprovação. |
| M18, M19 — entrada/composer | P PTY: R1/R8; demais A | Draft durável e entrega única de texto/anexos no ambiente correto. | S1 IME/seleção/teclas; S3 prompts inserem draft, não fazem envio implícito. |
| M20 — conexões | P visual: R9; domínio A | Grafo durável com revisões e permissões independentes de estilo. | S1 geometria e navegação entre floors. |
| M21 — comunicação | A: R3/R4 | Pedido/resposta entre duas sessões com correlação, prazo, cancelamento e autorização atual. | Requer cliente CLI local e alvo verificável; não proxyar tráfego dos vendors. |
| M22 — Maestro | A: R3 | Recrutamento/dispensa autorizado reutiliza SessionManager e modelos de workspace/floor. | S1 gerência; acesso comum não implica privilégio gerencial. |
| M23, M24 — rotinas | A: R3/R4 | Agenda/histórico duráveis; skip, execução única e restart não duplicam disparos. | Hooks pelo dono da execução; S1 UI/histórico; S3 workflow não é scheduler. |
| M25 — persistência de processo | P: R8 | Distinguir cliente reconectado de daemon novo e attachment tmux verificado. | S1 mostrar capacidade real; persistência após crash exige novo ADR. |
| M26 — conversa | P metadados: R8; resume A | Adapter persiste e valida identidade nativa; retoma a conversa escolhida. | Nunca reconstruir conversa a partir do PID ou de replay PTY. |
| M27, M28 — ambientes | A: R3/R7 | Resolver execução, cwd, arquivo e provisionamento no mesmo host; reconexão não duplica agente. | S1 ambiente/override; SSH/Docker/custom usam argv e transporte definido em ADR. |
| M29 — arquivo | A: R3 | Listar/criar/mover/remover sob raiz registrada, tratar nomes Unix e links sem escapar do escopo. | S1 árvore; S3 reutiliza segurança de I/O. |
| M30 — editor | A: R3 | Read/write em disco com limite de tamanho e revisão esperada; mudança externa gera conflito. | S1 buffers/edição; proteger conteúdo não salvo. |
| M31 — busca/tabs | A: R3/R4 | Busca cancelável com paginação estável; persistência de tabs/defaults. | S1 abre arquivo/linha; definir indexação e limites após file service. |
| M32 — Git local | P: R6 | Stage/unstage/commit e descarte revisado no repositório derivado do alvo. | S1 diff real; descarte precisa prova de estado, não um booleano force. |
| M33 — Git remoto/histórico | A: R6 | Git argv real com exclusão mútua, cancelamento e erros acionáveis. | S1 opções/history; credenciais continuam com Git do usuário. |
| M34 — floors | P infraestrutura: R2 | Primeiro session.create/start isolado pelo socket; depois floor com várias sessões e cwd compartilhado correto. | S1 canvas por floor; worktree por sessão não equivale a floor. |
| M35 — landing/remoção | P remoção: R2; landing A | Ligar preparo/token/remove e depois preview de landing vinculado a refs e estado atuais. | S1 confirmação; manter branch/diretório diante de conflito. |
| M36 — hooks | A: R3/R4 | Comandos ordenados com contexto, prazo, cancelamento e histórico via SessionManager. | Novo ADR de jobs; setup/teardown não abrem executor paralelo. |
| M37, M38, M39 — portais | A: R3 | Domínio de autorização e alvos de automação sobre bridge real, com revogação. | S1/Tauri webview e suporte Linux/macOS; adapter deve anunciar capacidades observadas. |
| M40 — dispositivos | A/L: R3/R12 | Sessão de dispositivo e operações verificadas no SDK local. | Android nos dois hosts; iOS local exige macOS/Xcode. Não chamar indisponibilidade de paridade. |
| M41, M42 — Wire | A: R3 | Transporte remoto autenticado reutiliza domínio; pareamento/revogação e feed recuperável. | Novo ADR e S1 cliente; socket local não prova protocolo Wire compatível. |
| M43 — Ombro | A/L: R3 | Decisão explícita sobre inferência local sem transformar o coordenador em proxy de agente. | Alternativa Linux e viabilidade medidas; badge de status não equivale a resumo. |
| M44, M45 — recuperação/plataforma | P: R4/R12 | Restaurar composição, notas e anexos reais com manifest/revisão; recuperar corrupção sem apagar dados. | S1 preview/restore, bridge de suspensão por plataforma. |
| M47 — agentes adicionais | P: R7 | Capacidade por versão/adaptador, envio/provisionamento/resume testados individualmente. | S1 exibe somente presets detectados e comportamentos suportados. |
| M48 — atualização | P build: R12 | Pacote atualizado preserva DB, ownership do daemon e identidade de conversa. | S1/Tauri/release e smoke Linux/macOS. |
| X01 — projetos/organização | P: R5 | Preservar pastas não Git e descoberta explícita; pin/importação persistentes. | S3 organização/descoberta, S1 navegação. |
| X02 — sessões gerais | P PTY: R1 | Objetivo/anexos/opções com destino verificável; sessão geral sem projeto fictício. | ADR de identidade/scope e S1 composer; S3 draft/contexto. |
| X03 — continuidade/fork | P: R8 | Fork/resume nativo quando suportado; shell vinculado com lifecycle independente. | S2 adapters/floor; S1 escolha explícita. |
| X04 — atividade/apresentação | P: R8/R9 | Atenção separada de processo/workflow, sem encerrar ao ocultar nó. | S1 grade/minimapa/busca. |
| X05 — preferências/diagnóstico | P: R4/R7/R12 | Preferências revisadas e diagnóstico sanitizado do fluxo real. | S3 regras; S1 UI; arquivos de credenciais não entram no editor. |
| X06 — regras/skills | A: R3 | Descoberta conhecida e leitura segura com origem/escopo; edição usa file service único. | S3 knowledge, S2 contratos compartilhados. |
| X07 — workflow/arquivo/prompts | A/P: R1/R6 | Metadados de workflow separados do processo; prompts/contexto persistem com revisão. | S3 responsável; S2 cleanup e métricas nativas, nunca custo inferido de PTY. |
| X08 — opções dos agentes | P: R7 | Registry consolidado e controles somente quando adapter comprova suporte. | S1 presets e opções. |
| X09 — contexto | A: R3/R4 | Knowledge global/projeto com origem, revisão, referências e busca/exportação. | S3 serviço/componentes; S2 migração/IPC; S1 montagem no canvas. |
| X10 — MCP/compartilhamento | A: R3 | Expor apenas contexto local autorizado; envio explícito fora da máquina em contrato próprio. | S3 ferramentas; S2 autorização/transporte; conector Portal continua opcional. |

## Sequência de incrementos verticais

1. **M14/M34/M35, base de X03:** separar preparação de criação e início na
   saga; ligar `new_worktree`, snapshot/listagem e remoção no socket; executar
   processo curto somente após `session.start`, incluindo subdiretórios,
   concorrência, compensação, restart e token invalidado por edição externa.
   S1 consome as respostas e comprova o fluxo no desktop.
2. **M29/M30 e base de M07/X06:** serviço de arquivos local compartilhado,
   inicialmente list/read/write em alvo registrado e revisão de conteúdo.
   Editor S1 lê, modifica e salva arquivo real; um segundo escritor produz
   conflito visível, sem sobrescrita. S3 reutiliza o leitor seguro para regras.
3. **M04/M07/M20/M44 e base de X09:** workspace/documento canvas revisados,
   nota Markdown em arquivo real e referências estáveis. S1 importa o documento
   local existente uma única vez, confirma persistência e só depois muda a
   fonte principal. Restart/reconnect recuperam nota, composição e vínculos.
   S3 entrega knowledge em migração reservada, com refresh/eventos no mesmo IPC.
4. **M14/M15/M17/M19/M21/M25/M26/M47/X02/X03/X08:** registry único,
   capacidades por adapter, roles, draft/anexo e resume. Em seguida autorização
   de grafo e cliente CLI: duas sessões reais trocam pedido/resposta; revogação,
   timeout e restart têm resultado explícito. Não confundir resume de conversa
   com reanexar processo tmux; ambos precisam de aceitação própria.
5. **M32/M33/M34/M35/M36:** Git de escrita, floor compartilhado, landing e
   hooks. Provar várias sessões no mesmo floor e terminais da ground floor
   preservados; hook é um job sob SessionManager, com política documentada
   para erro/timeout/teardown. Confirmar destruição com estado atualizado.
6. **M09/M11/M12/M23/M24/M44/M45:** fichários, preferências, exportação própria,
   rotinas duráveis e snapshots restauráveis. Testar duas gravações concorrentes,
   interrupção durante importação/backup e scheduler reiniciado antes/depois
   de registrar disparo. Histórico registra skipped/failed, não conclusão falsa.
7. **M27/M28/M37–M42:** ambientes reais antes da automação remota: SSH/Docker/
   custom, arquivo/anexo no ambiente alvo e persistência tmux. Depois bridge
   de portais/dispositivos e transporte remoto autorizado, cada um em ADR e
   incremento próprio com capacidades por plataforma.
8. **M43/M45/M48 e aceitação transversal:** decisão sobre assistente local,
   suspensão e atualização. Provar pacotes em Linux/macOS; reunir evidência
   S1/S3 para os IDs ainda parciais. Prioridade posterior não remove requisito.

Cada passo contém vários commits pequenos. Contrato só entra no catálogo
junto de handler utilizável e teste pelo socket; a sequência não autoriza
publicar stubs para reservar nomes.

## Decisões necessárias antes de ampliar fronteiras

| Tema do ADR futuro | Decisão que precisa ficar concreta | Alternativas e consequência |
| --- | --- | --- |
| Arquivos e revisão de documentos | Raiz/identidade autorizadas, encoding de nomes Unix, limits, revisões de conteúdo e escrita atômica. | Path relativo UTF-8 é menor, mas exclui nomes válidos; ID opaco/bytes preserva identidade. Não reutilizar `GitRelativePath` como path genérico. Definir symlinks e proteção contra troca de diretório durante a operação. |
| Workspace, floor e canvas | Project continua raiz registrada; workspace/floor são identidades próprias, com revisão de documento e vínculo de várias sessões ao isolamento. | Documento único facilita evolução S1, mas pode conflitar inteiro; tabelas por nó reduzem conflito e aumentam migrações. Escolha deve preservar migração do canvas existente e não duplicar truth em localStorage. |
| Notas e anexos | Ownership de arquivo gerenciado/externo, journal para SQLite+filesystem e recuperação. | SQLite de metadados + Markdown real preserva interoperabilidade, mas exige compensação/reconciliação; não esconder nota só em JSON de canvas. |
| Jobs, hooks e rotinas | CommandSpec/SessionManager como execução única, modelo durável de run, idempotência, cancellation e teardown. | Reusar sessão como job simplifica ownership; job explícito separa histórico/TTY. Ambos exigem um único executor e distinção prompt/programa. |
| Continuidade e ambientes | Resume por adapter versus attachment tmux, identidade de processo por ambiente e segredo/transporte. | Nova conversa não equivale a resume; PID antigo nunca prova ownership. Persistência opcional deve ter fallback explícito sem afirmar retomada. |
| Grafo/CLI, portais e Wire | Principais, autorização/revogação por recurso, correlação e transporte remoto separado do IPC local. | Grafo visual pode referenciar relações funcionais, mas estilos não concedem acesso. Não anunciar compatibilidade Wire sem contrato externo testado. |
| Inferência local e plataformas | Preservar coordenador local-first, alternativa Linux e política de capacidade por hardware. | Integrar inferência altera fronteira de produto; requer decisão registrada antes do código. |

Os números de migração e ADR são alocados por S2 no momento de integrar,
considerando commits S3. A matriz central continua responsável por paridade
global; este recorte não conclui o goal de backend.
