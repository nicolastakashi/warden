//! Per-rule and overall result models shared by the CI gate and reporters.

use crate::matchers::Violation;
use crate::schema::Rule;

#[derive(Debug, Clone)]
pub struct RuleResult {
    pub rule: Rule,
    pub violations: Vec<Violation>,
}

impl RuleResult {
    pub fn passed(&self) -> bool {
        self.violations.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub results: Vec<RuleResult>,
    pub blocked: bool,
    pub files_checked: usize,
}
