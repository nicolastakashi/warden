# Design: a tree-sitter structural backend

> **Status (implemented).** The backend swap, multi-language forbidden-imports,
> **and the `match.type: query` rules-as-data DSL are done.** The Rust port runs
> the `structural` matcher on tree-sitter (Python and Go wired in `src/lang.rs`),
> the `ast` backend is gone (see [`decisions.md §9`](decisions.md)), and a
> `query` match type embeds a `.scm` query in a rule (§5b below) — Python, Go,
> and Rust, compiled at `validate` time, dogfooded by `rules/no-unwrap-in-src.yaml`.
> The design differs from §5b in one detail: rather than a reserved `@violation`
> capture, **every** captured node is a violation (simpler, and lets a query
> capture with any name). The rest of this doc is kept as the design record.

A plan for replacing the `structural` matcher's parser and turning structural
matching into a **rules-as-data** capability. It assumes the framing in
[`decisions.md`](decisions.md): a local-only, single-user tool, **no backward
compatibility to preserve** — we can replace the structural backend outright
rather than carry two.

## 0. Verdict — is tree-sitter better *for us*?

The decision hinged on one question — *will Warden ever check code that isn't
Python?* — and it is now **answered: yes.** Warden is meant to run on **any
codebase**. That settles it: **tree-sitter is the path, and the only clean one.**
The two benefits that survive the §2 "could `ast` already do this?" test —
multi-language and rules-as-data — are precisely Warden's stated identity
("agent-agnostic," "rules are data"), and the `ast` stdlib is Python-only by
construction, so it can never get there. The native-wheel dependency cost (§8) is
accepted as the price of that identity.

**What's settled vs. what's sequencing.** The *direction* is decided — we build
it. The *order* still follows the rollout in §7, because the smart way to adopt a
backend swap is to prove it can't regress before extending it:

1. **Phase 0–1 first** — add the dependency and re-back `structural` on
   tree-sitter for Python, with the existing structural tests staying green
   *unchanged*. This proves the swap with zero behavior change before any new
   surface area.
2. **Then Phase 2+** — light up the other languages and the `query` match type.

So: not "build later," but "build in the safe order, starting now." The rest of
this doc is *why* tree-sitter is the right tool and *how* each phase goes.

## 1. The gap today

The `structural` matcher ([`match/structural.py`](../src/warden/match/structural.py))
is the one piece of the engine that is both **language-locked** and
**check-locked**:

- It uses the Python `ast` stdlib → **Python files only**; every other language
  is silently skipped.
- It supports exactly **one check kind**: forbidden imports (`from`/`to` globs).
- Crucially, that check kind is **engine code, not data**. Adding a second
  structural check (say, "no `cluster.id ==` comparisons") means writing a new
  matcher in `src/warden`, with tests and a release — even though the *policy* is
  what changed, not the engine.

That last point is the real one. Warden's identity is **rules are data, the
engine is fixed** — the whole project is "one rule format, two consumers." The
structural matcher quietly violates that: it's the one place where expressing a
new policy means changing the engine.

## 2. The test that disciplines this doc

Tree-sitter is a heavy thing to adopt (a native-wheel dependency). So every
claimed benefit gets run through one question: **could the Python `ast` stdlib
already do this?** If yes, it's an argument for *enriching the existing matcher*,
not for tree-sitter.

| Candidate benefit | `ast` already does it? | Survives? |
|---|---|---|
| Richer Python queries (`Compare`, `Call`, decorators, defs…) | **Yes** — `ast` exposes the full tree | ✗ not a tree-sitter argument |
| Move checks from `llm` → deterministic structural (for Python) | **Yes** — a richer `ast` matcher does this | ✗ not unique to tree-sitter |
| Structural checks as **data in a rule**, not new engine code | **No** — `ast` walks are Python written in `src/warden` | ✓ |
| **Multi-language** structural matching | **No** — `ast` is Python-only | ✓ |
| **Error-recovery** parsing of half-written edits | **No** — `ast.parse` throws `SyntaxError` and the file is skipped | ✓ (situational) |

Three benefits survive. They are the thesis. The two that don't survive are real
improvements — but they're reasons to *enrich the matcher*, and we should be
honest that tree-sitter is not required for them (see §6).

## 3. The thesis — three unlocks

**1. Rules-as-data (the spine).** Tree-sitter ships a declarative query language
(S-expression `.scm` patterns) for matching syntax-tree shapes. That query is a
**string** — it lives in the YAML rule, not in `src/warden`. So structural
matching stops being "a fixed list of match-kinds the engine hardcodes" and
becomes "the rule author writes the structural pattern as data," exactly like
`pattern` rules carry regexes. A new structural policy becomes a new *rule*, not
a new *matcher*. This is the unlock that aligns with the rest of Warden.

