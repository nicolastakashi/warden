# Warden — Roadmap

Roadmap único do Warden: junta a **estratégia de produto** (posicionamento, fosso,
onde ganhamos) com a **execução de UX/DX** (como cada passo se sente). Se você está
pegando o projeto do zero, leia antes o [`conclusion.md`](conclusion.md) (avaliação
honesta + o único limite que importa), o [`design.md`](design.md) (arquitetura) e o
[`decisions.md`](decisions.md) (escolhas — inclusive o §10, que enxugou o produto).

Ambição atual: **projeto OSS com tração** — sucesso = instalações, adapters e
rule-packs contribuídos por terceiros, estrelas. Não é produto comercial (nada de
dashboard hospedado) nem só ferramenta pessoal.

**Onde estamos (pós-slim):** o produto *é* o **runtime gate**. Removemos o matcher
`llm`, o score ponderado / `weight` e o `extent` (`decisions.md §10`); o CI gate
virou um **scan simples**. O near-term inteiro é runtime; a sofisticação do CI
(diff-scoping, score) é um **Capítulo CI — adiado** até o runtime estar redondo e
adotado.

## Tese de posicionamento

> **Warden é o único policy-as-code que roda no diff do PR *e* na tecla do agente,
> com a mesma regra YAML ciente de AST.**

O scan competitivo (jul/2026) mostrou que o nicho de *runtime gate* virou disputado —
**Agent RuleZ** (Rust + YAML determinístico, Claude-Code-specific, superfície de
eventos mais rica, sem CI, sem AST) e **det-acp** (TS, proxy MCP/shell, multi-agente
de verdade, sem CI, sem AST). Nenhum dos dois tem **CI gate** nem inspeção de
**estrutura de código**. Logo não competimos em "bloquear comando no Claude Code" —
competimos na **interseção CI + runtime + AST**, que é nossa e de mais ninguém.

**A frase que só nós podemos dizer:** *"a mesma regra que reprova seu PR também
impede o agente de escrever o código — e ela entende imports e AST, não só regex."*

## ICP & job-to-be-done

- **ICP:** tech/staff eng em time que já usa coding agents e sente que "os agentes
  reintroduzem coisas que a gente combinou não fazer".
- **JTBD:** *"quero que uma convenção do time seja aplicada automaticamente —
  enquanto o agente digita — sem eu virar revisor de guardrail."*
- **Momento de conversão OSS:** ligar o gate em ~3 comandos (hero-path) e ver ele
  bloquear um edit ruim na hora.

## North star

Três alavancas — adoção, retenção e defensabilidade:

- **Hero-path do runtime gate (adoção).** O item mais forte do produto
  (`conclusion.md`) hoje é o passo mais escondido. Colapsar
  `instalar → obter uma regra → conectar o hook → ver bloquear um edit ruim`
  de ~6 passos manuais para ~3 comandos (R7→R8→R9).
- **Loop de autoria (retenção).** Hoje se escreve regra às cegas e só se descobre se
  acertou rodando o gate num fixture. Fechar
  `escrever regra → ver o que ela pega → confiar nela` (R4+R2).
- **Fosso (defensabilidade).** Aprofundar o que ninguém copia: **AST multi-linguagem**
  (S2/S3) — que já serve o runtime hoje — e o core **agent-agnostic** entregue de
  verdade (S4/S5). O CI diff-scoped (S1) é a peça de defensabilidade do **Capítulo
  CI**, depois.

