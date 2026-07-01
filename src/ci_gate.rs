//! Consumer 1 — the CI gate.
//!
//! Pipeline: gather units from a path -> filter to scope contains `ci` -> apply
//! each rule's `paths` -> run matchers (pattern, then structural, then query) ->
//! enforcement: any violated `block` rule -> blocked.

use std::path::{Path, PathBuf};

use crate::glob::fnmatch;
use crate::matchers::{CodeUnit, Violation, run_matcher, units_for_rule};
use crate::results::{CheckResult, RuleResult};
use crate::schema::Rule;

// Extensions treated as scannable text. Structural still self-filters by lang.
const TEXT_SUFFIXES: [&str; 16] = [
    ".py", ".pyi", ".txt", ".md", ".cfg", ".ini", ".toml", ".yaml", ".yml", ".json", ".env", ".sh",
    ".js", ".ts", ".go", ".rs",
];

// Directories never worth scanning — VCS metadata, build output, vendored deps.
// Skipped so `warden check .` doesn't raise violations from artifacts.
const IGNORE_DIRS: [&str; 9] = [
    ".git",
    "target",
    "node_modules",
    ".venv",
    "dist",
    "build",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
];

fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if IGNORE_DIRS.contains(&name) {
                    continue;
                }
                walk_files(&path, out);
            } else if path.is_file() {
                out.push(path);
            }
        }
    }
}

/// The literal directory prefix of a glob, before the first wildcard — the place
/// to start walking. `src/**/*.rs` -> `src`; `*.rs` -> `.`.
fn glob_base(pattern: &str) -> PathBuf {
    let first_wild = pattern.find(['*', '?', '[']).unwrap_or(pattern.len());
    match pattern[..first_wild].rfind('/') {
        Some(i) => PathBuf::from(&pattern[..i]),
        None => PathBuf::from("."),
    }
}

fn has_scannable_suffix(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => TEXT_SUFFIXES.contains(&format!(".{ext}").as_str()),
        None => true, // no extension -> treat as text (e.g. Makefile)
    }
}

/// Build CodeUnits from a path: a file, a directory (recursive), or a glob.
pub fn gather_units(target: &str) -> Vec<CodeUnit> {
    let path = Path::new(target);
    let mut files: Vec<PathBuf> = Vec::new();

    if path.is_dir() {
        walk_files(path, &mut files);
    } else if path.is_file() {
        files.push(path.to_path_buf());
    } else {
        // Treat the target as a glob, using the SAME fnmatch semantics as rule
        // `paths` (one glob behavior everywhere: `*` crosses `/`).
        let mut candidates = Vec::new();
        walk_files(&glob_base(target), &mut candidates);
        for p in candidates {
            let s = p.to_string_lossy();
            let norm = s.strip_prefix("./").unwrap_or(&s);
            if fnmatch(norm, target) {
                files.push(p);
            }
        }
    }
    files.sort();

    let mut units = Vec::new();
    for f in files {
        if !has_scannable_suffix(&f) {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&f) {
            units.push(CodeUnit {
                path: f.to_string_lossy().to_string(),
                content,
            });
        }
    }
    units
}

/// Dry-run **one** rule against a path (the engine behind `warden test`): gather
/// units, honour the rule's `paths`, run its matcher. Ignores `scope` — a
/// dry-run is "what would this rule catch here", regardless of ci/runtime.
/// Returns `(files_scanned_after_paths, violations)`.
pub fn run_rule(rule: &Rule, target: &str) -> (usize, Vec<Violation>) {
    let units = gather_units(target);
    let scoped = units_for_rule(&units, rule);
    let scanned = scoped.len();
    let violations = run_matcher(&scoped, rule);
    (scanned, violations)
}

fn match_order(match_type: &str) -> u8 {
    match match_type {
        "pattern" => 0,
        "structural" => 1,
        "query" => 2,
        _ => 3,
    }
}

pub fn run_check(target: &str, rules: &[Rule]) -> CheckResult {
    let units = gather_units(target);
    let files_checked = units.len();

    // pattern -> structural -> query, a deterministic layer order.
    let mut ci_rules: Vec<&Rule> = rules.iter().filter(|r| r.in_ci()).collect();
    ci_rules.sort_by_key(|r| match_order(&r.match_type));

    let mut results: Vec<RuleResult> = Vec::new();
    let mut blocked = false;

    for rule in ci_rules {
        let scoped = units_for_rule(&units, rule); // honour optional paths
        let violations = run_matcher(&scoped, rule);
        if !violations.is_empty() && rule.enforcement == "block" {
            blocked = true;
        }
        results.push(RuleResult {
            rule: rule.clone(),
            violations,
        });
    }

    CheckResult {
        results,
        blocked,
        files_checked,
    }
}