**2. Multi-language.** Tree-sitter has maintained grammars for ~all mainstream
languages. One structural backend parses Python, Go, TS/JS, Rust, Java… The
engine finally makes good on "agent-agnostic" at the structural layer — which
matters because the agents Warden polices generate polyglot codebases.

   On the earlier revert: we [reverted a half-baked Go import
   extractor](decisions.md) (§6) — that was reverting *premature execution*, not
   abandoning the goal. Multi-language is the headline value of tree-sitter, and
   this doc is the principled path to it: one parsing layer, grammars as data,
   instead of a bespoke per-language extractor.

**3. Error-recovery.** Tree-sitter produces a usable tree even for syntactically
broken or partial input; `ast.parse` raises and we skip the file (fail-open).
Situational — the runtime gate usually sees a *complete intended file* in the
Write payload, so this matters most for diff/partial inputs — but it removes a
class of silent skips.

**The consequence (not a fourth unlock):** because structural checks become
expressive *and* data, work that today can only be expressed by the fuzzy,
non-deterministic, billable `llm` matcher can move **down** into the
deterministic, free, fast structural layer. The `llm` matcher goes back to being
the fallback for genuinely fuzzy intent — not the crutch for "a regex can't see
structure." That is a consequence of unlock #1, not a property unique to
tree-sitter.

## 4. What tree-sitter is (and why it fits)

A parser-generator + runtime: per-language grammars compile to fast,
**incremental**, **error-recovering** parsers, all driven through one C API
(Python bindings: `py-tree-sitter`). It exposes a **query language** — `.scm`
files of S-expression patterns with captures (`@name`) and predicates
(`#match?`, `#eq?`) — for matching tree shapes declaratively.

Why it fits Warden specifically: it is **deterministic** (same input → same
tree → same matches), which is the engine's whole identity; its grammars and
queries are **data**, which is the project's whole shape; and it is one
dependency that covers every language instead of one extractor per language.

## 5. Design — where it plugs in

**The core contracts don't move.** A matcher stays `(units, rule) -> list[Violation]`;
`CodeUnit` / `ProposedAction` / `GateDecision` are untouched. Tree-sitter is an
internal backend plus one new match type. Both gates, scoring, the closed-schema
discipline, and the adapter are unaffected.

Two surfaces, kept distinct so the easy case stays easy:

**(a) `structural` keeps its friendly sugar, re-backed by tree-sitter.** The
`forbidden: [{from, to}]` import form stays — authors don't write `.scm` for the
common case. Internally it compiles to a per-language import query. Because the
parser is now tree-sitter, the *same rule* applies across languages: language is
inferred per unit from file extension (`.py`→python, `.go`→go, `.ts`→typescript…),
with an optional `language:` override.

```yaml
# unchanged on the surface — now matches Python AND Go AND TS
match:
  type: structural
  forbidden:
    - from: "**/billing/**"
      to: "**/notifications/**"
```

**(b) a new `match.type: query` for full power.** The rule embeds a tree-sitter
query as data. A reserved capture — `@violation` — marks the node whose location
becomes the `Violation`; the rule's `description` is the message.

```yaml
match:
  type: query
  language: python          # required — a .scm query is grammar-specific
  query: |
    ((comparison_operator
       (attribute attribute: (identifier) @attr))
     @violation
     (#eq? @attr "id"))     # flags `... .id == ...` style env gating
```

This is the rules-as-data unlock made concrete: the policy above is a *rule*, and
shipped zero engine code.

**Schema impact** (closed schema, fields added deliberately — no compat shims):

- `structural` gains an optional `language` override (default: infer per unit).
- new `query` match type: `language` (required), `query` (scm string), with the
  `@violation` capture convention; `description` supplies the message.

The honest cost lives here: a `.scm` query is **grammar-specific** — node names
(`comparison_operator`, `import_from_statement`, …) come from each language's
grammar, so authoring queries has a real learning curve. Mitigations in §8:
keep the `forbidden` sugar for the common case, ship a query cookbook, and teach
the [`warden-rule-author`](../skills/warden-rule-author/SKILL.md) skill.

## 6. Alternative considered — just enrich the `ast` matcher

The fair rebuttal: *most of §3's value is "richer structural checks," and Python
`ast` already exposes the whole tree — so enrich the existing matcher and skip a
native dependency.* For a **Python-only** tool that would be right, and it's
cheaper. It loses on exactly the two axes that survived the §2 test:

