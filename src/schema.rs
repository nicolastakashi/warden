//! The rule format — the single source of truth.
//!
//! One rule per file in `rules/*.yaml`. The schema is **closed**: unknown
//! top-level fields are rejected. The only optional field is `paths` (a list of
//! globs scoping a rule to a subtree; absent = all files). Exactly one `match`
//! type per rule.

use serde_norway::Value;

pub const SCOPES: [&str; 2] = ["ci", "runtime"];
pub const ENFORCEMENTS: [&str; 3] = ["block", "warn", "audit"];
pub const WEIGHTS: [i64; 3] = [1, 2, 4];
pub const MATCH_TYPES: [&str; 3] = ["pattern", "structural", "llm"];

/// Raised when a rule file fails validation.
#[derive(Debug, Clone)]
pub struct RuleError(pub String);

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for RuleError {}

#[derive(Debug, Clone)]
pub struct PatternMatch {
    /// regex over each line of a unit; flat list, OR-combined.
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ForbiddenImport {
    /// files under `from_` may not import `to` (both are file-path globs).
    pub from_: String,
    pub to: String,
}

#[derive(Debug, Clone)]
pub struct StructuralMatch {
    pub forbidden: Vec<ForbiddenImport>,
}

#[derive(Debug, Clone)]
pub struct LlmMatch {
    pub prompt: String,
}

#[derive(Debug, Clone)]
pub enum Match {
    Pattern(PatternMatch),
    Structural(StructuralMatch),
    Llm(LlmMatch),
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub id: String,
    pub description: String,
    pub why: String,
    pub scope: Vec<String>,
    pub enforcement: String,
    pub weight: i64,
    pub matcher: Match,
    /// "pattern" | "structural" | "llm"
    pub match_type: String,
    /// Optional path scope. Empty = applies to every file (the default).
    pub paths: Vec<String>,
}

impl Rule {
    pub fn in_ci(&self) -> bool {
        self.scope.iter().any(|s| s == "ci")
    }
    pub fn in_runtime(&self) -> bool {
        self.scope.iter().any(|s| s == "runtime")
    }
}

fn is_kebab(s: &str) -> bool {
    // ^[a-z0-9]+(?:-[a-z0-9]+)*$
    if s.is_empty() {
        return false;
    }
    let segments: Vec<&str> = s.split('-').collect();
    segments.iter().all(|seg| {
        !seg.is_empty()
            && seg
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    })
}

fn require<'a>(m: &'a Value, key: &str, whence: &str) -> Result<&'a Value, RuleError> {
    m.get(key)
        .ok_or_else(|| RuleError(format!("{whence}: missing required field '{key}'")))
}

fn build_match(data: &Value, whence: &str) -> Result<(String, Match), RuleError> {
    if data.as_mapping().is_none() {
        return Err(RuleError(format!("{whence}: 'match' must be a mapping")));
    }
    let mtype = require(data, "type", &format!("{whence}.match"))?
        .as_str()
        .ok_or_else(|| RuleError(format!("{whence}.match.type must be a string")))?
        .to_string();
    if !MATCH_TYPES.contains(&mtype.as_str()) {
        return Err(RuleError(format!(
            "{whence}.match.type '{mtype}' invalid; must be one of {MATCH_TYPES:?}"
        )));
    }

    match mtype.as_str() {
        "pattern" => {
            let raw = require(data, "patterns", &format!("{whence}.match"))?;
            let seq = raw.as_sequence().filter(|s| !s.is_empty());
            let patterns: Option<Vec<String>> = seq.and_then(|s| {
                s.iter()
                    .map(|v| v.as_str().map(|x| x.to_string()))
                    .collect()
            });
            match patterns {
                Some(p) => Ok((mtype, Match::Pattern(PatternMatch { patterns: p }))),
                None => Err(RuleError(format!(
                    "{whence}.match.patterns must be a non-empty list of strings"
                ))),
            }
        }
        "structural" => {
            let raw = require(data, "forbidden", &format!("{whence}.match"))?;
            let seq = raw.as_sequence().filter(|s| !s.is_empty()).ok_or_else(|| {
                RuleError(format!("{whence}.match.forbidden must be a non-empty list"))
            })?;
            let mut edges = Vec::new();
            for (i, edge) in seq.iter().enumerate() {
                let ew = format!("{whence}.match.forbidden[{i}]");
                if edge.as_mapping().is_none() {
                    return Err(RuleError(format!("{ew} must be a mapping")));
                }
                let frm = require(edge, "from", &ew)?;
                let to = require(edge, "to", &ew)?;
                match (frm.as_str(), to.as_str()) {
                    (Some(f), Some(t)) => edges.push(ForbiddenImport {
                        from_: f.to_string(),
                        to: t.to_string(),
                    }),
                    _ => return Err(RuleError(format!("{ew} 'from'/'to' must be strings"))),
                }
            }
            Ok((
                mtype,
                Match::Structural(StructuralMatch { forbidden: edges }),
            ))
        }
        _ => {
            // llm
            let raw = require(data, "prompt", &format!("{whence}.match"))?;
            match raw.as_str() {
                Some(p) if !p.trim().is_empty() => Ok((
                    mtype,
                    Match::Llm(LlmMatch {
                        prompt: p.to_string(),
                    }),
                )),
                _ => Err(RuleError(format!(
                    "{whence}.match.prompt must be a non-empty string"
                ))),
            }
        }
    }
}