A armadilha a evitar: **não** importar "zero-config defaults" literalmente. Warden
não envia regras de propósito (`conclusion.md`: "match ≠ violação, a qualidade da
regra domina, confirme a intenção com um humano"). O onboarding world-class aqui é
**discovery guiado** (as skills `warden-rule-discovery` → `warden-rule-author` já são
essa UX), não regras enlatadas.

## Princípios & não-objetivos

- **Determinístico > probabilístico** — é a marca. (Por isso o matcher `llm` foi
  removido, não só depreciado — `decisions.md §10`.)
- **"Candidatos para julgamento humano", não "encontra violações"** — não overclaim.
- **Não** virar gateway/proxy de runtime (jogo do det-acp) — não perseguir
  "enforcement imburlável" agora.
- **Não** inflar a superfície de runtime pra empatar com o RuleZ (inject de contexto,
  N eventos) — nosso valor não está aí.
- Transferimos o **princípio** de DX world-class (Vercel/Resend: time-to-value curto,
  output legível, erros que ensinam), **não a superfície** (nada de dashboard).

## Visão geral (priorizado)

Itens `R*` são de UX/DX; itens `S*` são estratégicos/fosso. A coluna **Quando**:
`agora` = trilho runtime-first; `CI` = Capítulo CI adiado. Esforço: **S** ≈ horas–1
dia · **M** ≈ poucos dias · **L** ≈ semana+.

| ID | Item | Quando | Track | Impacto | Esforço |
|----|------|--------|-------|---------|---------|
| ~~R4~~ | ~~`warden test` / dry-run de regra~~ | ✅ | Autoria | **feito** — fecha o loop de retenção | M |
| R2 | `validate --against` avisa regra morta (casa 0) | agora | Autoria | Alto (mata o pior modo de falha) | S |
| R5 | `warden new-rule --type <t>` (template por tipo) | agora | Autoria | Alto | S |
| R6 | Normalizar o footgun do glob (`src/**`) | agora | Autoria | Médio | S |
| R3 | Mensagens de erro que ensinam o fix | agora | Polimento | Médio | S |
| R10 | Output legível: cor + símbolos + contagens + timing | agora | Polimento | Médio-alto | S |
| R7 | Binários pré-compilados + `curl\|sh` + Homebrew | agora | Hero-path | **Maior** (corta a barreira de instalação) | M |
| R8 | `warden init` (scaffold mecânico → roteia p/ discovery) | agora | Hero-path | Alto | M |
| R9 | `warden hook install` (merge idempotente no settings.json) | agora | Hero-path | Alto | S |
| S2 | +1 linguagem (TS/JS) + import walker Rust | agora | Moat/AST | Alto (multi-lang de verdade) | M–L |
| S3 | Biblioteca de regras `query` / starter packs (`npx skills add`) | agora | Moat/AST | Alto (tração + prova não-import-only) | M |
| R12 | Domar `query`: playground + receitas + erro claro de node kind | agora | Moat/Autoria | Alto (destrava o tipo mais poderoso) | M |
| R13 | Reframar taxonomia: `structural`→`imports` + guia de decisão | agora | Moat/Autoria | Médio | S |
| S4 | 2º adapter (Cursor **ou** MCP genérico) | agora | Reach | Alto (cumpre agent-agnostic) | M |
| S5 | Contrato de adapter documentado + guia de contribuição | agora | Reach | Alto (comunidade carrega adapters) | S |
| S1 | Diff-scoping do CI gate (flag só linhas alteradas) | CI | Trust | **Maior** (torna o CI confiável) | M–L |
| R11 | Anotações do GitHub Actions no `check` (depende de S1) | CI | Trust/Plataforma | Alto | S |
| R1 | `why` no CI report | CI | Trust/Polimento | Médio | S |
| R15 | `warden.toml` para defaults (rules dir, format) | backlog | Polimento | Baixo | S |
| ~~R14~~ | ~~Marcar `llm` como escape hatch~~ | ✅ | — | resolvido por **remoção** (§10) | — |

## Sequência recomendada

Fases por dependência, não uma lista chapada. Regra de ouro: **não traga gente para
autoria cega** — autoria confiável e hero-path antes de escalar; o Capítulo CI só
quando o runtime estiver redondo e adotado.

### Fase 0 — Quick wins (barato, envia já)
- **R3 — Erros que ensinam.** `error loading rules: {e}` reporta estado sem ensinar.
  — *Done when:* sem rules dir → "nenhuma regra encontrada em `./rules`; rode
  `warden init` ou aponte com `--rules <dir>`". `src/main.rs`.
- **R10 — Output legível.** Hoje `human.rs` é texto puro (a cor do demo vem do bash
  externo). — *Done when:* cor com detecção de TTY + `NO_COLOR`, símbolos `✗/⚠/•`,
  cabeçalho já com contagens; adicionar timing (`… · 3 files · 1 blocking · 3.4s`).
  O timing é o superpoder escondido (5.6k arquivos em ~3,4s).

### Fase 1 — Loop de autoria (retenção)
A DX central do runtime: parar de escrever regra às cegas.

- **R4 — `warden test` / dry-run. ✅ Feito.** `warden test <rule.yaml> <path>` roda
  UMA regra contra um path (sem rules dir), honra o `paths` da regra, ignora `scope`,
  e lista `file:line → snippet` + contagens. Engine em `ci_gate::run_rule`;
  carregamento de regra única em `load::load_rule_file`. Cobre o "send test email" do
  Resend para regras.
- **R2 — Aviso de regra morta.** `validate` só checa forma; uma regra com regex
  errado, node kind existente-mas-improdutivo ou `paths` que não casa nada valida e
  silenciosamente não faz nada. — *Done when:* `warden validate --against <path>`
  reporta `regra X: casou 0 arquivos/nós` como warning. Reaproveita `ci_gate`.
- **R5 — `warden new-rule --type <t>`.** Emite o template correto por tipo, matando o
  custo dos sub-schemas diferentes. — *Done when:* gera YAML válido comentado para
  `pattern|imports|query`, pronto para `validate`.
- **R6 — Normalizar o glob.** Hoje toda regra path-scoped precisa de `src/**` **e**
  `**/src/**` (footgun em `no-unwrap-in-src.yaml`). — *Done when:* `src/**` casa
  top-level e aninhado, **ou** `validate` avisa "glob casou 0 arquivos". `src/glob.rs`.

### Fase 2 — Colapsar o hero-path (adoção)
Depende da Fase 1: não adianta trazer gente para uma autoria cega.

- **R7 — Distribuição.** `cargo install --path .` exige toolchain Rust + C compiler —
  a maior barreira. Warden já é binário estático único. — *Done when:* GitHub Releases
  com binários por plataforma + `curl -fsSL .../install | sh` + fórmula Homebrew.
- **R8 — `warden init`.** Cria `rules/`, escreve **um** exemplo funcional comentado, e
  **roteia para o discovery guiado** (não despeja policy enlatada). — *Done when:*
  projeto zero-regras vira projeto com 1 regra de exemplo + next step apontando para a
  skill de discovery.
- **R9 — `warden hook install`.** Merge idempotente no `.claude/settings.json`
  (matcher `Write|Edit`, comando com `${CLAUDE_PROJECT_DIR}`), detecta wiring
  existente. — *Done when:* um comando ativa o gate e confirma "✓ gate ativo".

### Fase 3 — Fosso & alcance
O que blinda contra o RuleZ/det-acp fecharem o gap. Moat (AST) ∥ Reach (adapters).

- **S2 — +linguagens + import walker Rust.** "Multi-language" tem que ser verdade,
  inclusive na própria casa (hoje `structural` não acha nada em `.rs`). — *Done when:*
  gramática + extrator de imports para TS/JS em `src/lang.rs`; import walker para Rust;
  um teste provando a mesma regra spanning linguagens.
- **S3 — Biblioteca de regras `query` / starter packs.** Conjunto curado de regras
  `.scm` **de intenção nítida** (no-unwrap, no-console.log, forbidden-imports comuns —
  *não* regras soft dependentes de intenção), instalável via `npx skills add`.
  Conteúdo = tração e prova que não é import-only. — *Done when:* pacote por linguagem
  documentado e instalável.
- **R12 — Domar `query`.** Hoje expõe tree-sitter cru (node kinds da gramática) —
  expert-only. — *Done when:* `warden query try '<scm>' <file>` para iterar; receitas
  por linguagem; erro claro quando o node kind não existe na gramática.
- **R13 — Reframar taxonomia.** `structural` só faz forbidden-imports mas soa geral;
  `query` é o que de fato é estrutural. — *Done when:* `structural` renomeado para
  `imports` (schema é POC, sem backward-compat) + guia de decisão "pattern vs imports
  vs query" no `--help`.
- **S4 — 2º adapter.** Prova o design bet ("novo `parse_/format_`, zero mudança no
  core"). — *Done when:* segundo par de tradução (Cursor **ou** contrato genérico via
  MCP) com teste, sem tocar o core.
- **S5 — Contrato de adapter documentado.** — *Done when:* doc do contrato
  `ProposedAction`/`GateDecision` + guia "como escrever um adapter" com o adapter de
  referência, para a comunidade contribuir novos.

### Capítulo CI — adiado (só depois do runtime redondo e adotado)
Não começar antes: o CI só vira defensabilidade quando for confiável, e confiável =
diff-scoped. Reintroduzir com honestidade (warn-first; um "fail" é candidato).

- **S1 — Diff-scoping do CI gate.** Hoje o `check` escaneia arquivos inteiros
  (`design.md §5`), contando dívida pré-existente. — *Done when:* `warden check --diff`
  (ou `--base <ref>`) flagueia **só as linhas alteradas** de um PR; whole-file vira
  opt-in; violação mapeada ao hunk. É o maior salto de valor do Capítulo CI.
- **R11 — Anotações do GitHub Actions.** Depende de S1 para não anotar dívida velha.
  — *Done when:* `check` emite `::error file=…,line=…::` para os hits aparecerem inline
  no diff do PR.
- **R1 — `why` no CI report.** O runtime gate mostra o `why`; o `check` ainda não. —
  *Done when:* `warden check` imprime `why` sob cada violação, igual ao
  `render_deny_reason`. `src/report/human.rs`.
- **Score, se justificado** — reintroduzir, mas **diff-scoped** (sobre código novo, não
  dívida). Foi removido por over-claim (`decisions.md §10`); só volta se agregar sinal.

### Backlog
- **R15 — `warden.toml`** para defaults (o análogo do `vercel.json`). Baixa prioridade
  enquanto flags bastam.

## Métricas de sucesso (OSS)

- **Adoção (leading):** `cargo install`/brew, instalações da Action (quando o Capítulo
  CI chegar), ⭐, adapters e rule-packs contribuídos por terceiros.
- **Qualidade:** time-to-first-rule de um usuário novo; nº de linguagens com import
  walker real; (no Capítulo CI) taxa de falso-positivo em run diff-scoped.

## Riscos & mitigação

| Risco | Mitigação |
|---|---|
| **RuleZ adiciona CI + AST** → fosso evapora (já é Rust + YAML) | Chegar **primeiro e mais fundo** no AST agora (S2/S3); e no "PR diff + AST" quando o Capítulo CI vier |
| `matches ≠ violação` mina a confiança | Warn-first + discovery guiado ("confirme a intenção"); no CI, diff-scoping (S1) antes de qualquer score |
| Banda solo → over-scope | Foco: **Fase 0+1 inteiras**, depois **uma** linguagem (S2) e **um** adapter (S4); Capítulo CI e o resto ficam para depois |
| Adapters viram esteira infinita | Preferir **contrato/MCP** (S5) para a comunidade carregar |

## A tese

Hero-path (R7→R9) é a alavanca de **adoção**; loop de autoria (R4+R2) é a de
**retenção**; e o **fosso AST** (S2/S3, agora) + o **CI diff-scoped** (S1, no Capítulo
CI) são a **defensabilidade**. Os três `match.type` formam uma escada de
custo/precisão (`pattern` texto → `imports` o caso especial de imports → `query`
sintaxe/AST arbitrária) — a DX world-class não é achatar a escada, é dar um **loop de
feedback** (R4) e um **ponto de partida por degrau** (R5) para que subir não seja um
salto no escuro.
