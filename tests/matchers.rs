//! Ported from tests/test_matchers.py — pattern, structural, llm, path scoping.
//! Adds a Go case to prove the structural backend is multi-language.

use std::cell::RefCell;
use std::collections::HashSet;

use warden::matchers::base::{units_for_rule, CodeUnit};
use warden::matchers::llm::{match_llm, ClaudeRunner, ProcOutput, MAX_CHARS};
use warden::matchers::pattern::match_pattern;
use warden::matchers::structural::match_structural;
use warden::schema::{build_rule, Rule};

fn rule_from(yaml: &str) -> Rule {
    let value: serde_norway::Value = serde_norway::from_str(yaml).expect("test YAML parses");
    build_rule(&value, "t").expect("rule builds")
}

// --- pattern ----------------------------------------------------------------

#[test]
fn pattern_matches_lines_and_reports_line_numbers() {
    let rule = rule_from(
        r#"
id: r
description: d
why: w
scope: [ci]
enforcement: block
weight: 4
match:
  type: pattern
  patterns: ['os\.getenv', 'os\.environ']
"#,
    );
    let unit = CodeUnit::new("a.py", "clean\nx = os.getenv('A')\ny = os.environ['B']\n");
    let v = match_pattern(&[unit], &rule);
    let lines: HashSet<usize> = v.iter().map(|x| x.location.line).collect();
    assert_eq!(lines, HashSet::from([2, 3]));
    assert!(v.iter().all(|x| x.location.file == "a.py"));
}

#[test]
fn pattern_no_match() {
    let rule = rule_from(
        r#"
id: r
description: d
why: w
scope: [ci]
enforcement: block
weight: 4
match:
  type: pattern
  patterns: ['os\.getenv']
"#,
    );
    assert!(match_pattern(&[CodeUnit::new("a.py", "x = 1\n")], &rule).is_empty());
}

// --- structural -------------------------------------------------------------

fn struct_rule() -> Rule {
    rule_from(
        r#"
id: r
description: d
why: w
scope: [ci]
enforcement: block
weight: 4
match:
  type: structural
  forbidden:
    - from: "**/billing/**"
      to: "**/notifications/**"
"#,
    )
}

#[test]
fn structural_fires_on_import_from() {
    let unit = CodeUnit::new("src/billing/charge.py", "from src.notifications import email\n");
    let v = match_structural(&[unit], &struct_rule());
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].location.line, 1);
}

#[test]
fn structural_fires_on_dotted_import() {
    let unit = CodeUnit::new("src/billing/charge.py", "import src.notifications.email\n");
    assert_eq!(match_structural(&[unit], &struct_rule()).len(), 1);
}

#[test]
fn structural_dedupes_one_violation_per_import() {
    let unit = CodeUnit::new(
        "src/billing/charge.py",
        "from src.notifications.email import send_receipt\n",
    );
    let v = match_structural(&[unit], &struct_rule());
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].location.line, 1);
}

#[test]
fn structural_clean_file_passes() {
    let unit = CodeUnit::new(
        "src/billing/charge.py",
        "import os\nfrom src.billing import db\n",
    );
    assert!(match_structural(&[unit], &struct_rule()).is_empty());
}

#[test]
fn structural_from_glob_scopes_the_source_file() {
    let unit = CodeUnit::new("src/api/handler.py", "from src.notifications import email\n");
    assert!(match_structural(&[unit], &struct_rule()).is_empty());
}

#[test]
fn structural_skips_relative_imports() {
    let unit = CodeUnit::new("src/billing/charge.py", "from . import notifications\n");
    assert!(match_structural(&[unit], &struct_rule()).is_empty());
}

#[test]
fn structural_skips_non_supported_language() {
    let unit = CodeUnit::new("src/billing/notes.md", "import src.notifications.email\n");
    assert!(match_structural(&[unit], &struct_rule()).is_empty());
}

#[test]
fn structural_survives_syntax_error() {
    let unit = CodeUnit::new("src/billing/broken.py", "def (:\n  import src.notifications\n");
    assert!(match_structural(&[unit], &struct_rule()).is_empty());
}

#[test]
fn structural_is_multi_language_go() {
    // The whole point of the tree-sitter backend: the same rule works on Go.
    let bad = CodeUnit::new(
        "src/billing/charge.go",
        "package billing\nimport \"myapp/notifications/email\"\n",
    );
    let v = match_structural(&[bad], &struct_rule());
    assert_eq!(v.len(), 1, "Go import of notifications must fire");
    assert_eq!(v[0].location.line, 2);

    let good = CodeUnit::new(
        "src/billing/charge.go",
        "package billing\nimport \"myapp/billing/db\"\n",
    );
    assert!(match_structural(&[good], &struct_rule()).is_empty());
}

// --- llm (no live claude — fake the runner) ---------------------------------

struct FakeClaude {
    which: bool,
    code: i32,
    stdout: String,
    panic_on_run: bool,
    captured: RefCell<Option<(String, String)>>,
}

