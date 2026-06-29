//! Ported from tests/test_score.py — weighted score + bands.

use warden::matchers::{Location, Violation};
use warden::results::RuleResult;
use warden::schema::{build_rule, Rule};
use warden::score::{band, compute_score};

fn rule(id: &str, enforcement: &str, weight: i64) -> Rule {
    let yaml = format!(
        r#"{{id: {id}, description: d, why: w, scope: [ci], enforcement: {enforcement}, weight: {weight}, match: {{type: pattern, patterns: [x]}}}}"#
    );
    let value: serde_norway::Value = serde_norway::from_str(&yaml).unwrap();
    build_rule(&value, "t").unwrap()
}

fn result(id: &str, enforcement: &str, weight: i64, fired: bool) -> RuleResult {
    let violations = if fired {
        vec![Violation {
            rule_id: id.to_string(),
            location: Location {
                file: "f.py".to_string(),
                line: 1,
            },
            reason: String::new(),
        }]
    } else {
        Vec::new()
    };
    RuleResult {
        rule: rule(id, enforcement, weight),
        violations,
        extent: if fired { 1.0 } else { 0.0 },
    }
}

#[test]
fn bands() {
    assert_eq!(band(95), "Excellent");
    assert_eq!(band(70), "Good");
    assert_eq!(band(50), "Fair");
    assert_eq!(band(49), "Poor");
}

#[test]
fn audit_excluded_from_score() {
    let results = vec![
        result("a", "block", 4, false), // passed, weight 4
        result("b", "audit", 1, true),  // fired but audit → not scored
    ];
    assert_eq!(compute_score(&results), 100);
}

#[test]
fn warn_counts_in_score() {
    let results = vec![
        result("a", "block", 4, true),  // failed, weight 4
        result("b", "warn", 2, false),  // passed, weight 2
    ];
    // passed weight 2 / total 6 = 33
    assert_eq!(compute_score(&results), 33);
}

#[test]
fn zero_case_is_100_not_crash() {
    assert_eq!(compute_score(&[]), 100);
    assert_eq!(compute_score(&[result("a", "audit", 1, true)]), 100);
}
