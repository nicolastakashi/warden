#!/usr/bin/env bash
# Agent-in-the-loop test: wire the runtime gate into a REAL Claude Code session
# and watch it block a policy-violating Write.
#
# It builds a throwaway project (so it never touches this repo or your own
# session), activates the `warden gate` PreToolUse hook there, then asks
# `claude` to write code that reads an env var. The gate denies the Write, so
# the offending file is never created.
#
#   ./demo/try_with_claude.sh
#
# Requires `claude` installed + logged in.
set -uo pipefail

WARDEN_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEMO="$WARDEN_ROOT/demo"

# Make the `warden` binary resolvable on PATH (the demo's settings.json calls
# it by name). Build the release binary if needed, then put it on PATH.
if [ ! -x "$WARDEN_ROOT/target/release/warden" ] && command -v cargo >/dev/null 2>&1; then
  (cd "$WARDEN_ROOT" && cargo build --release --quiet)
fi
[ -d "$WARDEN_ROOT/target/release" ] && PATH="$WARDEN_ROOT/target/release:$PATH"
if ! command -v warden >/dev/null 2>&1 || ! command -v claude >/dev/null 2>&1; then
  echo "need both 'warden' and 'claude' on PATH" >&2
  exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Stand up a throwaway consuming project: its own rules/ and its own
# .claude/settings.json — both copied verbatim from the demo, no inlined JSON.
# ${CLAUDE_PROJECT_DIR} resolves to $WORK, so the gate finds $WORK/rules.
mkdir -p "$WORK/rules" "$WORK/.claude"
cp "$DEMO/rules/"*.yaml "$WORK/rules/"
cp "$DEMO/.claude/settings.json" "$WORK/.claude/settings.json"

echo "==== throwaway project: $WORK ===="
echo "Asking Claude to write env-var code (the gate should deny it)..."
echo

cd "$WORK"
claude -p \
  "Create a Python file config.py with a function get_timeout() that returns int(os.getenv('REQUEST_TIMEOUT', '30')). Use os.getenv directly." \
  --allowedTools "Write Edit" \
  --max-turns 3 \
  --output-format text 2>&1 || true

echo
echo "==== verdict ===="
# The reliable signal: no file containing direct env access was written.
if grep -rqE "os\.getenv|os\.environ" "$WORK" --include='*.py' 2>/dev/null; then
  echo "❌ env-var code landed on disk — the gate did NOT block it:"
  grep -rnE "os\.getenv|os\.environ" "$WORK" --include='*.py'
  exit 1
else
  echo "✅ no env-var code was written — the gate blocked every offending Write."
fi
