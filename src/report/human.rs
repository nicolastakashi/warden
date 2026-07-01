//! --format human (default) — a counts summary, blocking failures first.

use crate::results::{CheckResult, RuleResult};

fn lines_for(result: &RuleResult, out: &mut Vec<String>) {
    for v in &result.violations {
        out.push(format!(
            "  {}:{} — {}",
            v.location.file, v.location.line, result.rule.description
        ));
        if !v.snippet.is_empty() {
            out.push(format!("      {}", v.snippet));
        }
    }
}

pub fn render_human(check: &CheckResult) -> String {
    let mut lines: Vec<String> = Vec::new();

    let fired: Vec<&RuleResult> = check.results.iter().filter(|r| !r.passed()).collect();
    let blocking: Vec<&&RuleResult> = fired
        .iter()
        .filter(|r| r.rule.enforcement == "block")
        .collect();
    let warnings: Vec<&&RuleResult> = fired
        .iter()
        .filter(|r| r.rule.enforcement == "warn")
        .collect();
    let audits: Vec<&&RuleResult> = fired
        .iter()
        .filter(|r| r.rule.enforcement == "audit")
        .collect();

    lines.push(format!(
        "{} files checked · {} blocking, {} warnings, {} audit   {}",
        check.files_checked,
        blocking.len(),
        warnings.len(),
        audits.len(),
        if check.blocked { "BLOCKED" } else { "PASS" }
    ));

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
        lines.push("Audit (logged only, does not block):".to_string());
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
