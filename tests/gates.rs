//! Ported from tests/test_gates.py — CI gate, runtime gate, adapter, paths.

use std::path::PathBuf;

use warden::adapters::claude_code::{format_claude_response, parse_claude_payload};
use warden::ci_gate::run_check;
use warden::load::load_rules;
use warden::report::json_out::to_decision_record;
use warden::runtime_gate::{GateDecision, ProposedAction, Reason, evaluate_action};
use warden::schema::{Rule, build_rule};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn rules() -> Vec<Rule> {
    load_rules(&root().join("demo").join("rules")).expect("demo rules load")
}
fn examples() -> String {
    root().join("examples").to_string_lossy().to_string()
}

fn write_action(path: &str, content: &str) -> ProposedAction {
    ProposedAction {
        tool: "Write".to_string(),
        path: Some(path.to_string()),
        content: Some(content.to_string()),
        command: None,
    }
}

// --- CI gate ----------------------------------------------------------------

#[test]
fn check_examples_blocks_with_score() {
    let check = run_check(&examples(), &rules(), true);
    assert!(check.blocked);
    assert_eq!(check.band, "Fair");
    assert_eq!(check.score, 60); // passed(4 struct + 2 llm-skipped) / total 10
    let rec = to_decision_record(&check);
    let fired: Vec<&str> = rec["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["rule_id"].as_str().unwrap())
        .collect();
    assert!(fired.contains(&"no-env-vars"));
    assert!(fired.contains(&"prefer-flag-helper"));
    let env = rec["violations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["rule_id"] == "no-env-vars")
        .unwrap();
    assert_eq!(env["enforcement"], "block");
    let extent = env["extent"].as_f64().unwrap();
    assert!(extent > 0.0 && extent <= 1.0);
}

// --- runtime gate -----------------------------------------------------------

#[test]
fn runtime_blocks_env_access() {
    let d = evaluate_action(&write_action("x.py", "v = os.getenv('A')"), &rules(), true);
    assert_eq!(d.decision, "block");
    assert_eq!(d.reasons[0].rule_id, "no-env-vars");
}

#[test]
fn runtime_allows_clean() {
    let d = evaluate_action(&write_action("x.py", "v = 1"), &rules(), true);
    assert_eq!(d.decision, "allow");
}

#[test]
fn runtime_scans_bash_command() {
    let action = ProposedAction {
        tool: "Bash".to_string(),
        path: None,
        content: None,
        command: Some("echo $(os.environ)".to_string()),
    };
    let d = evaluate_action(&action, &rules(), true);
    assert_eq!(d.decision, "block");
}

#[test]
fn runtime_ignores_ci_only_rules() {
    // no-cross-module-coupling is ci-only; a billing import must not block at runtime
    let d = evaluate_action(
        &write_action(
            "src/billing/charge.py",
            "from src.notifications import email",
        ),
        &rules(),
        true,
    );
    assert_eq!(d.decision, "allow");
}

// --- adapter ----------------------------------------------------------------

#[test]
fn parse_write_content() {
    let a = parse_claude_payload(
        r#"{"tool_name": "Write", "tool_input": {"file_path": "f.py", "content": "c"}}"#,
    )
    .unwrap();
    assert_eq!(a.tool, "Write");
    assert_eq!(a.path.as_deref(), Some("f.py"));
    assert_eq!(a.content.as_deref(), Some("c"));
}

#[test]
fn parse_write_file_text_alias() {
    let a = parse_claude_payload(
        r#"{"tool_name": "Write", "tool_input": {"file_path": "f.py", "file_text": "c"}}"#,
    )
    .unwrap();
    assert_eq!(a.content.as_deref(), Some("c"));
}

#[test]
fn parse_edit_uses_new_string() {
    let a = parse_claude_payload(
        r#"{"tool_name": "Edit", "tool_input": {"file_path": "f.py", "new_string": "n", "old_string": "o"}}"#,
    )
    .unwrap();
    assert_eq!(a.content.as_deref(), Some("n"));
}

#[test]
fn parse_bash_command() {
    let a =
        parse_claude_payload(r#"{"tool_name": "Bash", "tool_input": {"command": "ls"}}"#).unwrap();
    assert_eq!(a.command.as_deref(), Some("ls"));
}

#[test]
fn format_block_is_deny_json_exit_0() {
    let d = GateDecision {
        decision: "block".to_string(),
        reasons: vec![Reason {
            rule_id: "no-env-vars".to_string(),
            message: "use flags".to_string(),
        }],
    };
    let (text, code) = format_claude_response(&d);
    assert_eq!(code, 0);
    let out: serde_json::Value = serde_json::from_str(&text).unwrap();
    let hso = &out["hookSpecificOutput"];
    assert_eq!(hso["hookEventName"], "PreToolUse");
    assert_eq!(hso["permissionDecision"], "deny");
    assert!(
        hso["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .contains("no-env-vars")
    );
}

#[test]
fn format_allow_is_empty_exit_0() {
    let (text, code) = format_claude_response(&GateDecision {
        decision: "allow".to_string(),
        reasons: vec![],
    });
    assert!(text.is_empty());
    assert_eq!(code, 0);
}

// --- paths scoping in both gates --------------------------------------------

fn env_rule(scope: &str, paths: Option<&str>, enforcement: &str, weight: i64) -> Rule {
    let paths_line = paths.map(|p| format!("paths: {p}\n")).unwrap_or_default();
    let yaml = format!(
        "id: no-env\ndescription: no env access\nwhy: use flags\nscope: {scope}\nenforcement: {enforcement}\nweight: {weight}\n{paths_line}match:\n  type: pattern\n  patterns: ['os\\.getenv']\n"
    );
    let value: serde_norway::Value = serde_norway::from_str(&yaml).unwrap();
    build_rule(&value, "t").unwrap()
}

#[test]
fn runtime_gate_respects_paths() {
    let rule = env_rule("[runtime]", Some(r#"["src/feature/**"]"#), "block", 4);
    let outside = evaluate_action(
        &write_action("settings.py", "x = os.getenv('A')"),
        std::slice::from_ref(&rule),
        true,
    );
    assert_eq!(outside.decision, "allow");
    let inside = evaluate_action(
        &write_action("src/feature/x.py", "x = os.getenv('A')"),
        std::slice::from_ref(&rule),
        true,
    );
    assert_eq!(inside.decision, "block");
}

#[test]
fn check_path_scoped_rule_only_fires_in_scope() {
    let dir = std::env::temp_dir().join("warden_rs_test_scoped");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("a")).unwrap();
    std::fs::create_dir_all(dir.join("b")).unwrap();
    std::fs::write(dir.join("a").join("x.py"), "v = os.getenv('A')\n").unwrap();
    std::fs::write(dir.join("b").join("y.py"), "v = os.getenv('A')\n").unwrap();

    let rule = env_rule("[ci]", Some(r#"["**/a/**"]"#), "warn", 2);
    let check = run_check(&dir.to_string_lossy(), std::slice::from_ref(&rule), true);
    let fired: Vec<String> = check
        .results
        .iter()
        .flat_map(|r| r.violations.iter().map(|v| v.location.file.clone()))
        .collect();
    assert!(fired.iter().any(|f| f.ends_with("a/x.py")));
    assert!(!fired.iter().any(|f| f.ends_with("b/y.py")));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_gate_roundtrip_block() {
    let payload = r#"{"tool_name": "Write", "tool_input": {"file_path": "x.py", "content": "v = os.getenv('A')"}}"#;
    let action = parse_claude_payload(payload).unwrap();
    let decision = evaluate_action(&action, &rules(), true);
    let (text, code) = format_claude_response(&decision);
    assert_eq!(code, 0);
    let out: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "deny");
}

// --- Edit reconstruction: the gate judges the post-edit file, not the fragment -

fn struct_runtime_rule() -> Rule {
    let value: serde_norway::Value = serde_norway::from_str(
        r#"
id: no-cross-module-coupling
description: "Billing must not import notifications"
why: w
scope: [runtime]
enforcement: block
weight: 4
match:
  type: structural
  forbidden:
    - from: "**/billing/**"
      to: "**/notifications/**"
"#,
    )
    .unwrap();
    build_rule(&value, "t").unwrap()
}

#[test]
fn edit_is_evaluated_as_the_resulting_file_not_the_fragment() {
    // An Edit adds a forbidden import *inside a function* — the new_string is an
    // indented fragment that does NOT parse standalone (tree-sitter sees an
    // unexpected indent and bails), so a structural rule run against the raw
    // fragment would silently skip it. The gate must reconstruct the full file
    // from disk; only then does the import parse and the rule fire. (Run against
    // the fragment alone this test asserts `block` but would get `allow`.)
    let dir = std::env::temp_dir().join("warden_rs_test_edit");
    let _ = std::fs::remove_dir_all(&dir);
    let billing = dir.join("billing");
    std::fs::create_dir_all(&billing).unwrap();
    let file = billing.join("charge.py");
    std::fs::write(&file, "def charge():\n    pass\n").unwrap();

    let payload = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {
            "file_path": file.to_string_lossy(),
            "old_string": "    pass",
            "new_string": "    from src.notifications import email\n    return email",
        }
    })
    .to_string();

    let action = parse_claude_payload(&payload).unwrap();
    // content is the full resulting file, not just the indented fragment
    let content = action.content.as_deref().unwrap();
    assert!(
        content.contains("def charge():"),
        "should be the whole file"
    );
    assert!(content.contains("from src.notifications import email"));

    let decision = evaluate_action(&action, std::slice::from_ref(&struct_runtime_rule()), true);
    assert_eq!(decision.decision, "block");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_falls_back_to_fragment_when_file_missing() {
    // No file on disk -> use the new_string fragment (previous behavior).
    let payload = r#"{"tool_name": "Edit", "tool_input": {"file_path": "definitely/missing/x.py", "old_string": "o", "new_string": "n"}}"#;
    let action = parse_claude_payload(payload).unwrap();
    assert_eq!(action.content.as_deref(), Some("n"));
}

#[test]
fn edit_replace_all_semantics() {
    // replace_all toggles between every-occurrence and first-only reconstruction.
    let dir = std::env::temp_dir().join("warden_rs_test_edit_all");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("x.py");
    std::fs::write(&file, "a = 1\nb = 1\n").unwrap();

    // replace_all: true -> every occurrence
    let all = parse_claude_payload(
        &serde_json::json!({
            "tool_name": "Edit",
            "tool_input": {
                "file_path": file.to_string_lossy(),
                "old_string": "1", "new_string": "2", "replace_all": true
            }
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(all.content.as_deref(), Some("a = 2\nb = 2\n"));

    // default (replace_all absent) -> first occurrence only
    let first = parse_claude_payload(
        &serde_json::json!({
            "tool_name": "Edit",
            "tool_input": {
                "file_path": file.to_string_lossy(),
                "old_string": "1", "new_string": "2"
            }
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(first.content.as_deref(), Some("a = 2\nb = 1\n"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_falls_back_when_old_string_absent() {
    // File is readable but old_string isn't in it (a degenerate edit) -> don't
    // emit a no-op file; fall back to the fragment.
    let dir = std::env::temp_dir().join("warden_rs_test_edit_absent");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("x.py");
    std::fs::write(&file, "real file content\n").unwrap();

    let payload = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {
            "file_path": file.to_string_lossy(),
            "old_string": "NOT PRESENT", "new_string": "frag"
        }
    })
    .to_string();
    let action = parse_claude_payload(&payload).unwrap();
    assert_eq!(action.content.as_deref(), Some("frag"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_pattern_rule_blocks_on_reconstructed_file() {
    // A `pattern` rule also runs against the reconstructed file. The new_string
    // here would match on its own too (pattern is per-line), so this confirms
    // the Edit path reaches pattern rules and scans the full resulting file.
    let dir = std::env::temp_dir().join("warden_rs_test_edit_pat");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("x.py");
    std::fs::write(&file, "def f():\n    pass\n").unwrap();

    let payload = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {
            "file_path": file.to_string_lossy(),
            "old_string": "    pass",
            "new_string": "    t = os.getenv(\"X\")"
        }
    })
    .to_string();
    let action = parse_claude_payload(&payload).unwrap();
    assert!(
        action.content.as_deref().unwrap().contains("def f():"),
        "scans the full reconstructed file"
    );

    let rule = env_rule("[runtime]", None, "block", 4);
    let decision = evaluate_action(&action, std::slice::from_ref(&rule), true);
    assert_eq!(decision.decision, "block");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_empty_old_string_does_not_fabricate_content() {
    // An empty old_string is not a real edit; `contains("")` is always true, so
    // without a guard the reconstruction would prepend/interleave new_string
    // into the file. It must fall back to the fragment, not fabricate content.
    let dir = std::env::temp_dir().join("warden_rs_test_edit_empty");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("x.py");
    std::fs::write(&file, "keep this\n").unwrap();

    let payload = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {
            "file_path": file.to_string_lossy(),
            "old_string": "", "new_string": "frag"
        }
    })
    .to_string();
    let action = parse_claude_payload(&payload).unwrap();
    assert_eq!(action.content.as_deref(), Some("frag"));

    let _ = std::fs::remove_dir_all(&dir);
}
