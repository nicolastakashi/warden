//! CLI-level tests: exercise the built `warden` binary end-to-end for the
//! `validate --against` coverage report (R2). Uses `CARGO_BIN_EXE_warden`
//! (set by Cargo for integration tests) — no extra dependency.

use std::path::PathBuf;
use std::process::Command;

fn warden() -> Command {
    Command::new(env!("CARGO_BIN_EXE_warden"))
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn validate_against_reports_coverage_and_is_advisory() {
    // demo rules DO apply to the examples tree — coverage prints, exit 0.
    let out = warden()
        .arg("validate")
        .arg("--rules")
        .arg(root().join("demo").join("rules"))
        .arg("--against")
        .arg(root().join("examples"))
        .output()
        .expect("run warden");
    assert!(out.status.success(), "advisory: exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("coverage vs"), "prints a coverage block:\n{stdout}");
    assert!(stdout.contains("hits"), "prints per-rule hits:\n{stdout}");
}

#[test]
fn validate_against_strict_exits_1_on_a_dead_rule() {
    // A rule scoped to a path that doesn't exist under the target is "dead":
    // its `paths` glob matches 0 files, so it can never fire.
    let dir = std::env::temp_dir().join("warden_r2_strict_rules");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("dead.yaml"),
        "id: dead\ndescription: d\nwhy: w\nscope: [ci]\nenforcement: block\n\
         paths: [\"nowhere/**\"]\nmatch:\n  type: pattern\n  patterns: ['x']\n",
    )
    .unwrap();
    let target = root().join("examples");

    let strict = warden()
        .arg("validate")
        .arg("--rules")
        .arg(&dir)
        .arg("--against")
        .arg(&target)
        .arg("--strict")
        .output()
        .expect("run warden");
    assert!(
        !strict.status.success(),
        "--strict fails when a rule is scoped to 0 files"
    );

    let advisory = warden()
        .arg("validate")
        .arg("--rules")
        .arg(&dir)
        .arg("--against")
        .arg(&target)
        .output()
        .expect("run warden");
    assert!(
        advisory.status.success(),
        "without --strict, a dead rule is advisory (exit 0)"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn validate_without_against_is_unchanged() {
    // No --against: plain shape validation, exit 0, no coverage block.
    let out = warden()
        .arg("validate")
        .arg("--rules")
        .arg(root().join("rules"))
        .output()
        .expect("run warden");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("rule(s) valid"));
    assert!(!stdout.contains("coverage vs"), "no coverage block without --against");
}
