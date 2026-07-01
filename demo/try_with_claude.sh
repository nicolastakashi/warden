#!/usr/bin/env bash
# Agent-in-the-loop test: wire the runtime gate into a REAL Claude Code session
# and watch it deny policy-violating actions — both a Write and an Edit.
#
# It builds a throwaway project (so it never touches this repo or your own
# session), activates the `warden gate` PreToolUse hook there, then asks `claude`
# to (1) write code that reads an env var and (2) edit an existing file to add a
# forbidden import. The gate denies both, so neither violation lands on disk.
#
# Scenario 2 specifically exercises the post-edit reconstruction: a `structural`
# rule only fires on the file as it WILL exist, not the raw Edit fragment.
#
#   ./demo/try_with_claude.sh
#
# Requires `claude` installed + logged in.
set -uo pipefail

WARDEN_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEMO="$WARDEN_ROOT/demo"

# Make the `warden` binary resolvable on PATH (the demo's settings.json calls it
# by name). Build the release binary so the hook runs the current code.
if command -v cargo >/dev/null 2>&1; then
  (cd "$WARDEN_ROOT" && cargo build --release --quiet) \
    || { echo "cargo build failed — aborting so the demo can't run a stale binary" >&2; exit 1; }
fi
[ -d "$WARDEN_ROOT/target/release" ] && PATH="$WARDEN_ROOT/target/release:$PATH"
if ! command -v warden >/dev/null 2>&1 || ! command -v claude >/dev/null 2>&1; then
  echo "need both 'warden' and 'claude' on PATH" >&2
  exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Stand up a throwaway consuming project: its own rules/ and .claude/settings.json.
# ${CLAUDE_PROJECT_DIR} resolves to $WORK, so the gate finds $WORK/rules.
mkdir -p "$WORK/rules" "$WORK/.claude" "$WORK/app"
cp "$DEMO/rules/"*.yaml "$WORK/rules/"
cp "$DEMO/.claude/settings.json" "$WORK/.claude/settings.json"

# A structural runtime rule that fires only on the reconstructed file (the
# matcher parses the resulting file, not the raw Edit fragment).
cat > "$WORK/rules/no-app-imports-legacy.yaml" <<'YAML'
id: no-app-imports-legacy
description: "Code under app/ must not import the legacy module"
why: "The legacy module is being retired; new code must not depend on it."
scope: [runtime]
enforcement: block
match:
  type: structural
  forbidden:
    # Forbid the `legacy` package however it is imported. The import is matched
    # as a slash-path with no leading slash, so it takes three edges:
    #   `import legacy`          -> "legacy"       (to: "legacy")
    #   `from legacy.x import y` -> "legacy/x"     (to: "legacy/**")
    #   nested `app.legacy.x`    -> "app/legacy/x" (to: "**/legacy/**")
    - from: "**/app/**"
      to: "legacy"
    - from: "**/app/**"
      to: "legacy/**"
    - from: "**/app/**"
      to: "**/legacy/**"
YAML

fail=0
cd "$WORK"

# --- Scenario 1: Write env-var code — pattern rule should DENY ----------------
echo "==== Scenario 1: Write os.getenv (pattern rule should DENY) ===="
claude -p \
  "Create a Python file config.py with a function get_timeout() that returns int(os.getenv('REQUEST_TIMEOUT', '30')). Use os.getenv directly." \
  --allowedTools "Write Edit" --max-turns 3 --output-format text 2>&1 || true
if grep -rqE "os\.getenv|os\.environ" "$WORK" --include='*.py' 2>/dev/null; then
  echo "❌ env-var code landed on disk — the gate did NOT block it:"
  grep -rnE "os\.getenv|os\.environ" "$WORK" --include='*.py'
  fail=1
else
  echo "✅ no env-var code written — the gate blocked the Write."
fi

echo
# --- Scenario 2: Edit a file to add a forbidden import — structural should DENY
echo "==== Scenario 2: Edit app/service.py to import legacy (structural should DENY) ===="
printf 'def handle():\n    return 1\n' > "$WORK/app/service.py"
claude -p \
  "Edit app/service.py: inside handle(), import compute from the legacy.helpers module ('from legacy.helpers import compute') and return compute() instead of 1." \
  --allowedTools "Edit" --max-turns 3 --output-format text 2>&1 || true
if grep -Eq '^[[:space:]]*(from|import)[[:space:]]+legacy' "$WORK/app/service.py" 2>/dev/null; then
  echo "❌ legacy import landed in app/service.py — the gate did NOT block the Edit:"
  cat "$WORK/app/service.py"
  fail=1
else
  echo "✅ no legacy import — the gate blocked the Edit (judged the reconstructed file)."
fi

echo
echo "==== verdict ===="
if [ "$fail" -eq 0 ]; then
  echo "✅ every policy-violating action was blocked (Write + Edit)."
else
  echo "❌ at least one violation landed on disk."
  exit 1
fi
