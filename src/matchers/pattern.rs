//! pattern matcher — regex over each line of a unit.
//!
//! `patterns` is a flat list, OR-combined. A line matching any pattern is one
//! violation at that line (1-based). A "unit" is a whole file in the CI gate, or
//! the proposed new content in the runtime gate — there is no diff parsing.

use regex::Regex;

use super::base::{CodeUnit, Location, Violation};
use crate::schema::{Match, Rule};

pub fn match_pattern(units: &[CodeUnit], rule: &Rule) -> Vec<Violation> {
    let patterns = match &rule.matcher {
        Match::Pattern(p) => &p.patterns,
        _ => return Vec::new(),
    };
    // Invalid regexes are skipped (a broken rule must not crash the gate).
    let compiled: Vec<Regex> = patterns.iter().filter_map(|p| Regex::new(p).ok()).collect();

    let mut violations = Vec::new();
    for unit in units {
        for (i, line) in unit.content.lines().enumerate() {
            let lineno = i + 1;
            for re in &compiled {
                if re.is_match(line) {
                    violations.push(Violation {
                        rule_id: rule.id.clone(),
                        location: Location {
                            file: unit.path.clone(),
                            line: lineno,
                        },
                        reason: format!("matched /{}/", re.as_str()),
                        snippet: line.trim().to_string(),
                    });
                    break; // one violation per line is enough
                }
            }
        }
    }
    violations
}
