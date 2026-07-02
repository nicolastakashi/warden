//! Ported from tests/test_matchers.py — pattern, structural, query, path scoping.
//! Adds a Go case to prove the structural backend is multi-language.

use std::collections::HashSet;

use warden::matchers::base::{CodeUnit, units_for_rule};
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
fn structural_glob_matches_package_at_any_depth() {
    // R6: a leading `**/` matches zero-or-more segments, so `**/foo/**` catches a
    // forbidden package whether imported top-level (`from foo.x`) or nested
    // (`from app.foo.x`) — the old footgun (top-level silently missed) is fixed.
    // The `to` glob matches the imported module as a slash path, no leading slash.
    fn forbid_to(to: &str) -> Rule {
        rule_from(&format!(
            "id: r\ndescription: d\nwhy: w\nscope: [ci]\nenforcement: block\nmatch:\n  type: structural\n  forbidden:\n    - from: \"app/**\"\n      to: \"{to}\"\n"
        ))
    }

    // top-level: `from legacy.helpers import x` -> module path "legacy/helpers"
    let top = CodeUnit::new("app/service.py", "from legacy.helpers import compute\n");
    assert_eq!(
        match_structural(std::slice::from_ref(&top), &forbid_to("legacy/**")).len(),
        1,
        "legacy/** matches a top-level package import"
    );
    assert_eq!(
        match_structural(std::slice::from_ref(&top), &forbid_to("**/legacy/**")).len(),
        1,
        "**/legacy/** now matches a top-level import too (footgun fixed)"
    );

    // nested: `from app.legacy.helpers import x` -> "app/legacy/helpers"
    let nested = CodeUnit::new("app/service.py", "from app.legacy.helpers import compute\n");
    assert_eq!(
        match_structural(&[nested], &forbid_to("**/legacy/**")).len(),
        1,
        "**/legacy/** matches a nested package import"
    );

    // bare: `import legacy` -> candidate "legacy" (no submodule). A `/**` glob
    // needs a segment after `legacy`, so only an exact `to: "legacy"` catches it.
    let bare = CodeUnit::new("app/service.py", "import legacy\n");
    assert_eq!(
        match_structural(std::slice::from_ref(&bare), &forbid_to("legacy")).len(),
        1,
        "an exact `to: legacy` catches a bare `import legacy`"
    );
    assert!(
        match_structural(std::slice::from_ref(&bare), &forbid_to("**/legacy/**")).is_empty(),
        "**/legacy/** does not match a bare `import legacy` (no submodule segment)"
    );
    assert!(
        match_structural(&[bare], &forbid_to("legacy/**")).is_empty(),
        "legacy/** does not catch a bare `import legacy`"
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

// --- query (rules-as-data via tree-sitter) ----------------------------------

fn no_unwrap_rule() -> Rule {
    rule_from(
        r#"
id: no-unwrap
description: no unwrap
why: w
scope: [ci]
enforcement: block
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
        "id: r\ndescription: d\nwhy: w\nscope: [ci]\nenforcement: block\n\
         match:\n  type: query\n  language: rust\n  query: \"(this is not valid scm\"\n",
    )
    .expect("YAML parses");
    let err = build_rule(&value, "t").expect_err("malformed query must be rejected");
    assert!(err.0.contains("query is invalid"), "got: {}", err.0);
}

#[test]
fn query_unknown_language_fails_validation() {
    let value: serde_norway::Value = serde_norway::from_str(
        "id: r\ndescription: d\nwhy: w\nscope: [ci]\nenforcement: block\n\
         match:\n  type: query\n  language: cobol\n  query: \"(identifier) @x\"\n",
    )
    .expect("YAML parses");
    let err = build_rule(&value, "t").expect_err("unknown language must be rejected");
    assert!(err.0.contains("unsupported"), "got: {}", err.0);
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
