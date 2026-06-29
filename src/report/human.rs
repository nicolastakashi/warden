//! --format human (default) — score + band, blocking failures first.

use crate::results::{CheckResult, RuleResult};

fn lines_for(result: &RuleResult, out: &mut Vec<String>) {
    for v in &result.violations {
        out.push(format!(
            "  {}:{} — {}",
            v.location.file, v.location.line, result.rule.description
        ));
    }
}

pub fn render_human(check: &CheckResult) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "Score: {}/100 ({})   files checked: {}   {}",
        check.score,
        check.band,
        check.files_checked,
        if check.blocked { "BLOCKED" } else { "PASS" }
    ));

    let fired: Vec<&RuleResult> = check.results.iter().filter(|r| !r.passed()).collect();
    let blocking: Vec<&&RuleResult> =
        fired.iter().filter(|r| r.rule.enforcement == "block").collect();
    let warnings: Vec<&&RuleResult> =
        fired.iter().filter(|r| r.rule.enforcement == "warn").collect();
    let audits: Vec<&&RuleResult> =
        fired.iter().filter(|r| r.rule.enforcement == "audit").collect();

    if !blocking.is_empty() {
        lines.push(String::new());
        lines.push("Blocking failures:".to_string());
        for r in &blocking {
            lines_for(r, &mut lines);
        }
    }
    if !warnings.is_empty() {
        lines.push(String::new());
        lines.push("Warnings:".to_string());
        for r in &warnings {
            lines_for(r, &mut lines);
        }
    }
    if !audits.is_empty() {
        lines.push(String::new());
        lines.push("Audit (logged only, not scored):".to_string());
        for r in &audits {
            lines_for(r, &mut lines);
        }
    }
    if fired.is_empty() {
        lines.push(String::new());
        lines.push("No rules fired.".to_string());
    }

    lines.join("\n")
}
