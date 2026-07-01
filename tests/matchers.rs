//! Ported from tests/test_matchers.py — pattern, structural, llm, path scoping.
//! Adds a Go case to prove the structural backend is multi-language.

use std::cell::RefCell;
use std::collections::HashSet;

use warden::matchers::base::{CodeUnit, units_for_rule};
use warden::matchers::llm::{ClaudeRunner, MAX_CHARS, ProcOutput, match_llm};
use warden::matchers::pattern::match_pattern;
use warden::matchers::query::match_query;
use warden::matchers::structural::match_structural;
use warden::schema::{Rule, build_rule};

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
    let unit = CodeUnit::new(
        "src/billing/charge.py",
        "from src.notifications import email\n",
    );
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
    let unit = CodeUnit::new(
        "src/api/handler.py",
        "from src.notifications import email\n",
    );
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
    let unit = CodeUnit::new(
        "src/billing/broken.py",
        "def (:\n  import src.notifications\n",
    );
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

#[test]
fn structural_glob_top_level_vs_nested_package() {
    // The `to` glob matches the imported module as a slash path with NO leading
    // slash, so `**/foo/**` (which needs a segment before `foo`) silently MISSES
    // a top-level import of `foo` — a fail-open footgun. Lock the behavior.
    fn forbid_to(to: &str) -> Rule {
        rule_from(&format!(
            "id: r\ndescription: d\nwhy: w\nscope: [ci]\nenforcement: block\nweight: 4\nmatch:\n  type: structural\n  forbidden:\n    - from: \"app/**\"\n      to: \"{to}\"\n"
        ))
    }

    // top-level: `from legacy.helpers import x` -> module path "legacy/helpers"
    let top = CodeUnit::new("app/service.py", "from legacy.helpers import compute\n");
    assert_eq!(
        match_structural(std::slice::from_ref(&top), &forbid_to("legacy/**")).len(),
        1,
        "legacy/** must match a top-level package import"
    );
    assert!(
        match_structural(&[top], &forbid_to("**/legacy/**")).is_empty(),
        "**/legacy/** must NOT match a top-level import (the footgun)"
    );

    // nested: `from app.legacy.helpers import x` -> "app/legacy/helpers"
    let nested = CodeUnit::new("app/service.py", "from app.legacy.helpers import compute\n");
    assert_eq!(
        match_structural(&[nested], &forbid_to("**/legacy/**")).len(),
        1,
        "**/legacy/** matches a nested package import"
    );

    // bare: `import legacy` -> candidate "legacy". Only an exact `to: "legacy"`
    // catches it; neither `legacy/**` nor `**/legacy/**` matches a bare name.
    let bare = CodeUnit::new("app/service.py", "import legacy\n");
    assert_eq!(
        match_structural(std::slice::from_ref(&bare), &forbid_to("legacy")).len(),
        1,
        "an exact `to: legacy` catches a bare `import legacy`"
    );
    assert!(
        match_structural(&[bare], &forbid_to("legacy/**")).is_empty(),
        "legacy/** does NOT catch a bare `import legacy`"
    );
}

#[test]
fn structural_multiline_import_points_at_the_offending_name() {
    // In a parenthesized multi-line import, the forbidden name is on line 3.
    // The violation must point there with a snippet naming it — not at the
    // `from` line with the opening `from a import (` fragment.
    let unit = CodeUnit::new(
        "app/service.py",
        "from a import (\n    b,\n    forbidden,\n)\n",
    );
    let rule = rule_from(
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
    - from: "app/**"
      to: "a/forbidden"
"#,
    );
    let v = match_structural(&[unit], &rule);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].location.line, 3, "points at the forbidden name's line");
    assert!(
        v[0].snippet.contains("forbidden"),
        "snippet names the offending symbol, got {:?}",
        v[0].snippet
    );
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

// --- query (rules-as-data via tree-sitter) ----------------------------------

