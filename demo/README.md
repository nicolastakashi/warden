# Demo — a fake checkout service

A small, believable app the warden can police, with two scenarios that tell the
whole story:

- **`before/`** — what an AI agent produced in a hurry. Trips every rule type.
- **`after/`** — the same app with each violation fixed. Passes clean.

> This is the *demo playground*. The spec's minimal fixture lives in
> [`../examples/`](../examples/) — leave it alone; experiment here.

The **policy** for this demo lives in [`rules/`](rules/) right here — the warden
engine ships no default rules, so the demo brings its own (a consuming project
would keep `rules/` at its own root instead). Every command below passes
`--rules demo/rules`; `run_demo.sh` does it for you.

## Run it

```bash
./demo/run_demo.sh            # deterministic, fully offline
```

The script walks through: validate → `before` (human) → `before` (JSON decision
record) → `after` → a runtime gate **block** → a runtime gate **allow**. Those
last two feed the gate synthetic hook payloads.

### Test the gate with the real Claude Code agent

```bash
./demo/try_with_claude.sh     # needs `claude` installed + logged in
```

This is the agent-in-the-loop test: it builds a throwaway project (so it never
touches this repo or your own session), copies this demo's real
[`.claude/settings.json`](.claude/settings.json) and [`rules/`](rules/) into it,
then asks `claude` to write code that reads an env var. The gate denies the
Write, Claude reports the block (quoting the rule), and the script verifies the
offending file never landed on disk. This proves the runtime gate end-to-end
against the actual agent — not just synthetic stdin.

The wiring is a committed file — [`demo/.claude/settings.json`](.claude/settings.json) —
not generated inline. It activates the gate as a `PreToolUse` hook for `Write|Edit`
and calls `warden gate`; `${CLAUDE_PROJECT_DIR}` resolves to the demo,
so the gate finds `demo/rules` with no `--rules` needed. Because of that, you can
also just run Claude Code with the demo as the project root and the gate is live
(requires `warden` on your PATH).

## What each file trips

| File (`before/`) | Rule | Type | Enforcement |
|---|---|---|---|
| `api/checkout.py` | `no-env-vars` (`os.getenv`) | pattern | **block** |
| `api/checkout.py` | `prefer-flag-helper` (`FEATURE_FLAGS[...]`) | pattern | audit |
| `billing/charge.py` | `no-cross-module-coupling` (imports `notifications`) | structural | **block** |
| `notifications/email.py`, `config/flags.py` | — clean — | | |

## What you should see

- **`before/`** — **BLOCKED** (exit 1): the two `block` rules fire; the audit
  finding is logged but does not block.
- **`after/`** — **PASS** (exit 0): nothing fires.

That before → after swing is the point: the same engine and the same rules, run
as a CI gate, turn a messy change into a blocking signal — and confirm the fix.
The runtime-gate steps show the *other* consumer of the same rules: one proposed
Claude Code action in, `block`/`allow` out.
