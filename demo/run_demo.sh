#!/usr/bin/env bash
# Walks the warden through the demo scenarios.
#
#   ./demo/run_demo.sh            # full run (llm matcher live via `claude`)
#   ./demo/run_demo.sh --no-llm   # offline: deterministic layers only
#
# Run from anywhere; it locates the warden root and built binary itself.
set -uo pipefail

WARDEN_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$WARDEN_ROOT"
export CLAUDE_PROJECT_DIR="$WARDEN_ROOT"

# Prefer the release build; build it if missing; fall back to debug, then PATH.
WARDEN="$WARDEN_ROOT/target/release/warden"
if [ ! -x "$WARDEN" ] && command -v cargo >/dev/null 2>&1; then
  cargo build --release --quiet
fi
[ -x "$WARDEN" ] || WARDEN="$WARDEN_ROOT/target/debug/warden"
[ -x "$WARDEN" ] || WARDEN="warden"

# This repo is the warden *engine* — it ships no default rules. The demo plays
# the "consuming project" and brings its own policy in demo/rules.
RULES="demo/rules"
LLM_FLAG="${1:-}"   # pass --no-llm to skip the semantic layer

rule() { printf '\n\033[1m%s\033[0m\n' "==== $* ===="; }

rule "1. validate rules"
"$WARDEN" validate --rules "$RULES"

rule "2. BEFORE — a messy PR (human output)"
"$WARDEN" check demo/before --rules "$RULES" $LLM_FLAG; echo "exit: $?  (1 = blocked)"

rule "3. BEFORE — same run as a decision record (JSON)"
"$WARDEN" check demo/before --rules "$RULES" $LLM_FLAG --format json

rule "4. AFTER — every violation fixed"
"$WARDEN" check demo/after --rules "$RULES" $LLM_FLAG; echo "exit: $?  (0 = pass)"

rule "5. runtime gate — Claude Code tries to Write env-var code (BLOCK)"
printf '%s' '{"hook_event_name":"PreToolUse","tool_name":"Write","tool_input":{"file_path":"x.py","content":"url = os.getenv(\"GW\")"}}' \
  | "$WARDEN" gate --rules "$RULES" --no-llm
echo "exit: $?  (deny JSON above, exit 0 = blocked the tool)"

rule "6. runtime gate — Claude Code tries a clean Write (ALLOW)"
printf '%s' '{"hook_event_name":"PreToolUse","tool_name":"Write","tool_input":{"file_path":"x.py","content":"total = price * qty"}}' \
  | "$WARDEN" gate --rules "$RULES" --no-llm
echo "exit: $?  (empty stdout above = no opinion = allowed)"
