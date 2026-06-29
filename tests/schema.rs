//! Ported from tests/test_schema.py — closed-schema validation.

use warden::schema::{Match, Rule, RuleError, build_rule};

fn build(yaml: &str) -> Result<Rule, RuleError> {
    let value: serde_norway::Value = serde_norway::from_str(yaml).expect("test YAML parses");
    build_rule(&value, "t")
}

const VALID_PATTERN: &str = r#"
id: no-env-vars
description: d
why: w
scope: [ci]
enforcement: block
weight: 4
match:
  type: pattern
  patterns: [x]
"#;

#[test]
fn valid_pattern_rule() {
    let r = build(VALID_PATTERN).unwrap();
    assert_eq!(r.match_type, "pattern");
    assert!(matches!(r.matcher, Match::Pattern(_)));
    assert!(r.in_ci() && !r.in_runtime());
}

#[test]
fn valid_structural_rule() {
    let r = build(
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
    - from: "a/**"
      to: "b/**"
"#,
    )
    .unwrap();
    match r.matcher {
        Match::Structural(s) => {
            assert_eq!(s.forbidden[0].from_, "a/**");
            assert_eq!(s.forbidden[0].to, "b/**");
        }
        _ => panic!("expected structural"),
    }
}

#[test]
fn valid_llm_rule() {
    let r = build(
        r#"
id: r
description: d
why: w
scope: [ci]
enforcement: block
weight: 4
match:
  type: llm
  prompt: check
"#,
    )
    .unwrap();
    assert!(matches!(r.matcher, Match::Llm(_)));
}

#[test]
fn invalid_rules_raise() {
    let cases = [
        // weight not in {1,2,4}
        r#"{id: r, description: d, why: w, scope: [ci], enforcement: block, weight: 3, match: {type: pattern, patterns: [x]}}"#,
        // bad enforcement
        r#"{id: r, description: d, why: w, scope: [ci], enforcement: halt, weight: 4, match: {type: pattern, patterns: [x]}}"#,
        // bad scope
        r#"{id: r, description: d, why: w, scope: [prod], enforcement: block, weight: 4, match: {type: pattern, patterns: [x]}}"#,
        // empty scope
        r#"{id: r, description: d, why: w, scope: [], enforcement: block, weight: 4, match: {type: pattern, patterns: [x]}}"#,
        // bad id
        r#"{id: "Not Kebab", description: d, why: w, scope: [ci], enforcement: block, weight: 4, match: {type: pattern, patterns: [x]}}"#,
        // bad match type
        r#"{id: r, description: d, why: w, scope: [ci], enforcement: block, weight: 4, match: {type: regex, patterns: [x]}}"#,
        // empty patterns
        r#"{id: r, description: d, why: w, scope: [ci], enforcement: block, weight: 4, match: {type: pattern, patterns: []}}"#,
        // unknown field — schema is closed
        r#"{id: r, description: d, why: w, scope: [ci], enforcement: block, weight: 4, extra: 1, match: {type: pattern, patterns: [x]}}"#,
    ];
    for (i, case) in cases.iter().enumerate() {
        assert!(build(case).is_err(), "case {i} should have failed: {case}");
    }
}

#[test]
fn missing_field_raises() {
    // base without `why`
    let r = build(
        r#"
id: r
description: d
scope: [ci]
enforcement: block
weight: 4
match:
  type: pattern
  patterns: [x]
"#,
    );
    assert!(r.is_err());
}

#[test]
fn paths_optional_defaults_empty() {
    let r = build(VALID_PATTERN).unwrap();
    assert!(r.paths.is_empty());
}

#[test]
fn paths_valid() {
    let r = build(
        r#"
id: r
description: d
why: w
scope: [ci]
enforcement: block
weight: 4
paths: ["src/**", "lib/*.py"]
match:
  type: pattern
  patterns: [x]
"#,
    )
    .unwrap();
    assert_eq!(r.paths, vec!["src/**".to_string(), "lib/*.py".to_string()]);
}

#[test]
fn paths_invalid_raises() {
    let bads = ["[]", r#""src/**""#, r#"[""]"#, "[1]", "{}"];
    for bad in bads {
        let yaml = format!(
            r#"{{id: r, description: d, why: w, scope: [ci], enforcement: block, weight: 4, paths: {bad}, match: {{type: pattern, patterns: [x]}}}}"#
        );
        assert!(build(&yaml).is_err(), "paths {bad} should have failed");
    }
}