impl FakeClaude {
    fn ok(result_text: &str) -> Self {
        let envelope = serde_json::json!({
            "type": "result", "subtype": "success", "is_error": false, "result": result_text
        });
        FakeClaude {
            which: true,
            code: 0,
            stdout: envelope.to_string(),
            panic_on_run: false,
            captured: RefCell::new(None),
        }
    }
}

impl ClaudeRunner for FakeClaude {
    fn which(&self) -> bool {
        self.which
    }
    fn run(&self, prompt: &str, model: &str) -> std::io::Result<ProcOutput> {
        if self.panic_on_run {
            panic!("claude must not be invoked");
        }
        *self.captured.borrow_mut() = Some((prompt.to_string(), model.to_string()));
        Ok(ProcOutput {
            code: self.code,
            stdout: self.stdout.clone(),
            stderr: String::new(),
        })
    }
}

fn llm_rule() -> Rule {
    rule_from(
        r#"
id: r
description: d
why: w
scope: [ci]
enforcement: block
weight: 4
match:
  type: llm
  prompt: flag PII in logs
"#,
    )
}

#[test]
fn llm_success_maps_verdict_to_violations() {
    let verdict =
        r#"{"violated": true, "locations": [{"file": "a.py", "line": 7, "reason": "logs an email"}]}"#;
    let fake = FakeClaude::ok(verdict);
    let v = match_llm(&[CodeUnit::new("a.py", "log(email)\n")], &llm_rule(), true, "m", &fake);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].location.file, "a.py");
    assert_eq!(v[0].location.line, 7);
    assert!(v[0].reason.contains("email"));
    let captured = fake.captured.borrow();
    let (prompt, model) = captured.as_ref().expect("claude was invoked");
    assert!(!prompt.is_empty());
    assert_eq!(model, "m");
}

#[test]
fn llm_not_violated_returns_empty() {
    let fake = FakeClaude::ok(r#"{"violated": false, "locations": []}"#);
    assert!(match_llm(&[CodeUnit::new("a.py", "x = 1\n")], &llm_rule(), true, "m", &fake).is_empty());
}

#[test]
fn llm_disabled_skips_without_calling() {
    let fake = FakeClaude {
        which: true,
        code: 0,
        stdout: String::new(),
        panic_on_run: true,
        captured: RefCell::new(None),
    };
    assert!(match_llm(&[CodeUnit::new("a.py", "x")], &llm_rule(), false, "m", &fake).is_empty());
}

#[test]
fn llm_malformed_result_is_inconclusive_pass() {
    let fake = FakeClaude::ok("not json at all");
    assert!(match_llm(&[CodeUnit::new("a.py", "x")], &llm_rule(), true, "m", &fake).is_empty());
}

#[test]
fn llm_cli_error_is_inconclusive_pass() {
    let mut fake = FakeClaude::ok("");
    fake.code = 1;
    assert!(match_llm(&[CodeUnit::new("a.py", "x")], &llm_rule(), true, "m", &fake).is_empty());
}

#[test]
fn llm_missing_binary_is_inconclusive_pass() {
    let mut fake = FakeClaude::ok("");
    fake.which = false;
    assert!(match_llm(&[CodeUnit::new("a.py", "x")], &llm_rule(), true, "m", &fake).is_empty());
}

#[test]
fn llm_oversized_input_is_skipped_without_calling() {
    let fake = FakeClaude {
        which: true,
        code: 0,
        stdout: String::new(),
        panic_on_run: true,
        captured: RefCell::new(None),
    };
    let huge = CodeUnit::new("big.py", "x".repeat(MAX_CHARS + 1));
    assert!(match_llm(&[huge], &llm_rule(), true, "m", &fake).is_empty());
}

// --- units_for_rule (path scoping) ------------------------------------------

#[test]
fn units_for_rule_no_paths_returns_all() {
    let units = vec![CodeUnit::new("a.py", ""), CodeUnit::new("b/c.py", "")];
    let rule = rule_from(
        r#"
id: r
description: d
why: w
scope: [ci]
enforcement: block
weight: 4
match:
  type: pattern
  patterns: [x]
"#,
    );
    assert_eq!(units_for_rule(&units, &rule), units);
}

#[test]
fn units_for_rule_filters_by_glob() {
    let units = vec![
        CodeUnit::new("src/api/h.py", ""),
        CodeUnit::new("src/billing/c.py", ""),
        CodeUnit::new("./src/billing/d.py", ""),
    ];
    let rule = rule_from(
        r#"
id: r
description: d
why: w
scope: [ci]
enforcement: block
weight: 4
paths: ["src/billing/**"]
match:
  type: pattern
  patterns: [x]
"#,
    );
    let got: HashSet<String> = units_for_rule(&units, &rule)
        .into_iter()
        .map(|u| u.path)
        .collect();
    assert_eq!(
        got,
        HashSet::from(["src/billing/c.py".to_string(), "./src/billing/d.py".to_string()])
    );
}
