//! Shared matcher types + path scoping.
//!
//! A matcher is `(units, rule) -> Vec<Violation>`. Matchers never know who
//! called them; that keeps the core agent-agnostic.

use crate::glob::fnmatch;
use crate::schema::Rule;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeUnit {
    pub path: String,
    pub content: String,
}

impl CodeUnit {
    pub fn new(path: impl Into<String>, content: impl Into<String>) -> Self {
        CodeUnit {
            path: path.into(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule_id: String,
    pub location: Location,
    pub reason: String,
    /// The offending source line, taken from the matcher's authoritative source
    /// (the matched line for `pattern`, the tree-sitter node for `structural`)
    /// so the line number and this text never come from two different places.
    pub snippet: String,
}

pub fn norm_path(path: &str) -> &str {
    path.strip_prefix("./").unwrap_or(path)
}

/// Apply a rule's optional `paths` scope. No `paths` -> every unit. Otherwise
/// the unit's path must match at least one glob (fnmatch, so `*` crosses `/`).
pub fn units_for_rule(units: &[CodeUnit], rule: &Rule) -> Vec<CodeUnit> {
    if rule.paths.is_empty() {
        return units.to_vec();
    }
    units
        .iter()
        .filter(|u| rule.paths.iter().any(|g| fnmatch(norm_path(&u.path), g)))
        .cloned()
        .collect()
}
