# Warden

[![CI](https://github.com/nicolastakashi/warden/actions/workflows/ci.yml/badge.svg)](https://github.com/nicolastakashi/warden/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**A deterministic policy engine for AI-agent-generated code.** Write a rule once;
enforce it both in CI (scans files under a path, blocks on violations) and at
runtime (a Claude Code hook that blocks a bad edit before it lands).

LLMs don't enforce policy — they're *subject* to it. Warden is the independent
authority that checks an agent's output regardless of what the agent
"remembered" to do.

> **Status — early but working.** A real Rust tool (single static binary,
> multi-language via tree-sitter, tested, run read-only against a real 5.6k-file
> repo) — not a toy, not yet battle-tested. Its proven sweet spot is the
> **runtime gate on sharp rules**; treat a CI failure as a **candidate, not a
> verdict** (a hit means "matches a written convention," not "is a defect").
> See [`docs/conclusion.md`](docs/conclusion.md) for the honest assessment.

## Quick start

```bash
cargo install --path .          # build + put `warden` on PATH (needs a C compiler)
./demo/run_demo.sh              # see the gate block a messy tree and pass a clean one
warden test demo/rules/no-env-vars.yaml examples   # then dry-run a rule yourself
```

A check looks like this:

```
$ warden check examples --rules demo/rules
3 files checked · 1 blocking, 0 warnings, 1 audit   BLOCKED

Blocking failures:
  examples/api/handler.py:16 — Use feature flags instead of environment variables
      return int(os.getenv("REQUEST_TIMEOUT", "30"))

Audit (logged only, does not block):
  examples/config/flags.py:13 — Prefer the get_flag() helper over direct FEATURE_FLAGS access
      return FEATURE_FLAGS["new_checkout"]
```

## How it works

One YAML rule format feeds two consumers that share the same engine:

| Consumer | Input | Output |
|---|---|---|
| **CI gate** (`warden check <path>`) | whole files under a path | exit 1 if any `block` rule fired, + a report of what fired |
| **Runtime gate** (`warden gate`) | one proposed agent action (hook payload on stdin) | block / allow |

The core is **agent-agnostic** — it only ever sees code in and violations out.
The entire Claude Code integration is two small adapter functions, so supporting
another agent is additive. (Architecture: [`docs/design.md`](docs/design.md).)

## Writing rules

One rule per file in `rules/*.yaml`. The schema is closed (unknown fields are
rejected); `paths` is the only optional field.

```yaml
id: no-env-vars
description: "Use feature flags instead of environment variables"
why: "Direct env access bypasses the flag system and causes config drift."
scope: [ci, runtime]          # where it applies: ci, runtime, or both
enforcement: block            # block | warn | audit
paths: ["src/**"]             # optional: scope to a subtree (default: all files)
match:
  type: pattern               # exactly one matcher (below)
  patterns: ['os\.getenv']
```

Three matcher types:

| type | how | use for |
|---|---|---|
| `pattern` | regex over each line | literal / syntactic signals |
| `structural` | tree-sitter forbidden-imports (multi-language, by file extension) | architectural boundaries |
| `query` | a tree-sitter query (`.scm`) as data; every captured node is a violation | structural checks beyond imports (e.g. no `.unwrap()`), one language per rule |

`warden validate --rules <dir>` checks every rule for validity. To see what a
rule actually *catches* before it lands, dry-run it against a path with `warden
test` — one rule, no rules dir needed. It honours the rule's `paths` and ignores
`scope`, so it answers "what would this rule flag here?":

```bash
$ warden test demo/rules/no-env-vars.yaml examples
rule: no-env-vars [pattern, block]
scanned 3 file(s) · 1 match(es):
  examples/api/handler.py:16 → return int(os.getenv("REQUEST_TIMEOUT", "30"))
```

A clean run reports `0 matches` (fired on nothing). For the whole rule set at
once, `warden validate --against <path>` adds a coverage report on top of the
usual validation — `test` inspects one rule in depth, `--against` shows every
rule's reach:

```bash
$ warden validate --rules demo/rules --against examples
...
coverage vs `examples` (dry-run, no enforcement):
  ✓ no-cross-module-coupling  structural   3 files · 0 hits
  ✓ no-env-vars               pattern      3 files · 1 hits
  ✓ prefer-flag-helper        pattern      3 files · 1 hits
```

A rule whose `paths` match no files shows `⚠ … paths matched nothing` — it can
never fire, so you catch a dead rule (often the `src/**` vs `**/src/**` glob
footgun) at authoring time. Add `--strict` to make that exit 1 in CI. Authoring
help lives in two
[Agent Skills](https://agentskills.io) under [`skills/`](skills/) —
`warden-rule-author` and `warden-rule-discovery` — usable by any skills-compatible
agent (Claude Code, Cursor, Codex, …). Install them into your own project with
[skills.sh](https://www.skills.sh):

```bash
npx skills add nicolastakashi/warden          # both skills (prompts to pick)
npx skills add nicolastakashi/warden --skill '*'   # all, non-interactive
```

## Use it as a Claude Code hook

`warden gate` reads a `PreToolUse` payload on stdin and decides. Wire it into
your agent's hook config — for Claude Code, `.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      { "matcher": "Write|Edit",
        "hooks": [
          { "type": "command",
            "command": "${CLAUDE_PROJECT_DIR}/target/release/warden gate" }
        ] }
    ]
  }
}
```

It blocks via the JSON permission path (`permissionDecision: "deny"`) and
**always exits 0** — exit code 1 does *not* block in Claude Code.
`${CLAUDE_PROJECT_DIR}` lets it find that project's own `rules/`.

## Rules live in your project

Warden is the engine; the policy is yours. The CLI ships **no default rules** and
resolves them as `--rules <dir>` → `$CLAUDE_PROJECT_DIR/rules` → `./rules`
(erroring if none is found). This repo dogfoods itself with [`rules/`](rules/)
(enforced on `src/`); the [`demo/`](demo/) app carries a separate sample policy.

## Requirements

A Rust toolchain and a C compiler (for the tree-sitter grammars) to build. At
runtime it needs nothing external — checks run fully locally and offline.

## FAQ

**Does running it on every edit cost tokens?** Effectively no — Claude Code runs
it as a local binary (not a model-visible tool), and an *allow* injects nothing
into the model's context. You only pay a retry when it **blocks** — the intended
trade (a cheap stop now beats unwinding a bad edit later).

**What leaves my machine?** Nothing — checks run fully locally; Warden makes no
network calls.

**Won't false positives block me constantly?** Only if you start at `block`.
Calibrate the way the `warden-rule-discovery` skill recommends: begin at
`warn` / `audit`, scope with `paths`, and promote to `block` once the existing
code is clean.

## Docs

- [`docs/conclusion.md`](docs/conclusion.md) — honest assessment + what to build next
- [`docs/design.md`](docs/design.md) — architecture & scoring
- [`docs/decisions.md`](docs/decisions.md) — design choices & deviations
- [`docs/tree-sitter.md`](docs/tree-sitter.md) — the structural backend & roadmap
