# Warden — guide for AI agents

Conventions, architecture, and commands for working in this repo. **This is the
canonical, tool-neutral project guide** — any AI coding agent (Claude Code,
Cursor, Codex, …) should read it. Agent-specific glue (a `CLAUDE.md`, a
`.claude/` directory) is intentionally **not versioned**; it's local per
developer/agent. Claude Code users can `ln -s AGENTS.md CLAUDE.md` locally to
auto-load this.

## What this is

**Warden** is a deterministic, agent-agnostic policy engine for AI-agent-generated code. One YAML rule format feeds two consumers: a **CI gate** (scores a path 0–100, blocks on `block` rules) and a **runtime gate** (checks one proposed agent action via a hook, returns block/allow). The CI gate scans whole files under the target path — it is not diff-scoped (see `docs/decisions.md`). It's **early but working** — a real tool (single-user, not yet diff-scoped; the CI score is a signal, not a verdict), not a toy. **If you're picking this up cold, read `docs/conclusion.md` first** (honest assessment, the one limit that matters, and what to build next), then `docs/design.md` (architecture) and `docs/decisions.md` (choices & deviations). Backward compatibility is not a concern; prefer the cleanest design over preserving old behavior.

It is written in **Rust** and ships as a single static binary. The `structural` matcher is built on **tree-sitter**, so the engine is multi-language by design (see `docs/tree-sitter.md`).

## Commands

```bash
# build (Rust toolchain via rustup/homebrew; needs a C compiler for tree-sitter grammars)
cargo build --release            # -> target/release/warden

# tests (the gate — there is no separate lint step beyond `cargo clippy`)
cargo test
cargo test --test gates          # one integration test file (tests/gates.rs)
cargo test runtime_blocks_env    # a single test by name

# run the CLI (rules dir is required — see "Rules live in the consuming project")
cargo run -- validate --rules <dir>
cargo run -- check <path> --rules <dir> [--no-llm] [--format human|json]
echo '<PreToolUse payload>' | cargo run -- gate --rules <dir> [--no-llm]
# or use the built binary directly: target/release/warden <...>

# install on PATH so `warden` works anywhere (and as a Claude Code hook)
cargo install --path .

# end-to-end demos
./demo/run_demo.sh [--no-llm]      # before (blocked) → after (pass) → gate block/allow
./demo/try_with_claude.sh          # drives a REAL claude session; confirms the gate denies a Write
```

`cargo test` is the gate. `--no-llm` makes everything run offline (the `llm` matcher is the only thing that reaches out).

## Architecture — the load-bearing ideas

**The agent-agnostic core is the whole point.** The engine only ever sees a `CodeUnit` (`path` + `content`) in and a `Violation` out; the runtime path sees `ProposedAction` in and `GateDecision` out (`runtime_gate.rs`). It never knows which agent called it. The *entire* Claude Code surface is two translation functions in `adapters/claude_code.rs` (`parse_claude_payload` / `format_claude_response`). Supporting another agent = a new `parse_*`/`format_*` pair, **zero core changes**. Don't leak agent-specific concepts into the core. (This invariant is dogfooded by the `core-stays-agent-agnostic` rule in `rules/`.)

**Matchers** (`src/matchers/`) all have signature `(units, rule) -> Vec<Violation>`, dispatched by `match_type`:
- `pattern` — regex (the `regex` crate) over each line of a unit; language-agnostic.
- `structural` — **tree-sitter** forbidden-imports; `from`/`to` are **file-path globs** (fnmatch semantics, `*` crosses `/`). Multi-language: the file's language is inferred from its extension (`src/lang.rs` maps `.py`→Python, `.go`→Go; add a grammar + extractor there for more). A file in an unsupported language, or that doesn't parse cleanly, is skipped. Imports are normalized to slash paths so one rule works across languages.
- `llm` — **shells out to the `claude` CLI** (`claude -p` headless via `std::process::Command`), NOT a direct API call. Rides the dev's existing Claude Code login (no `ANTHROPIC_API_KEY`); `claude` must be installed + logged in. Parses defensively: anything malformed/unavailable → **inconclusive → counts as pass** + warning. Skippable with `--no-llm`, size-guarded against repo-wide prompts. The process call is behind the `ClaudeRunner` trait so it's faked in tests.

**Rule schema** (`src/schema.rs`) is **closed** — `warden validate` rejects unknown fields — *except* the one optional `paths` field (a list of globs scoping a rule to a subtree; absent = all files). Fields: `id`, `description`, `why`, `scope ⊆ {ci,runtime}`, `enforcement ∈ {block,warn,audit}`, `weight ∈ {1,2,4}`, `match` (exactly one type). One rule per file; the schema is the single source of truth (`src/schema.rs`, `docs/design.md`).

**CI gate** (`ci_gate.rs`): filter rules to `scope` ∋ `ci` → apply each rule's `paths` → run matchers in order pattern→structural→llm → two independent results: **enforcement** (any violated `block` → exit 1) and a **weighted score** = `Σ(weight of passed scored rules)/Σ(weight of scored rules)×100`. Only `block`+`warn` are scored; `audit` is logged only. Score is **binary per rule** (`extent` is recorded but never weights it). No scored rules → 100.

**Runtime gate** (`runtime_gate.rs`): filter to `scope` ∋ `runtime`, read **only** `enforcement` (weight/score are meaningless for one action), and a rule whose `paths` don't match the action's path doesn't apply. **Blocking is via JSON `permissionDecision: "deny"` and exit 0** — exit code 1 does NOT block in Claude Code. All of that lives in `adapters/claude_code.rs`; don't rely on exit codes to block.

## Rules live in the consuming project — the engine ships none

There are **no bundled default rules**. The CLI resolves the rules dir as `--rules <dir>` → `$CLAUDE_PROJECT_DIR/rules` → `./rules`, and errors if none is found. This repo dogfoods itself: its own policy is in **`rules/`** at the root (the `core-stays-agent-agnostic` and `no-direct-anthropic-api` rules, enforced on `src/`). The runtime hook that runs them is wired locally (for Claude Code, a `.claude/settings.json` calling `warden gate` — gitignored, since the wiring is per-agent; the shared artifact is the policy in `rules/`). The `demo/` fake app plays the role of a separate consuming project with its own `demo/rules/` (`demo/before/` is messy, `demo/after/` is clean). `examples/` holds minimal fixtures used by the gate tests. A real project keeps its own `rules/` at its root; the runtime hook finds it via `${CLAUDE_PROJECT_DIR}`.

## Authoring skills

The two rule-authoring skills (`warden-rule-author`, `warden-rule-discovery`) follow the open [Agent Skills](https://agentskills.io) standard — a folder + `SKILL.md` (name/description frontmatter), usable by any skills-compatible agent (Cursor, Codex, Gemini CLI, Claude Code, …). They live in **`skills/`** at the root, *not* under a tool-specific path, and install into any project with `npx skills add nicolastakashi/warden` ([skills.sh](https://www.skills.sh)). For local Claude Code discovery in this repo, symlink `.claude/skills -> ../skills` (gitignored). Keep them agent-agnostic — describe the task, not a specific agent; declare tool dependencies (the `warden`/`claude` CLIs) in the `compatibility` frontmatter field.
