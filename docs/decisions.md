# Design decisions & deviations

A log of the notable choices made while building the POC, including where the
implementation deliberately diverged from the original lean spec. Each entry is
*what* changed and *why*, so the divergence is auditable.

> Framing: Warden started as a local-only proof-of-concept and has grown into an
> **early but working tool** (still local/single-user — see
> [`conclusion.md`](conclusion.md)). Backward compatibility is not a concern, and
> the original spec's non-goals are starting guidance, not law — we deviate when
> it improves the design, and note it here. (Entries below say "POC" where they
> describe that early phase historically.)

## 1. Language & packaging — Python first, then ported to Rust

> **Superseded by §9.** The POC was first built in Python; it has since been
> **ported to Rust**. This entry records the original rationale.

Python for fastest prototyping and because the `ast` stdlib handles the
structural matcher with no extra parsing dependency. Shipped as one console
script (`warden`) installable via `uv tool install` / `pipx`. The Claude Code
hook just invokes the CLI, so the Python warden is fully compatible with the
Node-based agent.

## 2. `llm` matcher transport — Claude Code CLI, not the Anthropic SDK

**Original:** the spec implied calling Claude via the API SDK.
**Now:** the `llm` matcher shells out to the local `claude` CLI in headless mode
(`claude -p --output-format json --max-turns 1 --system-prompt …`).

**Why:** it rides the developer's existing Claude Code OAuth login — **no
separate `ANTHROPIC_API_KEY`** — and keeps the warden on the same toolchain it
polices. We verified `-p` is still current (it is; `--bare` is becoming the
scripted default but requires an API key, so we stayed on plain `-p` for the
no-key property). Verified the JSON envelope shape against the real binary.
**Tradeoff:** the `claude` CLI must be installed + logged in wherever the `llm`
layer runs (CI included); `--no-llm` keeps everything else fully offline.

## 3. Rules are per-project — the engine ships none

**Original spec layout:** `rules/` at the warden root.
**Now:** the warden wheel packages only `src/warden`; there are **no bundled
default rules**. The CLI resolves `--rules <dir>` → `$CLAUDE_PROJECT_DIR/rules`
→ `./rules`, i.e. the *consuming project's* policy. With nothing found it errors
— there is no fallback.

**Why:** the warden is the *engine*; the policy belongs to each project. The
repo's own sample policy was moved to `demo/rules/` — the demo plays the role of
a consuming project rather than the engine pretending to own a default policy.

## 4. Added the optional `paths` field — crossed the closed-schema non-goal

**Original non-goal (§8):** no fields beyond the lean schema; schema closed.
**Now:** rules accept an optional `paths:` list of file-path globs. Absent =
applies to all files (so it's not compat baggage — repo-wide is a sensible
default). Present = the rule only applies to matching files.

