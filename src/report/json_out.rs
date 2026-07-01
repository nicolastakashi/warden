//! --format json — emit the decision record.

use serde_json::{Value, json};

use crate::results::CheckResult;

pub fn to_decision_record(check: &CheckResult) -> Value {
    let violations: Vec<Value> = check
        .results
        .iter()
        .filter(|r| !r.passed())
        .map(|r| {
            json!({
                "rule_id": r.rule.id,
                "enforcement": r.rule.enforcement,
                "match_type": r.rule.match_type,
                "locations": r.violations.iter().map(|v| json!({
                    "file": v.location.file,
                    "line": v.location.line,
                    "snippet": v.snippet,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    json!({
        "blocked": check.blocked,
        "rules_evaluated": check.results.len(),
        "violations": violations,
    })
}

pub fn render_json(check: &CheckResult) -> String {
    serde_json::to_string_pretty(&to_decision_record(check)).unwrap_or_default()
}
