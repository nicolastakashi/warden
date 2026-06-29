//! llm matcher — semantic check delegated to Claude Code, headless.
//!
//! Shells out to the local `claude` CLI in print mode (`claude -p`), riding the
//! developer's existing Claude Code login (OAuth) — no `ANTHROPIC_API_KEY`. It
//! parses defensively: a missing binary, unauthenticated session, nonzero exit,
//! or malformed JSON are all INCONCLUSIVE -> pass + warning. Skippable with
//! `--no-llm`. The `ClaudeRunner` trait isolates the process call so it can be
//! faked in tests.

use std::io::Write as _;
use std::process::{Command, Stdio};

use super::base::{CodeUnit, Location, Violation};
use crate::schema::{Match, Rule};

pub const DEFAULT_MODEL: &str = "claude-opus-4-8";
/// llm rules are for a diff or scoped subtree, not a whole repo. Past this many
/// chars, skip as inconclusive and tell the user to scope it.
pub const MAX_CHARS: usize = 200_000;

const JUDGE_SYSTEM: &str = "You are a deterministic policy checker. You judge code against ONE rule \
and reply with strict JSON ONLY — no prose, no markdown, no tools, no file reads. \
Output exactly one JSON object.";

/// The result of invoking the `claude` CLI (faked in tests).
pub struct ProcOutput {
    pub code: i32,
    pub stdout: String,
    #[allow(dead_code)]
    pub stderr: String,
}

pub trait ClaudeRunner {
    fn which(&self) -> bool;
    fn run(&self, prompt: &str, model: &str) -> std::io::Result<ProcOutput>;
}

/// The real runner — invokes the `claude` binary.
pub struct RealClaude;

fn which_on_path(bin: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let candidate = std::path::Path::new(dir).join(bin);
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

impl ClaudeRunner for RealClaude {
    fn which(&self) -> bool {
        which_on_path("claude")
    }

    fn run(&self, prompt: &str, model: &str) -> std::io::Result<ProcOutput> {
        let mut child = Command::new("claude")
            .args([
                "-p",
                "--output-format",
                "json",
                "--model",
                model,
                "--max-turns",
                "1",
                "--system-prompt",
                JUDGE_SYSTEM,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes())?;
        }
        let output = child.wait_with_output()?;
        Ok(ProcOutput {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

fn warn(rule_id: &str, msg: &str) {
    eprintln!("warning: llm rule '{rule_id}' inconclusive: {msg}");
}

fn render_code(units: &[CodeUnit]) -> String {
    let mut chunks = Vec::new();
    for unit in units {
        let numbered: Vec<String> = unit
            .content
            .lines()
            .enumerate()
            .map(|(i, ln)| format!("{:>4} | {ln}", i + 1))
            .collect();
        chunks.push(format!("=== FILE: {} ===\n{}", unit.path, numbered.join("\n")));
    }
    chunks.join("\n\n")
}

fn build_prompt(units: &[CodeUnit], rule: &Rule, prompt: &str) -> String {
    format!(
        "Policy rationale:\n{}\n\nRule to check:\n{}\n\nCode under review (line numbers are authoritative — cite them):\n\n{}\n\nReply with strict JSON only, shaped exactly like:\n{{\"violated\": true, \"locations\": [{{\"file\": \"path\", \"line\": 1, \"reason\": \"short reason\"}}]}}\nIf the rule is not violated, reply {{\"violated\": false, \"locations\": []}}.",
        rule.why,
        prompt,
        render_code(units)
    )
}

fn json_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn json_to_i64(v: &serde_json::Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        Some(n)
    } else if let Some(f) = v.as_f64() {
        Some(f as i64)
    } else {
        v.as_str().and_then(|s| s.trim().parse::<i64>().ok())
    }
}

/// Extract the assistant's result text from the `claude` JSON envelope, or an
/// error string describing why it was inconclusive.
fn run_claude(runner: &dyn ClaudeRunner, prompt: &str, model: &str) -> Result<String, String> {
    if !runner.which() {
        return Err("'claude' not found on PATH".to_string());
    }
    let out = runner.run(prompt, model).map_err(|e| e.to_string())?;
    if out.code != 0 {
        return Err(format!("claude exited {}", out.code));
    }
    let envelope: serde_json::Value =
        serde_json::from_str(&out.stdout).map_err(|e| format!("envelope not JSON: {e}"))?;
    if !envelope.is_object() {
        return Err("claude output was not a JSON object".to_string());
    }
    if envelope.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false)
        || envelope.get("type").and_then(|v| v.as_str()) != Some("result")
    {
        return Err("claude reported an error".to_string());
    }
    match envelope.get("result").and_then(|v| v.as_str()) {
        Some(s) => Ok(s.to_string()),
        None => Err("claude envelope had no 'result' text".to_string()),
    }
}

pub fn match_llm(
    units: &[CodeUnit],
    rule: &Rule,
    enabled: bool,
    model: &str,
    runner: &dyn ClaudeRunner,
) -> Vec<Violation> {
    let prompt_text = match &rule.matcher {
        Match::Llm(l) => &l.prompt,
        _ => return Vec::new(),
    };

    if !enabled {
        eprintln!("warning: llm rule '{}' skipped (--no-llm)", rule.id);
        return Vec::new();
    }
    if units.is_empty() {
        return Vec::new();
    }

    let total_chars: usize = units.iter().map(|u| u.content.chars().count()).sum();
    if total_chars > MAX_CHARS {
        warn(
            &rule.id,
            &format!(
                "{total_chars} chars across {} files exceeds the llm limit ({MAX_CHARS}); scope it",
                units.len()
            ),
        );
        return Vec::new();
    }

    let prompt = build_prompt(units, rule, prompt_text);
    let result_text = match run_claude(runner, &prompt, model) {
        Ok(t) => t,
        Err(e) => {
            warn(&rule.id, &e);
            return Vec::new();
        }
    };

    let verdict: serde_json::Value = match serde_json::from_str(&result_text) {
        Ok(v) => v,
        Err(e) => {
            warn(&rule.id, &format!("result not JSON: {e}"));
            return Vec::new();
        }
    };
    if !verdict.is_object()
        || !verdict.get("violated").and_then(|v| v.as_bool()).unwrap_or(false)
    {
        return Vec::new();
    }

    let mut violations = Vec::new();
    if let Some(locs) = verdict.get("locations").and_then(|v| v.as_array()) {
        for loc in locs {
            if !loc.is_object() {
                continue;
            }
            let file = match loc.get("file") {
                Some(v) => json_to_string(v),
                None => continue,
            };
            let line = match loc.get("line").and_then(json_to_i64) {
                Some(n) if n >= 0 => n as usize,
                _ => continue,
            };
            let reason = loc
                .get("reason")
                .map(json_to_string)
                .unwrap_or_default();
            violations.push(Violation {
                rule_id: rule.id.clone(),
                location: Location { file, line },
                reason,
            });
        }
    }
    violations
}