**Why:** bootstrapping a real project (`eng-pipeline-handler`) surfaced that
`pattern`/`llm` rules had no way to scope to a subtree, while `structural`'s
`from` glob did. Conventions like "no logging above debug **in rendering
paths**" or "no env gating **in feature code**" were inexpressible — you'd have
to drop to `audit` to hide the noise from legitimate boundary code. `paths`
makes pattern/llm rules as precise as structural, and is applied in both gates
(in CI it also makes `extent` relative to the scoped files; in the runtime gate
a non-matching path means the rule doesn't apply).

## 5. `llm` size guard

The `llm` matcher skips (inconclusive → pass + warning) when the combined input
exceeds ~200k chars. **Why:** `llm` rules are for a diff or a scoped subtree, not
a whole repo — a repo-wide run would build an enormous prompt. The guard nudges
toward `paths` / a narrower target / a diff instead of silently sending it.

## 6. Language-agnostic matchers — explored, reverted, deferred

We briefly started making the engine language-agnostic (a Go import extractor +
language-agnostic file ingestion). **Reverted** at the user's direction — that
reverted *premature execution*, not the goal. The natural extension points
if/when we revisit: a per-language dispatch in the `structural` matcher and the
file-ingestion filter in `ci_gate.gather_units`.

The principled path forward is designed in [`tree-sitter.md`](tree-sitter.md):
replace the `ast` structural backend with tree-sitter so structural matching
becomes **rules-as-data** (`.scm` queries in the rule, not match-kinds hardcoded
in the engine) and **multi-language** in one move — with the honest test of
"could `ast` already do this?" applied to every claimed benefit.

## 7. Two skills for the rule lifecycle

Authoring rules is two distinct jobs, so it's two skills: **discovery**
(investigate a codebase → propose evidence-backed rules → confirm with a
maintainer) feeding **author** (write the validated YAML). The discovery skill's
cardinal rule — *propose and confirm, never guess the intent* — came directly
from this build: an agent twice proposed the wrong rule (`os.getenv`) when the
real anti-pattern was gating features on `cluster.id`/`env_type`; only the
maintainer's domain knowledge corrected it. Static signals find the symptom
population; the human supplies the intent.

## 8. Conservative calibration by default

Rules for a zero-rules project start at `warn`/`audit`, scoped with `paths`, and
only promote to `block` once existing violations are cleaned up. **Why:** a wall
of blocking failures on day one gets the gate disabled. Surface first, block
later.

## 9. Ported from Python to Rust + tree-sitter

The POC was rebuilt in **Rust**, and the `structural` matcher's parser swapped
from the Python `ast` stdlib to **tree-sitter**. Both moves landed together,
on purpose.

**Why now, and why together.** Two decisions forced it (see
[`tree-sitter.md`](tree-sitter.md) and the project memory): (1) Warden must run
on **any codebase**, not just Python — so the structural backend had to become
multi-language, which `ast` can never be; (2) tree-sitter is a C-with-bindings
library available everywhere, so the *one* reason the engine was in Python — the
free, built-in `ast` — evaporated. With that gone, the better fit for a
hook-installed CLI is a **single static binary**, and of the compiled options
Rust has the cleanest tree-sitter story (canonical crate, grammars compiled by
cargo, no CGo tax). Doing the port *before* writing any tree-sitter code in
Python avoided building a backend twice.

**What changed.** Crates: `clap` (CLI), `serde` + `serde_norway` (YAML; the
closed schema is validated by hand against a generic `Value`, as before),
`serde_json`, `regex`, a hand-rolled `fnmatch`→regex translator that matches
Python semantics exactly (`*` crosses `/`; `src/glob.rs`), and the `glob` crate
only to expand a glob check target. Structural parsing: `tree-sitter` +
`tree-sitter-python` + `tree-sitter-go`. The `llm` matcher shells out to `claude` via
`std::process::Command` behind a `ClaudeRunner` trait (faked in tests). The
56 Python tests were reimplemented as Rust integration tests (`tests/*.rs`),
plus a Go case proving the structural matcher is genuinely multi-language.

**Behavioral parity, with two deliberate notes.** The structural matcher still
skips relative imports and still fail-opens on files that don't parse (now via
tree-sitter's `has_error`) — same outcomes as the `ast` version, so the ported
tests pass unchanged. The dogfood rules were re-authored for the Rust tree
(`core-stays-agent-agnostic` now greps for `crate::adapters`;
`no-direct-anthropic-api` replaces the old `import anthropic` structural rule).

**What's next (not done here).** The bigger tree-sitter unlock — a
`match.type: query` that puts a `.scm` query in the rule as data — is still
future work (Phase 3 in `tree-sitter.md`). This entry covers the backend swap
and the multi-language forbidden-imports, not the query DSL.

## 10. Slimmed to runtime-first — removed the `llm` matcher, the weighted score, and `extent`

A deliberate scope cut: the runtime gate is the strongest, most-defensible part
of Warden (`conclusion.md`), and a competitive scan (jul/2026) showed the
runtime-gate niche is now contested while **nobody else pairs a CI gate with
AST-aware rules**. So the near-term product *is* the runtime gate; the CI gate
stays as a simple offline scan and its sophistication is deferred to a later
"CI chapter" (`docs/roadmap.md`). Three things were removed:

- **The `llm` matcher.** It contradicted the core thesis (determinism is the
  brand) *and* the one limit that matters — it was the engine *pretending to
  supply intent*, which `conclusion.md` says the engine cannot and should not
  do. On the runtime path it also meant shelling out to `claude` per action:
  latency plus a non-deterministic *block*. The repo's own rules never used it
  (only a demo rule did). Gone: `matchers/llm.rs`, the `llm` match type, the
  `ClaudeRunner` plumbing, and the `no_llm` flag/param throughout.
- **The weighted 0–100 score + `weight` + bands.** We built a weighted score and
  then concluded it is "a signal, not a verdict" — premature precision on a
  consumer we're deferring, and an unreliable number is worse than none. `warden
  check` now reports which rules fired (`file:line → snippet`) plus
  blocking/warning/audit counts, and exits 1 on any violated `block` rule. Gone:
  `score.rs`, the `weight` field, `band`.
- **`extent`.** Dead computed state — recorded but never used to weight the
  score (which itself is now gone).

**Not removed:** the CI gate itself (`warden check`) — removing it would collapse
Warden into the runtime-only category the competitors already own and foreclose
the future CI+AST moat. The `query` matcher stays and is central (the sharp
AST rules it expresses are exactly the runtime gate's killer app).

**Blast radius was ~zero:** single-user, local-only, no external consumers, so
the schema change (dropping `weight`) and rule rewrites cost nothing (backward
compatibility is not a concern for this POC). The one thing to reintroduce when
the CI chapter starts is scoring — but diff-scoped (see `conclusion.md`), so it
scores *new* code, not pre-existing debt.

## 11. `validate --against` — 0 files is a warning, 0 hits is not

R2 (`warden validate --against <path>`) dry-runs the rule set over a path to
catch a *dead rule* before it lands. The roadmap phrased the signal as "casou 0
arquivos/nós" (0 files *or* 0 nodes), but we deliberately split those:

- **`paths` matched 0 files → `⚠` (actionable).** The rule literally cannot fire
  here; almost always a glob mistake (the `src/**` vs `**/src/**` footgun, R6).
- **0 hits on N scanned files → neutral, not a warning.** A *correct* rule over
  *clean* code matches 0 — that's the healthy, expected state. Flagging it would
  cry wolf on every well-behaved rule and train the user to ignore the output.

We can't cheaply tell "clean code" from "rule doesn't match what you think", so
the `0 hits` line stays informational and points at `warden test` to inspect.
`--strict` makes only a *genuine* dead rule exit 1 (0 files while the target
*has* files); an empty/mistyped target is a bad path, not a dead rule, so it
prints "no files found" and exits 0 like `warden check` — it never trips
`--strict`. The default (no `--strict`) is advisory. Engine is
`ci_gate::coverage` (gathers files once), reused from `run_rule` (R4).
