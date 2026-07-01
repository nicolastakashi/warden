# Warden — Roadmap

Roadmap único do Warden: junta a **estratégia de produto** (posicionamento, fosso,
onde ganhamos) com a **execução de UX/DX** (como cada passo se sente). Se você está
pegando o projeto do zero, leia antes o [`conclusion.md`](conclusion.md) (avaliação
honesta + o único limite que importa), o [`design.md`](design.md) (arquitetura) e o
[`decisions.md`](decisions.md) (escolhas).

Ambição atual: **projeto OSS com tração** — sucesso = instalações, adapters e
rule-packs contribuídos por terceiros, estrelas. Não é produto comercial (nada de
dashboard hospedado) nem só ferramenta pessoal.

> **Status (pós-slim, runtime-first).** O produto agora *é* o runtime gate. A
> codebase foi enxugada (removidos o matcher `llm`, o score ponderado / `weight`
> e o `extent` — `decisions.md §10`), e o **CI gate virou um scan simples** cuja
> sofisticação (diff-scoping, score) fica para um **"Capítulo CI" adiado**. Na
> tabela e nas fases abaixo, portanto: os itens de CI (**S1** diff-scoping, **R1**
> `why` no CI, **R11** anotações, **R10** output do `check`) são o *Capítulo CI —
> depois*; o near-term é **loop de autoria + hero-path + adapters + AST** (runtime).
> O **R14** (marcar `llm` como escape hatch) foi resolvido por remoção. A
> re-sequência completa da tabela ainda está pendente.

## Tese de posicionamento

> **Warden é o único policy-as-code que roda no diff do PR *e* na tecla do agente,
> com a mesma regra YAML ciente de AST.**

O scan competitivo (jul/2026) mostrou que o nicho de *runtime gate* virou disputado —
**Agent RuleZ** (Rust + YAML determinístico, Claude-Code-specific, superfície de
eventos mais rica, sem CI, sem AST) e **det-acp** (TS, proxy MCP/shell, multi-agente
de verdade, sem CI, sem AST). Nenhum dos dois tem **CI gate pontuado** nem inspeção
de **estrutura de código**. Logo, não competimos em "bloquear comando no Claude
Code" — competimos na **interseção CI + runtime + AST**, que é nossa e de mais
ninguém.

**A frase que só nós podemos dizer:** *"a mesma regra que reprova seu PR também
impede o agente de escrever o código — e ela entende imports e AST, não só regex."*

## ICP & job-to-be-done

- **ICP:** tech/staff eng em time que já usa coding agents e sente que "os agentes
  reintroduzem coisas que a gente combinou não fazer".
- **JTBD:** *"quero que uma convenção do time seja aplicada automaticamente — no PR e
  enquanto o agente digita — sem eu virar revisor de guardrail."*
- **Momento de conversão OSS:** um **GitHub Action de uma linha** que comenta inline
  no diff (R11), servido por diff-scoping (S1) para não nascer ruidoso.

## North star

Três alavancas — adoção, retenção e defensabilidade:

- **Hero-path do runtime gate (adoção).** O item mais forte do produto
  (`conclusion.md`) hoje é o passo mais escondido. Colapsar
  `instalar → obter uma regra → conectar o hook → ver bloquear um edit ruim`
  de ~6 passos manuais para ~3 comandos (R7→R8→R9).
- **Loop de autoria (retenção).** Hoje se escreve regra às cegas e só se descobre se
  acertou rodando o gate num fixture. Fechar
  `escrever regra → ver o que ela pega → confiar nela` (R4+R2).
- **Fosso (defensabilidade).** Tornar o CI confiável (S1) e aprofundar o que ninguém
  copia — AST multi-linguagem (S2/S3) e o core agent-agnostic entregue de verdade
  (S4/S5).

