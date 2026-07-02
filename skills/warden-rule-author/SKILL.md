---
name: warden-rule-author
description: >-
  Author Warden rules — the rules/*.yaml files the `warden` CLI enforces
  as a CI gate (scans a path, blocks on `block` rules) and a runtime gate
  (block/allow on a proposed agent action). Use this whenever someone wants to
  add, write, create, or edit a warden policy rule, or turn a coding convention
  or a "ban/avoid/forbid/require/warn on X" guideline into an enforceable rule.
  Trigger even when the intent is described without naming the rule file — e.g.
  "block direct env-var access", "forbid billing from importing notifications",
  "no .unwrap() in src". Also use when asked how the rule schema works (id,
  scope, enforcement, or the pattern / structural / query match types). Always
  finish by running `warden validate` so the rule is proven valid.
compatibility: "Requires the warden CLI."
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

## The schema (every field is required, except `paths`)

```yaml
id: no-env-vars                  # unique, kebab-case
description: "Use feature flags instead of environment variables"
why: "Direct env access bypasses the flag system and causes config drift."

scope: [ci]                      # subset of: ci, runtime
enforcement: block               # block | warn | audit

match:                           # exactly one type
  type: pattern                  # pattern | structural | query
  # ...type-specific fields (see below)
```

- **id** — kebab-case, unique across all rule files. The filename should match
  (`<id>.yaml`) so rules are easy to find.
- **description** — the one line developers see in output as `file:line —
  description`. Make it an actionable instruction, not a restatement of the id.
- **why** — the rationale, shown to humans in the block reason. Explain the
  actual risk so the message teaches the fix.

## scope — which consumer evaluates the rule

- `ci` — the CI gate scans a path, reports which rules fired (`file:line →
  snippet`) plus counts, and exits 1 if any `block` rule fired.
- `runtime` — the runtime gate checks **one** proposed agent action
  (e.g. a Write/Edit/Bash) via a hook and returns block/allow. It reads only
  `enforcement`.
- A rule may be in both (e.g. the critical env-var rule). Put it in `runtime`
  only when it makes sense to judge a single action against it — `pattern` and
  `query` do; a cross-file `structural` import rule usually does not.

## enforcement — what a violation does

- `block` — fails the gate. CI exits 1; runtime returns `deny`. Use for hard
  rules you want to stop.
- `warn` — reported but never blocks (CI notes it; runtime surfaces it and still
  allows). Use for "should fix".
- `audit` — logged only, never blocks. Use to observe adoption of a new
  convention without penalizing anyone yet.

## paths — optional, scope the rule to a subtree

```yaml
paths: ["**/platform_features/**", "**/src/api/**"]   # optional; file-path globs
```

By default a rule applies to every checked file. Add `paths` (a list of
file-path globs, `fnmatch`, `*` crosses `/`) to restrict it to matching files —
this is what lets a `pattern` rule target a subtree, the way `structural`'s
`from` glob already does. Use it when a convention only holds in certain paths:
e.g. "prefer feature flags over env access **in feature/render code**" (boundary
config legitimately reads env). In the runtime gate, a rule whose `paths` don't
match the action's path simply doesn't apply. This is the one optional field —
everything else is required, and the schema is otherwise closed.

A **leading `**/` matches at any depth, including the repo root** (`**/src/**`
matches both a top-level `src/…` and a nested `a/src/…`), so a single glob
usually suffices. `warden validate --against <path>` reports a `paths` glob that
matches no files as `⚠ 0 files` — dry-run it to catch a dead scope.

## The three match types — pick the simplest that captures the intent

All three are deterministic, fast, and run offline. Reach for the cheapest layer
that works: literal/syntactic → `pattern`; an import boundary → `structural`; a
structural check that isn't an import → `query`.

### pattern — regex over each line (language-agnostic)

```yaml
match:
  type: pattern
  patterns: ['os\.getenv', 'os\.environ']   # flat list, OR-combined
```

Any line matching any pattern is a violation, reported at that line. Patterns
are regular expressions — escape literal dots (`os\.getenv`). The list is flat
and OR-combined. Best for literal or syntactic signals (a banned call, a
forbidden token, a TODO marker). Note: an invalid regex is skipped silently at
match time, so dry-run with `warden test` to confirm it fires.

### structural — forbidden imports via tree-sitter (multi-language)

```yaml
match:
  type: structural
  forbidden:
    - from: "**/src/billing/**"
      to: "**/src/notifications/**"
```

For architectural boundaries. `from` and `to` are **file-path globs** (matched
with `fnmatch`, where `*` crosses `/`, and a leading `**/` matches at any depth
including the root). A file whose path matches `from` may not import a module
whose path matches `to`. The language is inferred from the file extension
(Python and Go today; Rust has no import walker yet). Files in unsupported
languages, or that don't parse, are skipped. List multiple edges under
`forbidden`.

The `to` glob matches the **imported module** as a slash path with no leading
slash (`services.analytics.metrics` → `services/analytics/metrics`).

**Matching a package — how the import shape maps to a glob.** For a package
`foo`, the candidate path depends on how it's imported:

| import | candidate | glob that matches |
|---|---|---|
| `import foo` | `foo` | `to: "foo"` (exact) |
| `from foo.bar import x` / `import foo.bar` | `foo/bar` | `to: "**/foo/**"` |
| nested `app.foo.bar` | `app/foo/bar` | `to: "**/foo/**"` |

Because a leading `**/` matches at any depth (including the root), **`**/foo/**`
catches every submodule import of `foo` — top-level `from foo.x` *and* nested
`from app.foo.x`**. The one case it misses is a bare `import foo` (candidate
`foo`, no submodule segment), which needs an exact `to: "foo"`. So forbidding a
package fully is two edges: `**/foo/**` + `foo`. This fails *open* (a miss is
silently no violation), so always dry-run `warden test <rule> <path>` (or
`warden check`) against a real offending import to confirm the rule fires.

### query — arbitrary structural check via a tree-sitter query (single language)

When the rule is structural but *not* an import boundary — a banned call like
`.unwrap()`, an `unsafe` block, a bare `except:` — use `query`. The rule *is* a
tree-sitter query (`.scm`); every captured node is one violation. This is
"rules-as-data": a new structural check needs **no engine code**, just a query.

```yaml
match:
  type: query
  language: rust                 # one of: python, go, rust
  query: |
    (call_expression
      function: (field_expression
        field: (field_identifier) @method)
      (#eq? @method "unwrap"))
```

The win over `pattern`: it matches the real syntax, so it fires on the method
call `.unwrap()` but **not** on `.unwrap_or(...)` or the substring `"unwrap"` in
a string or identifier — precision a regex can't give.

**Capture only the offending node.** *Every* captured node becomes one
violation, so a query with an auxiliary capture (e.g. `(#eq? @a @b)` captures
both `@a` and `@b`) reports the same problem twice. Capture just the node you
want flagged; use string-literal predicates (`(#eq? @m "unwrap")`) rather than
capturing a second node when you only need to compare against a constant. To
match a keyword itself (e.g. ban `unsafe`), match its anonymous node:
`("unsafe") @kw`.

**The honest limit — a query rule is single-language.** Unlike `structural`
(where one `to:` glob spans languages because imports normalize to slash-paths),
tree-sitter queries reference **grammar-specific node kinds** (`call_expression`
in Rust, `call` in Python), so a query is tied to exactly one `language:` and
runs only on files of that language. Covering several languages means several
rules — one per grammar. The `language` must be one warden supports (`python`,
`go`, `rust`); the query is compiled at `warden validate` time, so a malformed
`.scm` or a node kind that doesn't exist in that grammar fails when the rule
loads, not silently at runtime. Text predicates (`#eq?`, `#match?`, `#any-of?`)
are applied. As with `structural`, files that don't parse are skipped (fail-open)
— always dry-run against a real offending file to confirm it fires.

## Workflow

1. **Translate the intent into the smallest match type.** Literal token or call
   → `pattern`. Import/dependency boundary → `structural`. A structural check
   that isn't an import (banned call, `unsafe`, syntax shape), single-language →
   `query`.
2. **Fill the metadata.** Pick a kebab-case `id`, an actionable `description`,
   and a `why` that states the real risk.
3. **Set scope / enforcement.** Hard stop → `block`; nudge → `warn`; observe-only
   → `audit`. Add `runtime` to `scope` only if a single action can be judged
   against it.
4. **Write one file** to the project's rules directory as `<id>.yaml`. One rule
   per file — the loader treats each file as a single rule and ids must be
   unique across files.
5. **Validate.** Run `warden validate` (add `--rules <dir>` if the rules live
   somewhere other than `./rules` or `$CLAUDE_PROJECT_DIR/rules` — in this repo
   the sample policy is `demo/rules`). Fix anything it reports; the message names
   the file and field.
6. **Dry-run** to confirm it fires where you expect: `warden test <rule.yaml>
   <path>` (one rule, no rules dir needed), or `warden check <path> --rules
   <dir>`.

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
- **Prefer `query` over `pattern` when you mean a syntactic construct.** A regex
  matches substrings in strings/comments; a query matches the real AST node.
- **Dry-run before trusting a rule.** Structural/query rules fail open, so a
  wrong glob or node kind silently matches nothing — `warden test` shows you.

## Worked examples (shipped in this repo)

Each demonstrates one concept — read them under `demo/rules/` and `rules/`:

| id | match | enforcement | scope | shows |
|---|---|---|---|---|
| `no-env-vars` | pattern | block | ci, runtime | critical rule in both gates |
| `no-cross-module-coupling` | structural | block | ci | architectural boundary |
| `prefer-flag-helper` | pattern | audit | ci | audit mode (logged, non-blocking) |
| `no-unwrap-in-src` | query | block | ci, runtime | structural check beyond imports |
