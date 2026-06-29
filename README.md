# Warden

A deterministic, agent-agnostic policy engine for AI-agent-generated code.
**One rule format, two consumers:** a CI gate (scored) and a runtime gate
(blocking).

LLMs do not enforce policy; they are *subject* to policy. The warden is the
policy authority — it checks the agent's output independently of what the agent
"remembered".

> **Status — early but working.** A real Rust tool (single static binary,
> multi-language, tested, dogfooded, run read-only against a real 5.6k-file
> repo), not a toy. Its validated sweet spot is narrow: the **runtime gate on
> sharp rules**. The CI **score is a signal, not a verdict** — it's single-user,
> not yet diff-scoped, and a finding means "matches a written convention," not
> "is a defect." Read [`docs/conclusion.md`](docs/conclusion.md) for the honest
> assessment and what to build next.

Warden targets **Claude Code** as the only runtime agent. It is written in
**Rust** (one static binary) and its `structural` matcher runs on **tree-sitter**,
so it is multi-language by design (Python and Go today; more is a grammar away —
see [`docs/tree-sitter.md`](docs/tree-sitter.md)).

**Design docs:** [`docs/design.md`](docs/design.md) (what it is + architecture)
and [`docs/decisions.md`](docs/decisions.md) (the choices and where the build
diverged from the original spec).

## Install

```bash
cargo build --release    # -> target/release/warden
cargo install --path .   # or: put it on PATH
# dev: cargo test
```

**Requirements.** Building needs a Rust toolchain and a C compiler (for the
tree-sitter grammars). At runtime the deterministic layers (pattern, structural)
and the gate need nothing external. The `llm` matcher shells out to the
[`claude`](https://code.claude.com) CLI in headless mode — so it needs `claude`
installed and **logged in** (it rides your existing Claude Code OAuth session;
no `ANTHROPIC_API_KEY` required). In CI, either install + authenticate `claude`,
or run with `--no-llm` to skip the semantic layer entirely.

## Rules live in your project — the warden ships none

The warden is the **engine**; the policy is the **consuming project's**. The
CLI carries no default rules (the binary ships none). It resolves the rules
directory at runtime:

1. `--rules <dir>` if you pass it, else
2. `$CLAUDE_PROJECT_DIR/rules` (set by Claude Code), else
3. `./rules`

A real project keeps its own `rules/` at its root; with the gate wired via
`${CLAUDE_PROJECT_DIR}` (see below) that resolves to *that* project's policy. If
no rules directory is found, the CLI errors — there is nothing to fall back to.

This repo **dogfoods itself**: its own policy is in [`rules/`](rules/) at the root
(enforced on `src/`, wired as a live hook in `.claude/settings.json`). A separate
**sample** policy lives in [`demo/rules/`](demo/rules/) — the `demo/` app plays
the role of an independent consuming project.

## Use

```bash
warden validate --rules demo/rules
warden check demo/before --rules demo/rules               # score + which rules fired + block/pass
warden check demo/before --rules demo/rules --format json # the decision record (§4)
warden check demo/before --rules demo/rules --no-llm      # deterministic layers offline
echo '<payload>' | warden gate --rules demo/rules         # one action → block/allow (Claude Code hook)

./demo/run_demo.sh        # the whole story end-to-end (see demo/README.md)
```

In a consuming project you don't pass `--rules` — the CLI finds that project's
own `rules/`.

`demo/before` trips **no-env-vars** (block, `os.getenv`), **no-cross-module-coupling**
(block, structural), **prefer-flag-helper** (audit), and — when the `llm` matcher
runs — **no-pii-in-logs** (warn). It scores 0/100 live (blocked); `demo/after`
scores 100/100.

**Scoring an `llm` rule that didn't run** (skipped via `--no-llm`, or
inconclusive because `claude` is missing / not authenticated / returned
malformed JSON) is a deliberate choice: it counts as a **pass**. Offline runs
therefore score the deterministic layers as if every `llm` rule passed
(`demo/before --no-llm` reads 20/100, not 0).

## The rule format

One rule per file in `rules/*.yaml`. This is the entire schema — see the spec
§2. Every field is required:

```yaml
id: no-env-vars
description: "Use feature flags instead of environment variables"
why: "Direct env access bypasses the flag system and causes config drift."
scope: [ci]                 # subset of: ci, runtime
enforcement: block          # block | warn | audit
weight: 4                   # 1 | 2 | 4   (CI scorer only)
match:                      # exactly one type
  type: pattern             # pattern | structural | llm
  patterns: ['os\.getenv']
```

An optional `paths:` list of file-path globs scopes a rule to a subtree (absent
= all files) — so `pattern`/`llm` rules can target an area the way `structural`'s
`from` glob does. It's the one optional field; the schema is otherwise closed.

- **pattern** — regex over each line of a unit (flat list, OR-combined).
- **structural** — forbidden imports via **tree-sitter** (multi-language: the
  language is inferred from the file extension); `from`/`to` are file-path globs.
- **llm** — semantic check delegated to Claude via the `claude` CLI headless
  (`claude -p`, single turn, no tools); strict JSON verdict, parsed
  defensively (anything malformed → inconclusive → pass); skippable with
  `--no-llm`.

## Scoring (CI gate)

```
        Σ (passed_i × weight_i)
score = ───────────────────────── × 100   90+ Excellent · 70+ Good · 50+ Fair · <50 Poor
        Σ (total_i  × weight_i)
```

Scored set = `ci`-scoped rules with enforcement `block` or `warn`. `audit` rules
are logged only — excluded from the score and never block. Score is binary per
rule; `extent` (fraction of files where a rule fired) is recorded but never
weights the score.

Two independent results: **enforcement** (any violated `block` rule → exit 1)
and the **score**.

## Claude Code wiring (runtime gate)

`warden gate` is just a CLI subcommand: it reads the Claude Code hook payload
from stdin and writes the decision to stdout. Wire it in `.claude/settings.json`
(see [`settings.example.json`](.claude/settings.example.json)):

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Write|Edit",
        "hooks": [
          { "type": "command",
            "command": "${CLAUDE_PROJECT_DIR}/target/release/warden gate" }
        ]
      }
    ]
  }
}
```

`${CLAUDE_PROJECT_DIR}` lets the warden locate the project's own `rules/`
(at the project root) regardless of cwd — no `--rules` needed in a real project.
The gate blocks with the JSON permission path (`permissionDecision: "deny"`) and
**always exits 0** — exit code 1 does NOT block in Claude Code.

## How to add another agent

The core only ever sees `ProposedAction` and returns `GateDecision` — it never
knows which agent called it. The entire Claude-Code-specific surface is two
translation functions in [`adapters/claude_code.rs`](src/adapters/claude_code.rs):

```
parse_claude_payload(stdin_json)  → ProposedAction     (inbound adapter)
evaluate_action(action, rules)    → GateDecision        (core — agent-agnostic)
format_claude_response(decision)  → stdout + exit code  (outbound adapter)
```

To support another agent (codex, opencode, …), write a new `parse_*` /
`format_*` pair (or add an `--agent` flag) — **zero changes to the core**. That
decoupling is the whole point.
