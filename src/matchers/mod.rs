//! Matchers + the shared dispatch used by both gates.

pub mod base;
pub mod llm;
pub mod pattern;
pub mod structural;

pub use base::{CodeUnit, Location, Violation, units_for_rule};
pub use llm::{ClaudeRunner, DEFAULT_MODEL, RealClaude, match_llm};
pub use pattern::match_pattern;
pub use structural::match_structural;

use crate::schema::Rule;

/// Run the matcher named by `rule.match_type` over `units`.
pub fn run_matcher(
    units: &[CodeUnit],
    rule: &Rule,
    no_llm: bool,
    runner: &dyn ClaudeRunner,
) -> Vec<Violation> {
    match rule.match_type.as_str() {
        "pattern" => match_pattern(units, rule),
        "structural" => match_structural(units, rule),
        "llm" => match_llm(units, rule, !no_llm, DEFAULT_MODEL, runner),
        _ => Vec::new(),
    }
}
