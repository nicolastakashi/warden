//! --format json — emit the decision record.

use serde_json::{Value, json};

use crate::results::CheckResult;

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

pub fn to_decision_record(check: &CheckResult) -> Value {
    let violations: Vec<Value> = check
        .results
        .iter()
        .filter(|r| !r.passed())
        .map(|r| {
            json!({
                "rule_id": r.rule.id,
                "enforcement": r.rule.enforcement,
                "weight": r.rule.weight,
                "match_type": r.rule.match_type,
                "extent": round4(r.extent),
                "locations": r.violations.iter().map(|v| json!({
                    "file": v.location.file,
                    "line": v.location.line,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    json!({
        "score": check.score,
        "band": check.band,
        "blocked": check.blocked,
        "rules_evaluated": check.results.len(),
        "violations": violations,
    })
}

pub fn render_json(check: &CheckResult) -> String {
    serde_json::to_string_pretty(&to_decision_record(check)).unwrap_or_default()
}
