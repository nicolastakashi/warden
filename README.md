# Warden

**A deterministic policy engine for AI-agent-generated code.** Write a rule once;
enforce it both in CI (a 0–100 score, blocks on violations) and at runtime (a
Claude Code hook that blocks a bad edit before it lands).

LLMs don't enforce policy — they're *subject* to it. Warden is the independent
authority that checks an agent's output regardless of what the agent
"remembered" to do.

> **Status — early but working.** A real Rust tool (single static binary,
> multi-language via tree-sitter, tested, run read-only against a real 5.6k-file
> repo) — not a toy, not yet battle-tested. Its proven sweet spot is the
> **runtime gate on sharp rules**; treat the CI score as a **signal, not a
> verdict** (a hit means "matches a written convention," not "is a defect").
> See [`docs/conclusion.md`](docs/conclusion.md) for the honest assessment.

## Quick start

```bash
cargo install --path .          # build + put `warden` on PATH (needs a C compiler)
./demo/run_demo.sh --no-llm     # see it block a messy tree and pass a clean one
```

A check looks like this:

```
$ warden check examples --rules demo/rules --no-llm
Score: 60/100 (Fair)   files checked: 3   BLOCKED

Blocking failures:
  examples/api/handler.py:16 — Use feature flags instead of environment variables
```

## How it works

One YAML rule format feeds two consumers that share the same engine:

| Consumer | Input | Output |
|---|---|---|
| **CI gate** (`warden check <path>`) | whole files under a path | a 0–100 score + exit 1 if any `block` rule fired |
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
weight: 4                     # 1 | 2 | 4  (weights the CI score)
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
| `llm` | delegates to the `claude` CLI for a strict JSON verdict | judgment a regex can't express |

`warden validate --rules <dir>` checks every rule. Authoring help lives in the
`warden-rule-author` and `warden-rule-discovery` skills under `.claude/skills/`.

## Use it as a Claude Code hook

`warden gate` reads a `PreToolUse` payload on stdin and decides. Wire it in
`.claude/settings.json` ([example](.claude/settings.example.json)):

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

A Rust toolchain + a C compiler (for the tree-sitter grammars) to build. At
runtime, only the `llm` matcher reaches out — it shells out to the
[`claude`](https://code.claude.com) CLI over your existing OAuth session (no
`ANTHROPIC_API_KEY`); `--no-llm` skips it so everything else runs offline.

## FAQ

**Does running it on every edit cost tokens?** With deterministic rules
(`pattern` / `structural`), effectively no — Claude Code runs it as a local
binary (not a model-visible tool), and an *allow* injects nothing into the
model's context. You only pay a retry when it **blocks** — the intended trade. The
exception: an `llm`-type rule in `runtime` scope fires a `claude` call on *every*
edit, so keep runtime rules deterministic and leave `llm` for CI.

**What leaves my machine?** With `pattern` / `structural`, nothing — they run
fully locally. Only the `llm` matcher sends the code under review to the `claude`
CLI (over your own OAuth session); `--no-llm` keeps everything offline.

**Won't false positives block me constantly?** Only if you start at `block`.
Calibrate the way the `warden-rule-discovery` skill recommends: begin at
`warn` / `audit`, scope with `paths`, and promote to `block` once the existing
code is clean.

**What if `claude` isn't installed, or I'm offline?** The `llm` matcher degrades
to *inconclusive → pass* (with a warning); the deterministic layers are
unaffected. `--no-llm` skips it outright.

## Docs

- [`docs/conclusion.md`](docs/conclusion.md) — honest assessment + what to build next
- [`docs/design.md`](docs/design.md) — architecture & scoring
- [`docs/decisions.md`](docs/decisions.md) — design choices & deviations
- [`docs/tree-sitter.md`](docs/tree-sitter.md) — the structural backend & roadmap