fn no_unwrap_rule() -> Rule {
    rule_from(
        r#"
id: no-unwrap
description: no unwrap
why: w
scope: [ci]
enforcement: block
weight: 2
match:
  type: query
  language: rust
  query: |
    (call_expression
      function: (field_expression
        field: (field_identifier) @method)
      (#eq? @method "unwrap"))
"#,
    )
}

#[test]
fn query_fires_on_unwrap_but_not_lookalikes() {
    // The payoff over regex: it matches the real method call `.unwrap()` and
    // ignores `.unwrap_or(...)` and the substring "unwrap" in a string/ident.
    let src = "fn a(x: Option<i32>) -> i32 {\n    x.unwrap()\n}\n\
               fn b(x: Option<i32>) -> i32 {\n    x.unwrap_or(0)\n}\n\
               fn c() {\n    let unwrap = \"unwrap\";\n    let _ = unwrap;\n}\n";
    let v = match_query(&[CodeUnit::new("src/a.rs", src)], &no_unwrap_rule());
    assert_eq!(v.len(), 1, "only the real .unwrap() call fires");
    assert_eq!(v[0].location.line, 2);
    assert_eq!(v[0].snippet, "x.unwrap()");
}

#[test]
fn query_only_runs_on_its_declared_language() {
    // A `language: rust` query must not run against a .py file, even if the
    // path scope would include it.
    let py = CodeUnit::new("src/a.py", "x = maybe.unwrap()\n");
    assert!(
        match_query(&[py], &no_unwrap_rule()).is_empty(),
        "a rust query does not run on Python files"
    );
}

#[test]
fn query_clean_file_passes() {
    let clean = CodeUnit::new(
        "src/a.rs",
        "fn a(x: Option<i32>) -> i32 {\n    x.unwrap_or(0)\n}\n",
    );
    assert!(match_query(&[clean], &no_unwrap_rule()).is_empty());
}

#[test]
fn query_survives_syntax_error() {
    // Unparseable file -> fail open (skipped), matching the structural matcher.
    let broken = CodeUnit::new("src/a.rs", "fn a( { x.unwrap()\n");
    assert!(match_query(&[broken], &no_unwrap_rule()).is_empty());
}

#[test]
fn query_malformed_scm_fails_validation() {
    // A bad query must fail when the rule loads, not silently at runtime.
    let value: serde_norway::Value = serde_norway::from_str(
        "id: r\ndescription: d\nwhy: w\nscope: [ci]\nenforcement: block\nweight: 2\n\
         match:\n  type: query\n  language: rust\n  query: \"(this is not valid scm\"\n",
    )
    .expect("YAML parses");
    let err = build_rule(&value, "t").expect_err("malformed query must be rejected");
    assert!(err.0.contains("query is invalid"), "got: {}", err.0);
}

#[test]
fn query_unknown_language_fails_validation() {
    let value: serde_norway::Value = serde_norway::from_str(
        "id: r\ndescription: d\nwhy: w\nscope: [ci]\nenforcement: block\nweight: 2\n\
         match:\n  type: query\n  language: cobol\n  query: \"(identifier) @x\"\n",
    )
    .expect("YAML parses");
    let err = build_rule(&value, "t").expect_err("unknown language must be rejected");
    assert!(err.0.contains("unsupported"), "got: {}", err.0);
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
    let verdict = r#"{"violated": true, "locations": [{"file": "a.py", "line": 7, "reason": "logs an email"}]}"#;
    let fake = FakeClaude::ok(verdict);
    let v = match_llm(
        &[CodeUnit::new("a.py", "log(email)\n")],
        &llm_rule(),
        true,
        "m",
        &fake,
    );
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
    assert!(
        match_llm(
            &[CodeUnit::new("a.py", "x = 1\n")],
            &llm_rule(),
            true,
            "m",
            &fake
        )
        .is_empty()
    );
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
    assert!(
        match_llm(
            &[CodeUnit::new("a.py", "x")],
            &llm_rule(),
            false,
            "m",
            &fake
        )
        .is_empty()
    );
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
        HashSet::from([
            "src/billing/c.py".to_string(),
            "./src/billing/d.py".to_string()
        ])
    );
}
