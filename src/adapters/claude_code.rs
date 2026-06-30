//! The entire Claude-Code-specific surface: two translation functions.
//!
//! Inbound : parse_claude_payload(stdin_json) -> ProposedAction
//! Outbound: format_claude_response(decision) -> (stdout_text, exit_code)
//!
//! Translation ONLY — no matching, scoring, or rule loading. Adding another
//! agent later = another parse_*/format_* pair, zero core changes. Liberal in
//! what it accepts (e.g. both `content` and `file_text` for a Write) because
//! hook payload shapes vary across Claude Code versions.

use serde_json::Value;

use crate::runtime_gate::{GateDecision, ProposedAction};

/// For an Edit, the matchers should see the file as it WILL exist after the
/// edit — not the raw `new_string` fragment. A fragment usually doesn't parse,
/// so structural (tree-sitter) rules would silently skip it, and pattern rules
/// would lose surrounding context. Reconstruct the result by applying
/// `old_string` -> `new_string` to the on-disk file. Falls back to the fragment
/// if the file can't be read or `old_string` isn't present (e.g. a brand-new
/// file), preserving the previous behavior in those cases.
fn edited_file_content(
    path: Option<&str>,
    tool_input: &Value,
    new_string: Option<String>,
) -> Option<String> {
    let new_string = new_string?;
    let old = tool_input.get("old_string").and_then(|v| v.as_str());
    let replace_all = tool_input
        .get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if let (Some(p), Some(old)) = (path, old)
        && let Ok(current) = std::fs::read_to_string(p)
        && current.contains(old)
    {
        return Some(if replace_all {
            current.replace(old, &new_string)
        } else {
            current.replacen(old, &new_string, 1)
        });
    }
    Some(new_string)
}

/// Map a PreToolUse hook payload to a neutral ProposedAction.
pub fn parse_claude_payload(stdin_json: &str) -> Result<ProposedAction, String> {
    let payload: Value = serde_json::from_str(stdin_json).map_err(|e| e.to_string())?;
    let tool = payload
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tool_input = payload.get("tool_input").cloned().unwrap_or(Value::Null);

    let field = |k: &str| {
        tool_input
            .get(k)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };

    let path = field("file_path");
    let command = field("command");
    // Write: the new file content. Edit: the file as it will exist *after* the
    // edit (reconstructed from disk), not the raw fragment — see
    // edited_file_content. Field names vary across CC versions.
    let content = match tool.as_str() {
        "Write" => field("content").or_else(|| field("file_text")),
        "Edit" => {
            let new_string = field("new_string").or_else(|| field("content"));
            edited_file_content(path.as_deref(), &tool_input, new_string)
        }
        _ => None,
    };

    Ok(ProposedAction {
        tool,
        path,
        content,
        command,
    })
}

/// Translate a GateDecision into (stdout, exit_code) for Claude Code.
///
/// Block uses the JSON permission path with `deny` and **exit 0** — exit code 1
/// does NOT block in Claude Code; only the JSON path carries a reason. To allow,
/// emit nothing (no opinion); the normal permission flow continues.
pub fn format_claude_response(decision: &GateDecision) -> (String, i32) {
    if decision.decision == "block" {
        let reason = if decision.reasons.is_empty() {
            "blocked by policy".to_string()
        } else {
            decision
                .reasons
                .iter()
                .map(|r| format!("{}: {}", r.rule_id, r.message))
                .collect::<Vec<_>>()
                .join("; ")
        };
        let out = serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason,
            }
        });
        return (out.to_string(), 0);
    }
    // allow -> print nothing (NOT an approval, just "no opinion")
    (String::new(), 0)
}
