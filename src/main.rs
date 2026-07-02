//! The warden CLI: subcommands `validate`, `check`, `gate`.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use warden::adapters::claude_code::{format_claude_response, parse_claude_payload};
use warden::ci_gate::{CoverageReport, coverage, run_check, run_rule};
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
        /// dry-run every rule against this path and report coverage (files scanned, hits)
        #[arg(long)]
        against: Option<String>,
        /// with --against, exit 1 if any rule matches 0 files (catch a dead rule in CI)
        #[arg(long)]
        strict: bool,
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

/// Human-facing `warden validate --against` output: per-rule coverage, with a
/// `⚠` on any rule whose `paths` matched no files (the one actionable "dead
/// rule" signal — `0 hits` on real files is healthy, so it stays neutral).
fn render_coverage(report: &CoverageReport, target: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    if report.total_files == 0 {
        let _ = write!(
            out,
            "no files found under `{target}` — nothing to check coverage against."
        );
        return out;
    }
    let _ = writeln!(out, "coverage vs `{target}` (dry-run, no enforcement):");
    let id_w = report
        .rules
        .iter()
        .map(|c| c.rule_id.len())
        .max()
        .unwrap_or(0);
    let mt_w = report
        .rules
        .iter()
        .map(|c| c.match_type.len())
        .max()
        .unwrap_or(0);
    for c in &report.rules {
        if c.scanned == 0 {
            let _ = writeln!(
                out,
                "  ⚠ {:id_w$}  {:mt_w$}   0 files · paths matched nothing",
                c.rule_id, c.match_type
            );
        } else {
            let _ = writeln!(
                out,
                "  ✓ {:id_w$}  {:mt_w$}   {} files · {} hits",
                c.rule_id, c.match_type, c.scanned, c.hits
            );
        }
    }
    let dead = report.rules.iter().filter(|c| c.scanned == 0).count();
    if dead > 0 {
        let _ = write!(
            out,
            "\n⚠ {dead} rule(s) scoped to 0 files — the `paths` glob matches nothing here\n  \
             (the `src/**` vs `**/src/**` footgun?). Inspect any rule with:\n  \
             warden test <rule.yaml> {target}"
        );
    }
    out.truncate(out.trim_end().len());
    out
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
        Command::Validate {
            rules,
            against,
            strict,
        } => {
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
                    // --against: dry-run the rules over a path and report coverage.
                    if let Some(target) = against {
                        let report = coverage(&rules, &target);
                        println!("\n{}", render_coverage(&report, &target));
                        // A rule scoped to 0 files can never fire — with --strict
                        // that's a failure (catch a dead rule in CI); otherwise
                        // it's advisory. An empty target (total_files == 0) is a
                        // bad path, not a dead rule — render_coverage already says
                        // so, so it must NOT trip --strict (exit 0, like `check`).
                        if strict
                            && report.total_files > 0
                            && report.rules.iter().any(|c| c.scanned == 0)
                        {
                            return ExitCode::FAILURE;
                        }
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
