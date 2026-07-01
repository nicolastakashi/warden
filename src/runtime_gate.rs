//! Consumer 2 — the runtime gate.
//!
//! Receives one proposed action, filters rules to scope contains `runtime`,
//! reads only `enforcement` (block/warn), ignores weight/score, returns
//! block/allow. The core only sees `ProposedAction` and returns `GateDecision`.

use crate::matchers::{CodeUnit, RealClaude, run_matcher, units_for_rule};
use crate::schema::Rule;

#[derive(Debug, Clone)]
pub struct ProposedAction {
    pub tool: String,
    pub path: Option<String>,
    pub content: Option<String>,
    pub command: Option<String>,
}

/// A rule that fired, with enough detail for the agent to fix it on the next
/// try: where it fired (with the offending line) and *why* the rule exists.
#[derive(Debug, Clone)]
pub struct Reason {
    pub rule_id: String,
    pub description: String,
    pub why: String,
    pub enforcement: String, // "block" | "warn"
    pub locations: Vec<ReasonLocation>,
}

/// One place a rule fired: `file:line` plus the offending source line.
#[derive(Debug, Clone)]
pub struct ReasonLocation {
    pub file: String,
    pub line: usize,
    pub snippet: String,
}

#[derive(Debug, Clone)]
pub struct GateDecision {
    pub decision: String, // "block" | "allow"
    pub reasons: Vec<Reason>,
}

impl GateDecision {
    fn allow() -> Self {
        GateDecision {
            decision: "allow".to_string(),
            reasons: Vec::new(),
        }
    }
}

fn unit_for(action: &ProposedAction) -> Option<CodeUnit> {
    if let Some(content) = &action.content {
        let path = action
            .path
            .clone()
            .unwrap_or_else(|| "<action>".to_string());
        return Some(CodeUnit {
            path,
            content: content.clone(),
        });
    }
    if let Some(command) = &action.command {
        return Some(CodeUnit {
            path: "<bash>".to_string(),
            content: command.clone(),
        });
    }
    None
}

pub fn evaluate_action(action: &ProposedAction, rules: &[Rule], no_llm: bool) -> GateDecision {
    let unit = match unit_for(action) {
        Some(u) => u,
        None => return GateDecision::allow(),
    };
    let units = vec![unit];
    let runner = RealClaude;

    let mut block = false;
    let mut reasons: Vec<Reason> = Vec::new();

    for rule in rules.iter().filter(|r| r.in_runtime()) {
        // audit rules never affect a runtime decision — skip before any work.
        if rule.enforcement == "audit" {
            continue;
        }
        let scoped = units_for_rule(&units, rule); // rule's paths must match
        if scoped.is_empty() {
            continue;
        }
        let violations = run_matcher(&scoped, rule, no_llm, &runner);
        if violations.is_empty() {
            continue;
        }

        block |= rule.enforcement == "block";
        let locations = violations
            .iter()
            .map(|v| ReasonLocation {
                file: v.location.file.clone(),
                line: v.location.line,
                snippet: v.snippet.clone(),
            })
            .collect();
        reasons.push(Reason {
            rule_id: rule.id.clone(),
            description: rule.description.clone(),
            why: rule.why.clone(),
            enforcement: rule.enforcement.clone(),
            locations,
        });
    }

    GateDecision {
        decision: if block { "block" } else { "allow" }.to_string(),
        reasons,
    }
}
