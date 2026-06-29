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

#[derive(Debug, Clone)]
pub struct Reason {
    pub rule_id: String,
    pub message: String,
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
        let scoped = units_for_rule(&units, rule); // rule's paths must match
        if scoped.is_empty() {
            continue;
        }
        let violations = run_matcher(&scoped, rule, no_llm, &runner);
        if violations.is_empty() {
            continue;
        }
        if rule.enforcement == "block" {
            block = true;
            reasons.push(Reason {
                rule_id: rule.id.clone(),
                message: rule.description.clone(),
            });
        } else if rule.enforcement == "warn" {
            reasons.push(Reason {
                rule_id: rule.id.clone(),
                message: format!("(warn) {}", rule.description),
            });
        }
        // audit rules never affect a runtime decision
    }

    GateDecision {
        decision: if block { "block" } else { "allow" }.to_string(),
        reasons,
    }
}
