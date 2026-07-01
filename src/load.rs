//! Discover and parse `rules/*.yaml` into validated Rule objects.

use std::collections::HashMap;
use std::path::Path;

use serde_norway::Value;

use crate::schema::{Rule, RuleError, build_rule};

/// Parse and validate a single rule file into a `Rule`, naming the file in any
/// error. Used by `load_rules` (whole dir) and by `warden test` (one rule,
/// before it lives in the rules dir).
pub fn load_rule_file(path: &Path) -> Result<Rule, RuleError> {
    let display = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|e| RuleError(format!("{display}: {e}")))?;
    let value: Value = serde_norway::from_str(&text)
        .map_err(|e| RuleError(format!("{display}: invalid YAML: {e}")))?;
    build_rule(&value, &display)
}

/// Load every `*.yaml` under `dir` as one rule each. Errors on any invalid rule
/// or duplicate id, naming the file.
pub fn load_rules(dir: &Path) -> Result<Vec<Rule>, RuleError> {
    if !dir.is_dir() {
        return Err(RuleError(format!(
            "rules directory not found: {}",
            dir.display()
        )));
    }

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| RuleError(format!("{}: {e}", dir.display())))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("yaml"))
        .collect();
    files.sort();

    let mut rules: Vec<Rule> = Vec::new();
    let mut seen: HashMap<String, String> = HashMap::new();

    for path in files {
        let display = path.display().to_string();
        let rule = load_rule_file(&path)?;

        if let Some(other) = seen.get(&rule.id) {
            return Err(RuleError(format!(
                "{display}: duplicate rule id '{}' (also in {other})",
                rule.id
            )));
        }
        seen.insert(rule.id.clone(), display);
        rules.push(rule);
    }

    Ok(rules)
}
