# Where Warden stands — honest assessment & handoff

Read this first if you're picking the project up cold (new machine, new session).
It records what Warden *is*, the one limit that matters, and what to build next —
the conclusions, not the architecture (that's in [`design.md`](design.md) and
[`decisions.md`](decisions.md)).

## Status

**Early but working** — past the proof-of-concept stage in engineering, not yet
proven at scale or with external users. Written in **Rust** (single static
binary), with a **tree-sitter** structural backend (Python + Go today;
multi-language by design). It runs on real codebases: a read-only `warden check`
over `coralogix/eng-pipeline-handler` scanned **5,645 files in ~3.4s** with zero
parse failures. The original goal — prove the architecture and the runtime-gate
loop end-to-end — is met; what remains is narrowing the gap between "works" and
"trustworthy at scale" (see below). See [`decisions.md §9`](decisions.md) for the
Python→Rust port and [`tree-sitter.md`](tree-sitter.md) for the backend.

## What genuinely works (verified)

- Deterministic, fast, single binary; agent-agnostic core; closed schema;
  dogfoods itself (`rules/` enforced on `src/` via a live PreToolUse hook).
- The tree-sitter structural matcher handled thousands of real `.py` files
  cleanly and is multi-language (a Go test proves the same rule spans languages).
- The **runtime gate** (block/allow on one proposed action) is the strongest
  part — it's inherently change-scoped and deterministic.

## The one limit that matters (structural, not a bug)

**"Matches the pattern" ≠ "is a violation."** The matcher proves a line contains
`cluster.id == "mini01"` with certainty; it cannot say that's *wrong*. The jump
from "matched" to "is a defect" is **intent**, which is human/contextual.

This was made concrete on `eng-pipeline-handler`: the rules' premises were
genuinely anchored in the repo's own `AGENTS.md` "Critical Rules" (a real source
of truth — "prefer feature flags over hardcoded env checks"; "never log above
debug from rendering paths"). Yet static signal still could **not** separate true
defects from legitimate env-specific infra among the 38 hits — because the
convention is "prefer" (soft) and the repo legitimately pins some behavior to
environments. The engine does not, and cannot, supply intent.

## How to position it (don't overclaim)

- **Honest pitch:** "deterministically enforces sharp rules and surfaces a
  grounded candidate population against written conventions, for human
  judgment." **Not** "finds violations."
- **Strong use case = runtime gate on nitid rules** (`import anthropic`, "don't
  touch file X", "no `os.getenv` here"). Determinism + change-scoping + clear
  author intent align; low false-positive cost. This is the killer app, not the
  score.
- **Be humble about the CI score.** It's a trend signal over a candidate
  population, not a verdict: it's not diff-scoped (counts pre-existing
  violations) and each "fail" is a candidate, not a confirmed defect. "Score 50"
  is not precision.
- **Rule quality dominates, not the engine.** On the real repo, the precise
  env-gating rule produced a useful list; the logging rule mis-scoped onto
  `schema_migration/` (an operational job under `render_utils/`, not rendering)
  and produced noise. `paths`, warn-first calibration, and the discovery skill's
  "confirm intent with a human" are load-bearing for exactly this reason.

## What to build next (in priority order)

1. **Diff-scoping for the CI gate.** Today it scans whole files (see
   `design.md §5` "Known limitation"). Focusing on the *changed* lines of a PR is
   where the gate would actually change behavior and stop punishing pre-existing
   debt. This raises real value more than anything else.
2. **Lean into the runtime gate** — it's the use case where the engine's
   guarantees and the user's value line up. More adapters, sharper rules.
3. **The `query` match-type (`.scm` rules-as-data).** The bigger expressiveness
   unlock (Phase 3 in `tree-sitter.md`), but lower priority than (1) — precision
   of *what* you match matters less than *whether you're scoped to new code*.

## Repo state at handoff

- Built and tested: `cargo test` green; `cargo build --release`.
- Dogfood policy in `rules/`; sample/demo policy in `demo/rules/`; `demo/run_demo.sh`
  and `demo/try_with_claude.sh` walk it end-to-end.
- Open calibration item (not acted on — it's another team's repo): the
  `eng-pipeline-handler` `no-elevated-logging-in-render` rule should probably
  exclude `**/schema_migration/**` from its `paths`.
