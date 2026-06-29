//! structural matcher — forbidden imports via tree-sitter (multi-language).
//!
//! A file whose path matches an edge's `from` glob may not import a module
//! whose slash-path matches that edge's `to` glob. Imports are extracted by the
//! language backend in `crate::lang`; one import statement yields at most one
//! violation (dedupe by line). Files in unsupported languages, or that don't
//! parse cleanly, are skipped.

use std::collections::HashSet;

use super::base::{CodeUnit, Location, Violation, norm_path};
use crate::glob::fnmatch;
use crate::lang::{import_candidates, lang_for_path};
use crate::schema::{Match, Rule};

pub fn match_structural(units: &[CodeUnit], rule: &Rule) -> Vec<Violation> {
    let forbidden = match &rule.matcher {
        Match::Structural(s) => &s.forbidden,
        _ => return Vec::new(),
    };

    let mut violations = Vec::new();
    for unit in units {
        let path = norm_path(&unit.path);
        let lang = match lang_for_path(path) {
            Some(l) => l,
            None => continue, // unsupported language — skip
        };

        // Which forbidden edges apply to this file?
        let edges: Vec<&crate::schema::ForbiddenImport> = forbidden
            .iter()
            .filter(|e| fnmatch(path, &e.from_))
            .collect();
        if edges.is_empty() {
            continue;
        }

        let candidates = match import_candidates(&unit.content, lang) {
            Some(c) => c,
            None => {
                // Supported language but the file didn't parse cleanly. We skip
                // it (fail-open), but warn — a silent skip here would hide both
                // genuinely broken code and tree-sitter grammar lag (valid
                // modern syntax the grammar doesn't yet recognize), turning off
                // enforcement on good code with no signal.
                eprintln!(
                    "warning: {}: could not parse for structural rule '{}'; skipping this file",
                    path, rule.id
                );
                continue;
            }
        };

        // `from X import Y` yields candidates X and X/Y; one import statement
        // must produce at most one violation, so dedupe by line number.
        let mut seen_lines: HashSet<usize> = HashSet::new();
        for (module_path, lineno) in candidates {
            if seen_lines.contains(&lineno) {
                continue;
            }
            for edge in &edges {
                if fnmatch(&module_path, &edge.to) {
                    seen_lines.insert(lineno);
                    violations.push(Violation {
                        rule_id: rule.id.clone(),
                        location: Location {
                            file: unit.path.clone(),
                            line: lineno,
                        },
                        reason: format!(
                            "'{path}' imports '{module_path}' (forbidden: {} -> {})",
                            edge.from_, edge.to
                        ),
                    });
                    break; // one violation per import is enough
                }
            }
        }
    }
    violations
}