- **Rules-as-data.** An `ast`-based richer matcher still expresses each new check
  as Python *in the engine*. You'd grow a library of match-kinds in `src/warden`
  — the opposite of the unlock. Tree-sitter's `.scm` puts the pattern in the rule.
- **Multi-language.** `ast` is Python-only by construction. No amount of
  enriching reaches Go or TS.

So the dependency buys precisely the two things `ast` can't give, and we should
say so rather than over-claim the rest.

> **Accuracy note (don't over-claim).** The dogfood rule
> [`core-stays-agent-agnostic`](../rules/core-stays-agent-agnostic.yaml) is a
> regex today because the *current* structural matcher **deliberately skips
> relative imports** (`structural.py`: `if node.level > 0: continue`) — that is a
> policy choice, **not an `ast` limitation**; `ast` can see relative imports
> fine. The real win is that a `query` rule lets us *include* relative imports
> trivially and retire that regex (see §7, Phase 3) — a clean proof point, stated
> for the right reason.

## 7. Rollout

No backward compatibility to preserve, so we **replace** the structural backend
rather than run two in parallel — no dual-backend, no "fall back to `ast` if the
grammar wheel is missing." One backend, swapped.

- **Phase 0 — dependency + spike.** Add `tree-sitter` + `tree-sitter-language-pack`
  (see §8). Spike: parse a Python and a Go file, run one query, confirm the
  API/ABI pairing on the dev machine.
- **Phase 1 — parity swap (Python).** Re-implement `forbidden`-imports on
  tree-sitter for Python. Bar: the existing structural fire-tests stay green,
  unchanged. This proves the backend swap with zero behavior change.
- **Phase 2 — light up multi-language.** Extend `forbidden`-imports to Go and
  TS/JS via the extension→grammar map; add fixtures per language. This is where
  the §3 multi-language unlock lands, on the *existing* rule surface.
- **Phase 3 — introduce `match.type: query`.** Add the new match type and the
  `@violation` capture convention. Proof point: migrate `core-stays-agent-agnostic`
  from regex to a query that catches relative **and** absolute adapter imports
  structurally, and retire the regex.
- **Phase 4 — teach authoring.** Add a `.scm` query cookbook and extend the
  `warden-rule-author` skill so agents can write `query` rules, not just sugar.

Each phase is independently shippable and leaves the engine green.

## 8. Packaging & dependency

- **`tree-sitter`** (py-tree-sitter) — the runtime/bindings. Current line is
  **0.25.x**.
- **`tree-sitter-language-pack`** — one wheel bundling ~all grammars as prebuilt
  binaries, exposing `get_language(name)` / `get_parser(name)`. Current line
  **0.7.x** (Apr 2025+). It is the **maintained** successor to the now-unmaintained
  `tree-sitter-languages` — use the language-pack, not the old one.

Decisions:

- **Core dependency, not optional.** Structural is a core feature being rebuilt
  on tree-sitter, so the parser is a core dep — `pyyaml` gets company. We drop
  the "lean base wheel" aim for the structural layer; the `llm` matcher stays
  dependency-free (it still just shells out to the `claude` CLI).
- **Pin the ABI pair.** `py-tree-sitter` and the grammar wheels are coupled by a
  C ABI; a mismatch fails at parse/load time. Pin both to compatible ranges and
  bump them together.
- **Cost, stated plainly:** native wheels add tens of MB and platform-specific
  build/install surface vs. today's pure-Python `ast`. That is the price of
  unlocks #2 and #3; for a Python-only tool it wouldn't be worth it (§6).

## 9. Risks & tradeoffs

- **Query authoring is harder than globs.** Grammar-specific node names are a
  learning curve. Mitigated by keeping the `forbidden` sugar, the cookbook, and
  the skill (§8/§5).
- **Grammar/ABI version drift.** Node names and ABIs can shift across grammar
  releases, which can break a shipped `.scm`. Pin grammars; treat a grammar bump
  like a dependency upgrade with test coverage.
- **Dependency weight** (§8) — accepted, scoped to the structural layer.
- **Not a risk: determinism.** Tree-sitter is fully deterministic, so this
  *strengthens* the deterministic core rather than diluting it — unlike leaning
  harder on the `llm` matcher.

## 10. What stays the same

Core contracts (`CodeUnit` / `Violation` / `ProposedAction` / `GateDecision`),
the matcher signature, both gates, the scoring formula, the runtime gate's
JSON-`deny` blocking, the Claude adapter, and the closed-schema discipline. This
is a backend swap plus one new match type — **not** an engine redesign. The
engine stays fixed; the policy stays data — which is the entire point of doing it
this way.
