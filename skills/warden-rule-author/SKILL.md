---
name: warden-rule-author
description: >-
  Author Warden rules — the rules/*.yaml files the `warden` CLI enforces
  as a CI gate (scored) and a runtime gate (block/allow on a proposed agent action).
  Use this whenever someone wants to add, write, create, or edit a warden policy
  rule, or turn a coding convention or a "ban/avoid/forbid/require/warn on X"
  guideline into an enforceable rule. Trigger even when the intent is described
  without naming the rule file — e.g. "block direct env-var access", "warn when
  we log PII", "forbid billing from importing notifications", "flag TODOs in
  prod code". Also use when asked how the rule schema works (id, scope,
  enforcement, weight, or the pattern / structural / llm match types). Always
  finish by running `warden validate` so the rule is proven valid.
compatibility: "Requires the warden CLI; llm-rule dry-runs also need the claude CLI."
---

# Authoring Warden rules

The warden is the policy authority: it checks code independently of what an
agent "remembered". A rule is a single YAML file. You are producing **one file,
one rule**, dropped into the project's rules directory, and then proven valid
with `warden validate`.

The schema is **closed** — there are no fields beyond the ones below, and
`warden validate` rejects unknown fields. This is deliberate: the format is the
single source of truth, kept small so rules stay readable and reviewable. Resist
inventing fields.

## The schema (every field is required)

```yaml
id: no-env-vars                  # unique, kebab-case
description: "Use feature flags instead of environment variables"
why: "Direct env access bypasses the flag system and causes config drift."

scope: [ci]                      # subset of: ci, runtime
enforcement: block               # block | warn | audit
weight: 4                        # 1 | 2 | 4   (CI scorer only)

match:                           # exactly one type
  type: pattern                  # pattern | structural | llm
  # ...type-specific fields (see below)
```

- **id** — kebab-case, unique across all rule files. The filename should match
  (`<id>.yaml`) so rules are easy to find.
- **description** — the one line developers see in output as `file:line —
  description`. Make it an actionable instruction, not a restatement of the id.
- **why** — the rationale. It is shown to humans *and* passed to the `llm`
  matcher as context, so write it to explain the actual risk.

## scope — which consumer evaluates the rule

- `ci` — the CI gate checks a diff/path, scores it 0–100, and can block.
- `runtime` — the runtime gate checks **one** proposed agent action
  (e.g. a Write/Edit/Bash) via a hook and returns block/allow. It reads only
  `enforcement`; it ignores `weight` and the score (meaningless for one action).
- A rule may be in both (e.g. the critical env-var rule). Put it in `runtime`
  only when it makes sense to judge a single action against it — `pattern` and
  `llm` do; a cross-file `structural` import rule usually does not.

## enforcement — what a violation does

- `block` — fails the gate. CI exits 1; runtime returns `deny`. Use for hard
  rules you want to stop.
- `warn` — never blocks, but **counts against the CI score** (lowers it).
  Runtime surfaces it but still allows. Use for "should fix".
- `audit` — logged only: excluded from the score, never blocks. Use to observe
  adoption of a new convention without penalizing anyone yet.

## weight — 1 | 2 | 4 (CI score only)

The CI score is a weighted ratio of rules that passed:

```
score = Σ(weight of passed scored rules) / Σ(weight of scored rules) × 100
```

Only `block` and `warn` rules are scored; `audit` is excluded. Weight sets how
much a failure hurts the score: **4** = critical, **2** = moderate, **1** =
minor. The runtime gate ignores weight entirely.

## paths — optional, scope the rule to a subtree

```yaml
paths: ["**/platform_features/**", "src/api/**"]   # optional; file-path globs
```

By default a rule applies to every checked file. Add `paths` (a list of
file-path globs, `fnmatch`, `*` crosses `/`) to restrict it to matching files —
this is what lets a `pattern` or `llm` rule target a subtree, the way
`structural`'s `from` glob already does. Use it when a convention only holds in
certain paths: e.g. "prefer feature flags over env access **in feature/render
code**" (boundary config legitimately reads env), or "no logging above debug
**in rendering paths**". In the CI gate `extent` is then relative to the scoped
files; in the runtime gate, a rule whose `paths` don't match the action's path
simply doesn't apply. This is the one optional field — everything else is
required, and the schema is otherwise closed.

## The three match types — pick the simplest that captures the intent

Reach for the cheapest layer that works: `pattern` and `structural` are
deterministic, fast, and run offline; `llm` costs a Claude call. Use `llm` only
when the rule genuinely needs judgment a regex or import graph can't express.

### pattern — regex over changed lines (language-agnostic)

```yaml
match:
  type: pattern
  patterns: ['os\.getenv', 'os\.environ']   # flat list, OR-combined
```

Any line matching any pattern is a violation, reported at that line. Patterns
are Python regular expressions — escape literal dots (`os\.getenv`). The list is
flat and OR-combined; there are no per-language maps. Best for literal or
syntactic signals (a banned call, a forbidden token, a TODO marker).

### structural — forbidden imports via tree-sitter (multi-language)

```yaml
match:
  type: structural
  forbidden:
    - from: "src/billing/**"
      to: "src/notifications/**"
```

For architectural boundaries. `from` and `to` are **file-path globs** (matched
with `fnmatch`, where `*` crosses `/`, so `src/notifications/**` matches
`src/notifications/email`). A file whose path matches `from` may not import a
module whose path matches `to`. The language is inferred from the file extension
(Python and Go today); files in unsupported languages, or that don't parse, are
skipped. List multiple edges under `forbidden`.

Globs are matched against the path **as the warden sees it** — relative to
whatever you point `check` at. `services/payments/**` matches
`services/payments/charge.py` only when the path starts there. If you can't rely
on where `check` runs (or want it to match at any depth), lead with `**/`, e.g.
`**/payments/**`. The `to` glob matches the **imported module** as a slash path
(`services.analytics.metrics` → `services/analytics/metrics`).

**Footgun — matching a package takes several edges; the import is a slash-path
with no leading slash.** For a package `foo`, the candidate paths differ by how
it's imported, and each needs its own `to` glob:

| import | candidate | glob that matches |
|---|---|---|
| `import foo` | `foo` | `to: "foo"` (exact) |
| `from foo.bar import x` / `import foo.bar` | `foo/bar` | `to: "foo/**"` |
| nested `app.foo.bar` | `app/foo/bar` | `to: "**/foo/**"` |

So `**/foo/**` (a leading `**/` requires a segment *before* `foo`) matches the
**nested** case only — **not** a top-level `import foo` or `from foo.x`. To fully
forbid the `foo` package, give it all three edges (`foo`, `foo/**`,
`**/foo/**`). This is easy to get wrong and fails *open* (silently no violation),
so always dry-run `warden check` against a real offending import to confirm the
rule actually fires.

### llm — semantic check delegated to Claude

```yaml
match:
  type: llm
  prompt: "Flag any logging or print statement that includes personally identifiable information such as email addresses, full names, phone numbers, or government IDs."
```

The warden sends `why` + `prompt` + the changed code to Claude (headless
`claude -p`) and expects a strict JSON verdict. Anything malformed, or `claude`
being unavailable, is treated as **inconclusive → pass** (and the layer is
skippable with `--no-llm`), so an `llm` rule must never be the only thing
standing between you and a critical violation — pair critical checks with a
deterministic `pattern`/`structural` rule too. Write the prompt as one precise,
single-criterion instruction; vague prompts produce noisy verdicts.

## Workflow

1. **Translate the intent into the smallest match type.** Literal token or call
   → `pattern`. Import/dependency boundary → `structural`. Needs judgment →
   `llm`.
2. **Fill the metadata.** Pick a kebab-case `id`, an actionable `description`,
   and a `why` that states the real risk.
3. **Set scope / enforcement / weight.** Hard stop → `block`; nudge that should
   lower the score → `warn`; observe-only → `audit`. Weight 4/2/1 by severity.
   Add `runtime` to `scope` only if a single action can be judged against it.
4. **Write one file** to the project's rules directory as `<id>.yaml`. One rule
   per file — the loader treats each file as a single rule and ids must be
   unique across files.
5. **Validate.** Run `warden validate` (add `--rules <dir>` if the rules live
   somewhere other than `./rules` or `$CLAUDE_PROJECT_DIR/rules` — in this repo
   the sample policy is `demo/rules`). Fix anything it reports; the message names
   the file and field.
6. **Optionally dry-run** the rule against code to confirm it fires where you
   expect: `warden check <path> --rules <dir>` (add `--no-llm` to skip the
   semantic layer).

## Guardrails (and why)

- **Don't add fields.** The schema is closed except for the optional `paths`;
  `warden validate` fails on any other unknown key. If you feel a rule needs
  more, it usually wants a different match type or a `paths` scope, not a new
  field.
- **Reach for `paths` before weakening enforcement.** If a rule would be noisy
  repo-wide (legit uses outside the target area), scoping it with `paths` and
  keeping `warn`/`block` is better than dropping to `audit` to hide the noise.
- **One rule per file, unique id.** The loader maps file → rule; a duplicate id
  is a hard error.
- **Prefer deterministic layers.** Don't write an `llm` rule for something a
  `pattern` catches — it's slower, costs a call, and won't run offline.
- **The runtime gate ignores weight/score.** Don't tune weight hoping to change
  runtime behavior; only `enforcement` matters there.

## Worked examples (shipped in this repo)

Each demonstrates one concept — read them under `demo/rules/`:

| id | match | enforcement | weight | scope | shows |
|---|---|---|---|---|---|
| `no-env-vars` | pattern | block | 4 | ci, runtime | critical rule in both gates |
| `no-cross-module-coupling` | structural | block | 4 | ci | architectural boundary |
| `no-pii-in-logs` | llm | warn | 2 | ci | semantic, score-only |
| `prefer-flag-helper` | pattern | audit | 1 | ci | audit mode (logged, unscored) |
