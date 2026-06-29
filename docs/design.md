# Warden — Design

A deterministic, agent-agnostic policy engine for AI-agent-generated code.
**One rule format, two consumers.** This document describes what Warden is and
how it's built; [`decisions.md`](decisions.md) records the notable choices and
how the design diverged from the original spec as it was built. For the honest
state of the project (maturity, validated scope, what's next) see
[`conclusion.md`](conclusion.md).

## 1. The core idea

LLMs do not enforce policy — they are *subject* to it. The warden is the
**policy authority**: it checks the agent's output independently of what the
agent "remembered", using deterministic layers where possible and a delegated
semantic layer where judgment is required.

A single YAML rule format feeds two consumers:

- **CI gate** — checks a path (whole files under it), blocks on `block` rules,
  and produces a 0–100 score. It is not diff-scoped (see §5).
- **Runtime gate** — checks one proposed agent action (via a Claude Code hook)
  and returns block/allow.

Both run the same engine over the same rules; only the entry point differs.

## 2. Architecture

```
            rules/*.yaml  (the policy — lives in the consuming project)
                  │
                  ▼
        ┌───────────────────────────────────────────────┐
        │ engine (agent-agnostic core)                    │
        │   load + schema  →  matchers  →  score          │
        │   matchers operate on a list of CodeUnit        │
        └───────────────────────────────────────────────┘
            │                                   │
   one unit = one action            many units = whole files under a path
            ▼                                   ▼
   ┌──────────────────┐                ┌──────────────────┐
   │ runtime gate     │                │ CI gate          │
   │ ProposedAction → │                │ path → score +   │
   │ GateDecision     │                │ exit code        │
   └──────────────────┘                └──────────────────┘
            │
   ┌──────────────────┐
   │ Claude Code adapter (the only agent-specific surface)   │
   │ parse_claude_payload / format_claude_response           │
   └──────────────────┘
```

**The agent-agnostic invariant.** The core only ever sees a `CodeUnit`
(`path` + `content`) on the way in and a `Violation` on the way out — for the
runtime gate, a `ProposedAction` in and a `GateDecision` out. It never knows
which agent called it. Supporting another agent is a new `parse_*`/`format_*`
pair in `adapters/`, with zero core changes. That decoupling is the whole point.

## 3. The rule format

One rule per file in `rules/*.yaml`. The schema is **closed** (`warden
validate` rejects unknown fields) except for the one optional `paths` field:

```yaml
id: no-env-vars                  # unique, kebab-case
description: "..."               # the one-liner shown as file:line — description
why: "..."                       # rationale; also passed to the llm matcher
scope: [ci]                      # subset of: ci, runtime
enforcement: block               # block | warn | audit
weight: 4                        # 1 | 2 | 4   (CI scorer only)
paths: ["src/**"]                # OPTIONAL — scope to a subtree (default: all files)
match:                           # exactly one type
  type: pattern                  # pattern | structural | llm
  # ...type-specific fields
```

The schema is the single source of truth; the [`warden-rule-author`](../skills/warden-rule-author/SKILL.md)
skill teaches an agent to write it correctly.

## 4. Matchers

A matcher is `(units, rule) -> list[Violation]`. Three types, deliberately
ordered cheapest-first in the CI pipeline (pattern → structural → llm):

| type | how | language | notes |
|---|---|---|---|
| **pattern** | regex over each line of a unit, OR-combined flat list | any (regex) | literal/syntactic signals |
| **structural** | **tree-sitter** forbidden imports; `from`/`to` are file-path globs | multi (per-extension: Python, Go, …) | architectural boundaries; unsupported-language or unparseable files skipped |
| **llm** | shells out to `claude -p` headless, strict JSON verdict | any | judgment a regex can't express; defensive (malformed/missing → inconclusive → pass); size-guarded; `--no-llm` skips |

A rule's optional `paths` filters which units a matcher even sees — this is what
lets pattern/llm rules target a subtree, the way structural's `from` glob
already does.

## 5. CI gate (`ci_gate.rs`)

Pipeline: gather units from a path → filter rules whose `scope` contains `ci` →
apply each rule's `paths` filter → run matchers → collect violations → produce
two independent results:

- **Enforcement:** any violated `block` rule → exit 1.
- **Score:** weighted ratio of *passed* rules, scaled to 100:

  ```
  score = Σ(weight of passed scored rules) / Σ(weight of scored rules) × 100
  ```

  Scored set = `ci` rules with enforcement `block` or `warn`. `audit` is logged
  only — excluded from the score, never blocks. Score is binary per rule;
  `extent` (fraction of in-scope files where a rule fired) is recorded for output
  but never weights the score. Bands: 90+ Excellent, 70+ Good, 50+ Fair, <50 Poor.
  With no scored rules, score is 100 (nothing to fail).

Output: `--format human` (default) or `--format json` (the decision record).

**Known limitation — whole-path, not diff-scoped.** "Gather units from a path"
reads *whole files*; the matchers scan every line. The CI gate is **not**
change-scoped: a PR that touches a file with a pre-existing violation on an
unchanged line still has that violation counted against the score. (The runtime
gate *is* inherently change-scoped — it only ever sees the one proposed action's
content.) Diff input for the CI gate is a natural future feature; it is not
implemented.

## 6. Runtime gate (`runtime_gate.rs`) + Claude Code adapter

`evaluate_action(action, rules) -> GateDecision`: filters rules to `scope`
contains `runtime`, reads only `enforcement` (ignores weight/score — meaningless
for one action), applies `paths` (a rule whose paths don't match the action's
path simply doesn't apply), and returns `block`/`allow`.

The Claude Code surface is two translation functions in
[`adapters/claude_code.rs`](../src/adapters/claude_code.rs):

- `parse_claude_payload(stdin_json) -> ProposedAction` — maps a `PreToolUse`
  payload (Write/Edit/Bash; tolerant of field-name variants across CC versions).
- `format_claude_response(decision) -> (stdout, exit_code)` — blocks via the
  JSON permission path (`permissionDecision: "deny"`) and **always exits 0**
  (exit 1 does not block in Claude Code; only the JSON path carries a reason).

Wired via `.claude/settings.json` as a `PreToolUse` hook calling `warden gate`;
`${CLAUDE_PROJECT_DIR}` lets it find the project's own `rules/`.

## 7. Project layout

```
src/
  main.rs                    # CLI (clap): subcommands check, validate, gate
  lib.rs                     # module tree
  schema.rs / load.rs        # the rule format + loader
  glob.rs                    # fnmatch (Python-compatible: * crosses /)
  lang.rs                    # tree-sitter language registry + import extraction
  matchers/{base,pattern,structural,llm}.rs
  score.rs                   # weighted formula + bands
  results.rs                 # RuleResult / CheckResult
  ci_gate.rs                 # path -> violations -> score + exit code
  runtime_gate.rs            # evaluate_action(action, rules)
  adapters/claude_code.rs
  report/{human,json_out}.rs
rules/                       # this repo's own dogfood policy (enforced on src/)
demo/                        # a fake app: before/ (messy) vs after/ (clean) + its own rules/
examples/                    # minimal fixtures used by the gate tests
skills/              # warden-rule-discovery + warden-rule-author
tests/                       # Rust integration tests (the ported suite + a Go case)
```

Note: the **engine ships no default rules**. The warden resolves `--rules <dir>`
→ `$CLAUDE_PROJECT_DIR/rules` → `./rules`. This repo dogfoods itself via its own
root `rules/` (enforced on `src/`); the separate sample policy lives in
`demo/rules/` (the `demo/` app plays the role of an independent consuming
project).

## 8. The rule lifecycle skills

Two skills form a pipeline for authoring policy:

- **[warden-rule-discovery](../skills/warden-rule-discovery/SKILL.md)** —
  investigate a codebase (docs → lint config → structure → grep for evidence +
  extent), propose a prioritized, evidence-backed rule set, and **confirm intent
  with a maintainer before committing** (static signals mislead about intent).
- **[warden-rule-author](../skills/warden-rule-author/SKILL.md)** —
  given a confirmed intent, produce a valid `rules/*.yaml` and validate it.

## 9. How it was validated

- **Rust test suite** (`cargo test`) — the ported Python suite across schema,
  matchers (incl. the structural fire-tests, a Go multi-language case, and the
  llm success/failure paths via a faked `ClaudeRunner`), scoring, both gates, the
  adapter, and `paths`.
- **`demo/`** — `warden check demo/before` scores 0/100 and blocks; `demo/after`
  scores 100/100. `demo/run_demo.sh` walks the whole story; `demo/try_with_claude.sh`
  drives a **real Claude Code session** and confirms the gate denies a
  policy-violating Write end-to-end.
- **Real project** — bootstrapped against `coralogix/eng-pipeline-handler` (zero
  rules): rules derived from the team's `AGENTS.md` conventions, grounded in real
  violation counts, surfaced via `warden check` on 764 files.

## 10. Non-goals and deviations

The original spec was a lean POC blueprint; several choices evolved during the
build (llm transport, per-project rules, the `paths` field, …) and some non-goals
were deliberately crossed. See [`decisions.md`](decisions.md) for the log and the
rationale behind each.
