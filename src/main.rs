//! The warden CLI: subcommands `validate`, `check`, `gate`.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use warden::adapters::claude_code::{format_claude_response, parse_claude_payload};
use warden::ci_gate::{run_check, run_rule};
use warden::load::{load_rule_file, load_rules};
use warden::matchers::Violation;
use warden::report::human::render_human;
use warden::report::json_out::render_json;
use warden::runtime_gate::evaluate_action;
use warden::schema::Rule;

#[derive(Parser)]
#[command(
    name = "warden",
    about = "A deterministic, agent-agnostic policy engine for AI-agent code."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// validate rules/*.yaml
    Validate {
        #[arg(long)]
        rules: Option<PathBuf>,
    },
    /// run the CI gate over a path/glob
    Check {
        /// path, directory, or glob to check
        path: String,
        #[arg(long)]
        rules: Option<PathBuf>,
        #[arg(long, default_value = "human", value_parser = ["human", "json"])]
        format: String,
    },
    /// runtime gate: read the hook payload on stdin
    Gate {
        #[arg(long)]
        rules: Option<PathBuf>,
    },
    /// dry-run ONE rule against a path — see what it catches before it lands
    Test {
        /// the rule file to try (a `rules/*.yaml`)
        rule: PathBuf,
        /// path, directory, or glob to run it against
        path: String,
    },
}

/// Human-facing `warden test` output: what the rule caught, `file:line → snippet`.
fn render_test(rule: &Rule, scanned: usize, violations: &[Violation]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "rule: {} [{}, {}]",
        rule.id, rule.match_type, rule.enforcement
    );
    if scanned == 0 {
        let _ = write!(
            out,
            "0 files matched — check the path/glob, or this rule's `paths` scope."
        );
        return out;
    }
    if violations.is_empty() {
        let _ = write!(
            out,
            "scanned {scanned} file(s) · 0 matches — this rule fired on nothing here."
        );
        return out;
    }
    let _ = writeln!(
        out,
        "scanned {scanned} file(s) · {} match(es):",
        violations.len()
    );
    for v in violations {
        if v.snippet.is_empty() {
            let _ = writeln!(out, "  {}:{}", v.location.file, v.location.line);
        } else {
            let _ = writeln!(
                out,
                "  {}:{} → {}",
                v.location.file, v.location.line, v.snippet
            );
        }
    }
    out.truncate(out.trim_end().len());
    out
}

/// Locate the rules directory: `--rules` -> `$CLAUDE_PROJECT_DIR/rules` -> `./rules`.
fn rules_dir(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = explicit {
        return dir;
    }
    if let Ok(project) = std::env::var("CLAUDE_PROJECT_DIR") {
        return PathBuf::from(project).join("rules");
    }
    PathBuf::from("rules")
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { rules } => {
            let dir = rules_dir(rules);
            match load_rules(&dir) {
                Ok(rules) => {
                    println!("ok: {} rule(s) valid", rules.len());
                    for r in &rules {
                        println!(
                            "  - {} [{}, {}, scope {}]",
                            r.id,
                            r.match_type,
                            r.enforcement,
                            r.scope.join(",")
                        );
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("invalid: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Check {
            path,
            rules,
            format,
        } => {
            let dir = rules_dir(rules);
            let rules = match load_rules(&dir) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error loading rules: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let check = run_check(&path, &rules);
            if format == "json" {
                println!("{}", render_json(&check));
            } else {
                println!("{}", render_human(&check));
            }
            if check.blocked {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Command::Gate { rules } => {
            let dir = rules_dir(rules);
            // A broken rule set must not crash the agent — emit no opinion.
            let rules = match load_rules(&dir) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("warning: gate could not load rules: {e}");
                    return ExitCode::SUCCESS;
                }
            };
            let mut stdin_json = String::new();
            if std::io::stdin().read_to_string(&mut stdin_json).is_err() {
                return ExitCode::SUCCESS;
            }
            let action = match parse_claude_payload(&stdin_json) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("warning: gate could not parse payload: {e}");
                    return ExitCode::SUCCESS;
                }
            };
            let decision = evaluate_action(&action, &rules);
            let (stdout_text, _exit) = format_claude_response(&decision);
            if !stdout_text.is_empty() {
                println!("{stdout_text}");
            }
            ExitCode::SUCCESS
        }
        Command::Test { rule, path } => {
            let rule = match load_rule_file(&rule) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("invalid rule: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let (scanned, violations) = run_rule(&rule, &path);
            println!("{}", render_test(&rule, scanned, &violations));
            ExitCode::SUCCESS
        }
    }
}