/// Validate a parsed YAML mapping and build a Rule, or return RuleError.
pub fn build_rule(data: &Value, whence: &str) -> Result<Rule, RuleError> {
    let mapping = data
        .as_mapping()
        .ok_or_else(|| RuleError(format!("{whence}: top-level rule must be a mapping")))?;

    let id = require(data, "id", whence)?;
    let id = match id.as_str() {
        Some(s) if is_kebab(s) => s.to_string(),
        _ => {
            return Err(RuleError(format!("{whence}: id must be unique kebab-case")));
        }
    };

    let description = require(data, "description", whence)?
        .as_str()
        .ok_or_else(|| RuleError(format!("{whence}: 'description' must be a string")))?
        .to_string();
    let why = require(data, "why", whence)?
        .as_str()
        .ok_or_else(|| RuleError(format!("{whence}: 'why' must be a string")))?
        .to_string();

    let scope_raw = require(data, "scope", whence)?
        .as_sequence()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| RuleError(format!("{whence}: 'scope' must be a non-empty list")))?;
    let mut scope: Vec<String> = Vec::new();
    for s in scope_raw {
        let s = s
            .as_str()
            .filter(|x| SCOPES.contains(x))
            .ok_or_else(|| RuleError(format!("{whence}: scope invalid; subset of {SCOPES:?}")))?;
        if !scope.iter().any(|e| e == s) {
            scope.push(s.to_string()); // dedupe, preserve order
        }
    }

    let enforcement = require(data, "enforcement", whence)?
        .as_str()
        .filter(|e| ENFORCEMENTS.contains(e))
        .ok_or_else(|| {
            RuleError(format!(
                "{whence}: enforcement invalid; one of {ENFORCEMENTS:?}"
            ))
        })?
        .to_string();

    let weight = require(data, "weight", whence)?
        .as_i64()
        .filter(|w| WEIGHTS.contains(w))
        .ok_or_else(|| RuleError(format!("{whence}: weight invalid; one of {WEIGHTS:?}")))?;

    let (match_type, matcher) = build_match(require(data, "match", whence)?, whence)?;

    // Optional path scope (file-path globs). Absent/null -> applies to all files.
    let paths: Vec<String> = match data.get("paths") {
        None | Some(Value::Null) => Vec::new(),
        Some(v) => {
            let seq = v.as_sequence().filter(|s| !s.is_empty());
            let parsed: Option<Vec<String>> = seq.and_then(|s| {
                s.iter()
                    .map(|x| x.as_str().filter(|t| !t.is_empty()).map(|t| t.to_string()))
                    .collect()
            });
            parsed.ok_or_else(|| {
                RuleError(format!(
                    "{whence}: 'paths' must be a non-empty list of glob strings"
                ))
            })?
        }
    };

    // The schema is closed — no unknown top-level fields.
    let allowed = [
        "id",
        "description",
        "why",
        "scope",
        "enforcement",
        "weight",
        "match",
        "paths",
    ];
    let mut extra: Vec<String> = Vec::new();
    for key in mapping.keys() {
        match key.as_str() {
            Some(k) if allowed.contains(&k) => {}
            Some(k) => extra.push(k.to_string()),
            None => extra.push("<non-string key>".to_string()),
        }
    }
    if !extra.is_empty() {
        extra.sort();
        return Err(RuleError(format!(
            "{whence}: unknown field(s) {extra:?}; schema is closed"
        )));
    }

    Ok(Rule {
        id,
        description,
        why,
        scope,
        enforcement,
        weight,
        matcher,
        match_type,
        paths,
    })
}
