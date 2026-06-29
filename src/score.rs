//! Weighted score + bands.
//!
//! ```text
//! score = Σ(passed_i × weight_i) / Σ(total_i × weight_i) × 100
//! ```
//!
//! Scored set = ci rules with enforcement `block` or `warn`. `audit` is logged
//! only (excluded). Score is binary per rule; `extent` is recorded for output
//! but never weights the score. No scored rules -> 100.

use crate::results::RuleResult;

pub fn band(score: i64) -> &'static str {
    if score >= 90 {
        "Excellent"
    } else if score >= 70 {
        "Good"
    } else if score >= 50 {
        "Fair"
    } else {
        "Poor"
    }
}

fn is_scored(r: &RuleResult) -> bool {
    r.rule.enforcement == "block" || r.rule.enforcement == "warn"
}

/// Round half-to-even (banker's rounding), matching Python's `round`.
fn round_half_even(x: f64) -> i64 {
    let floor = x.floor();
    let diff = x - floor;
    if diff > 0.5 {
        floor as i64 + 1
    } else if diff < 0.5 {
        floor as i64
    } else {
        let fi = floor as i64;
        if fi % 2 == 0 { fi } else { fi + 1 }
    }
}

pub fn compute_score(results: &[RuleResult]) -> i64 {
    let scored: Vec<&RuleResult> = results.iter().filter(|r| is_scored(r)).collect();
    let total_weight: i64 = scored.iter().map(|r| r.rule.weight).sum();
    if total_weight == 0 {
        return 100;
    }
    let passed_weight: i64 = scored
        .iter()
        .filter(|r| r.passed())
        .map(|r| r.rule.weight)
        .sum();
    round_half_even(passed_weight as f64 / total_weight as f64 * 100.0)
}