A armadilha a evitar: **não** importar "zero-config defaults" literalmente. Warden
não envia regras de propósito (`conclusion.md`: "match ≠ violação, a qualidade da
regra domina, confirme a intenção com um humano"). O onboarding world-class aqui é
**discovery guiado** (as skills `warden-rule-discovery` → `warden-rule-author` já são
essa UX), não regras enlatadas.

## Princípios & não-objetivos

- **Determinístico > probabilístico** — é a marca; o `llm` matcher é escape hatch
  opcional/offline-first (R14).
- **"Candidatos para julgamento humano", não "encontra violações"** — não overclaim.
- **Não** virar gateway/proxy de runtime (jogo do det-acp) — não perseguir
  "enforcement imburlável" agora.
- **Não** inflar a superfície de runtime pra empatar com o RuleZ (inject de contexto,
  N eventos) — nosso valor não está aí.
- Transferimos o **princípio** de DX world-class (Vercel/Resend: time-to-value curto,
  output legível, erros que ensinam), **não a superfície** (nada de dashboard).

## Visão geral (priorizado)

Itens `R*` são de UX/DX; itens `S*` são estratégicos/fosso. Esforço: **S** ≈ horas–1
dia · **M** ≈ poucos dias · **L** ≈ semana+.

| ID | Item | Track | Impacto | Esforço |
|----|------|-------|---------|---------|
| S1 | **Diff-scoping do CI gate** (pontua/bloqueia só linhas alteradas) | Trust | **Maior** (torna o score real; #1 do conclusion.md) | M–L |
| R1 | `why` ausente no CI report | Trust/Polimento | Alto (bug de consistência) | S |
| R11 | Anotações do GitHub Actions no `check` (depende de S1) | Trust/Plataforma | Alto (consumidor CI) | S |
| R4 | `warden test` / dry-run de regra | Autoria | **Maior** (fecha o loop de retenção) | M |
| R2 | `validate` avisa regra morta (casa 0) | Autoria | Alto (mata o pior modo de falha) | S |
| R5 | `warden new-rule --type <t>` (template por tipo) | Autoria | Alto | S |
| R6 | Normalizar o footgun do glob (`src/**`) | Autoria | Médio | S |
| R7 | Binários pré-compilados + `curl\|sh` + Homebrew | Hero-path | **Maior** (corta a barreira de instalação) | M |
| R8 | `warden init` (scaffold mecânico → roteia p/ discovery) | Hero-path | Alto | M |
| R9 | `warden hook install` (merge idempotente no settings.json) | Hero-path | Alto | S |
| S2 | **+1 linguagem (TS/JS) + import walker Rust** | Moat | Alto (multi-lang de verdade) | M–L |
| S3 | **Biblioteca de regras `query` / starter packs** (`npx skills add`) | Moat | Alto (tração + prova não-import-only) | M |
| R12 | Domar `query`: playground + receitas + erro claro de node kind | Moat/Autoria | Alto (destrava o tipo mais poderoso) | M |
| R13 | Reframar taxonomia: `structural`→`imports` + guia de decisão | Moat/Autoria | Médio | S |
| S4 | **2º adapter (Cursor **ou** MCP genérico)** | Reach | Alto (cumpre agent-agnostic) | M |
| S5 | **Contrato de adapter documentado + guia de contribuição** | Reach | Alto (comunidade carrega adapters) | S |
| R3 | Mensagens de erro que ensinam o fix | Polimento | Médio | S |
| R10 | Output legível: cor + símbolos + contagens + timing | Polimento | Médio-alto | S |
| R14 | Marcar `llm` como escape hatch não-determinístico | Posicionamento | Baixo | S |
| R15 | `warden.toml` para defaults (rules dir, format, no-llm) | Backlog | Baixo | S |

## Sequência recomendada

Fases por dependência e por relação impacto/custo, não uma lista chapada. Regra de
ouro: **não traga gente para autoria cega** — autoria e confiança antes de adoção em
massa.

### Fase 0 — Quick wins (barato, envia já)
Corrige inconsistências reais e reaproveita o que já existe.

- **R1 — `why` no CI report.** O runtime gate mostra o `why` (a alternativa
  sancionada); o `check` descarta. Mesma violação ensina o fix no hook e não no CI.
  — *Done when:* `warden check` imprime `why` sob cada violação, igual ao
  `render_deny_reason`. `src/report/human.rs`.
- **R3 — Erros que ensinam.** `error loading rules: {e}` reporta estado sem ensinar.
  — *Done when:* sem rules dir → "nenhuma regra encontrada em `./rules`; rode
  `warden init` ou aponte com `--rules <dir>`". `src/main.rs`.
- **R10 — Output legível.** Hoje `human.rs` é texto puro (a cor do demo vem do bash
  externo). — *Done when:* cor com detecção de TTY + `NO_COLOR`, símbolos `✗/⚠/•`,
  cabeçalho com contagens e timing (`... · 3 files · 1 blocking, 2 warnings · 3.4s`).
  O timing é o superpoder escondido (5.6k arquivos em ~3,4s).

### Fase 1 — "Torna real": loop de autoria (retenção) ∥ Trust (confiança do score)
As duas metades tocam código diferente (subcomandos CLI vs `ci_gate`/parse de diff),
então rodam em paralelo. É a fase que faz o produto valer.

**Loop de autoria — parar de escrever regra às cegas:**

- **R4 — `warden test` / dry-run.** Mostra o que uma regra pega contra um path, com
  `file:line → snippet`, sem commitar (o análogo do "send test email" do Resend). —
  *Done when:* `warden test <rule.yaml> <path>` lista os matches de uma única regra;
  funciona antes de a regra entrar no rules dir.
- **R2 — Aviso de regra morta.** `validate` só checa forma; uma regra com regex
  errado, node kind inexistente ou `paths` que não casa nada valida e silenciosamente
  não faz nada. — *Done when:* `validate --against <path>` reporta `regra X: casou 0
  arquivos/nós` como warning. Reaproveita `ci_gate`.
- **R5 — `warden new-rule --type <t>`.** Emite o template correto por tipo, matando o
  custo dos quatro sub-schemas diferentes. — *Done when:* gera YAML válido comentado
  para `pattern|imports|query|llm`, pronto para `validate`.
- **R6 — Normalizar o glob.** Hoje toda regra path-scoped precisa de `src/**` **e**
  `**/src/**` (footgun em `no-unwrap-in-src.yaml`). — *Done when:* `src/**` casa
  top-level e aninhado, **ou** `validate` avisa "glob casou 0 arquivos". `src/glob.rs`.

**Trust — fazer o score virar sinal real:**

- **S1 — Diff-scoping do CI gate.** Hoje o gate escaneia arquivos inteiros
  (`design.md §5`), contando dívida pré-existente — por isso o score é "sinal, não
  veredito". — *Done when:* `warden check --diff` (ou `--base <ref>`) pontua e bloqueia
  **só as linhas alteradas** de um PR; whole-file vira opt-in; violação é mapeada ao
  hunk. Este é o maior salto de valor do roadmap.
- **R11 — Anotações do GitHub Actions.** O análogo real de "Vercel se integra ao Git"
  para um CI gate; depende de S1 para não anotar dívida velha. — *Done when:* `check`
  emite `::error file=…,line=…::` para os hits aparecerem inline no diff do PR.

### Fase 2 — Colapsar o hero-path (adoção)
Depende da Fase 1 estar boa: não adianta trazer gente para uma autoria cega ou um CI
ruidoso.

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
O que blinda contra o RuleZ/det-acp fecharem o gap. Moat (AST) ∥ Reach (adapters)
rodam em paralelo.

- **S2 — +linguagens + import walker Rust.** "Multi-language" tem que ser verdade,
  inclusive na própria casa (hoje `structural` não acha nada em `.rs`). — *Done when:*
  gramática + extrator de imports para TS/JS em `src/lang.rs`; import walker para Rust;
  um teste provando a mesma regra spanning linguagens.
- **S3 — Biblioteca de regras `query` / starter packs.** Conjunto curado de regras
  `.scm` por ecossistema (no-unwrap, no-console.log, forbidden-imports comuns),
  instalável via `npx skills add`. Conteúdo = tração e prova que não é import-only. —
  *Done when:* pacote de regras por linguagem documentado e instalável.
- **R12 — Domar `query`.** Hoje expõe tree-sitter cru (node kinds da gramática) —
  expert-only. — *Done when:* `warden query try '<scm>' <file>` para iterar; receitas
  por linguagem; erro claro quando o node kind não existe na gramática.
- **R13 — Reframar taxonomia.** `structural` só faz forbidden-imports mas soa geral;
  `query` é o que de fato é estrutural. — *Done when:* `structural` renomeado para
  `imports` (schema é POC, sem backward-compat) + guia de decisão "pattern vs query vs
  llm" no `--help`.
- **S4 — 2º adapter.** Prova o design bet ("novo `parse_/format_`, zero mudança no
  core"). — *Done when:* segundo par de tradução (Cursor **ou** contrato genérico via
  MCP) com teste, sem tocar o core.
- **S5 — Contrato de adapter documentado.** — *Done when:* doc do contrato
  `ProposedAction`/`GateDecision` + guia "como escrever um adapter" com o adapter de
  referência, para a comunidade contribuir novos.

### Fase 4 — Posicionamento & backlog
- **R14 — `llm` como escape hatch.** Quebra a promessa de determinismo; docs já
  empurram para longe dele. — *Done when:* marcado explicitamente como
  experimental/não-determinístico no schema e no output de `validate`.
- **R15 — `warden.toml`** para defaults (o análogo do `vercel.json`). Baixa prioridade
  enquanto flags bastam.

## Métricas de sucesso (OSS)

- **Adoção (leading):** instalações da Action, `cargo install`/brew, ⭐, adapters e
  rule-packs contribuídos por terceiros.
- **Qualidade (trust gate):** taxa de falso-positivo em run diff-scoped; time-to-first-
  rule de um usuário novo; nº de linguagens com import walker real.

## Riscos & mitigação

| Risco | Mitigação |
|---|---|
| **RuleZ adiciona CI + AST** → fosso evapora (já é Rust + YAML) | Chegar **primeiro e mais fundo** no "PR diff + AST"; dominar a narrativa já no R11 |
| `matches ≠ violação` mina a confiança no score | Diff-scoping (S1) + warn-first + packs calibrados (item aberto do `schema_migration/`) |
| Banda solo → over-scope | Corte de 90 dias: **Fase 0+1 inteiras**, **uma** linguagem (S2), **um** adapter (S4); o resto é Later |
| Adapters viram esteira infinita | Preferir **contrato/MCP** (S5) para a comunidade carregar |

## A tese

Hero-path (R7→R9) é a alavanca de **adoção**; loop de autoria (R4+R2) é a de
**retenção** — e **S1 (diff-scoping)** é a de **defensabilidade**, pois transforma o
consumidor que só nós temos (CI) num sinal confiável, enquanto **S2–S5** (AST
multi-linguagem + adapters) são o fosso que RuleZ/det-acp não têm. Os quatro
`match.type` formam uma escada de custo/precisão (`pattern` texto → `query` sintaxe →
`llm` semântica; `imports` é o caso especial de imports) — a DX world-class não é
achatar a escada, é dar um **loop de feedback** e um **ponto de partida por degrau**
para que subir não seja um salto no escuro.
