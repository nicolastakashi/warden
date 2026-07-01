//! query matcher — arbitrary structural checks via tree-sitter queries.
//!
//! Where `structural` is hard-wired to forbidden imports, `query` is
//! rules-as-data: the rule *is* a tree-sitter query (`.scm`), so a new
//! structural check needs no engine code — just a query in a rule file
//! (e.g. "no `.unwrap()` in `src/**`"). Every captured node is one violation.
//!
//! Tree-sitter queries reference grammar-specific node kinds, so a query rule
//! targets exactly one `language:` and runs only on files of that language.
//! Covering several languages means several rules — the honest limit of
//! rules-as-data (see `docs/tree-sitter.md`).

use super::base::{CodeUnit, Location, Violation, norm_path};
use crate::lang::{compile_query, lang_by_name, lang_for_path, run_query};
use crate::schema::{Match, Rule};

pub fn match_query(units: &[CodeUnit], rule: &Rule) -> Vec<Violation> {
    let q = match &rule.matcher {
        Match::Query(q) => q,
        _ => return Vec::new(),
    };
    // `language` and `query` were validated (and the query compiled) when the
    // rule loaded, so failures here would be a bug — warn and fail open.
    let lang = match lang_by_name(&q.language) {
        Some(l) => l,
        None => return Vec::new(),
    };
    let query = match compile_query(lang, &q.query) {
        Ok(query) => query,
        Err(e) => {
            eprintln!("warning: query rule '{}' failed to compile: {e}", rule.id);
            return Vec::new();
        }
    };

    let mut violations = Vec::new();
    for unit in units {
        let path = norm_path(&unit.path);
        // A query only runs on files of its declared language.
        if lang_for_path(path) != Some(lang) {
            continue;
        }
        let hits = match run_query(&unit.content, lang, &query) {
            Some(h) => h,
            None => {
                // Supported language but the file didn't parse cleanly. Skip it
                // (fail-open) but warn, so grammar lag or genuinely broken code
                // doesn't silently switch off enforcement (see structural.rs).
                eprintln!(
                    "warning: {}: could not parse for query rule '{}'; skipping this file",
                    path, rule.id
                );
                continue;
            }
        };
        for (line, snippet) in hits {
            violations.push(Violation {
                rule_id: rule.id.clone(),
                location: Location {
                    file: unit.path.clone(),
                    line,
                },
                reason: format!("'{path}' matches query rule '{}'", rule.id),
                snippet,
            });
        }
    }
    violations
}
