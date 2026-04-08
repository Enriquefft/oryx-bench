//! End-to-end smoke test covering init → show → lint → skill install.

use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

fn oryx_bench() -> Command {
    Command::cargo_bin("oryx-bench").expect("binary built")
}

#[test]
fn init_blank_then_show_and_lint() {
    let td = TempDir::new().unwrap();
    oryx_bench()
        .args(["init", "--blank", "--geometry", "voyager", "--no-skill"])
        .current_dir(td.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Created local-mode project"));

    // kb.toml + layout.toml exist
    assert!(td.path().join("kb.toml").is_file());
    assert!(td.path().join("layout.toml").is_file());

    // show renders the main layer without errors
    oryx_bench()
        .arg("show")
        .current_dir(td.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Main"));

    // lint runs cleanly on the scaffold
    oryx_bench()
        .arg("lint")
        .current_dir(td.path())
        .assert()
        .success();
}

#[test]
fn init_refuses_overwrite_without_force() {
    let td = TempDir::new().unwrap();
    oryx_bench()
        .args(["init", "--blank", "--geometry", "voyager", "--no-skill"])
        .current_dir(td.path())
        .assert()
        .success();
    oryx_bench()
        .args(["init", "--blank", "--geometry", "voyager", "--no-skill"])
        .current_dir(td.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing to overwrite"));
}

#[test]
fn init_with_force_overwrites() {
    let td = TempDir::new().unwrap();
    oryx_bench()
        .args(["init", "--blank", "--geometry", "voyager", "--no-skill"])
        .current_dir(td.path())
        .assert()
        .success();
    oryx_bench()
        .args([
            "init",
            "--blank",
            "--geometry",
            "voyager",
            "--no-skill",
            "--force",
        ])
        .current_dir(td.path())
        .assert()
        .success();
}

#[test]
fn init_oryx_mode_creates_pulled_dir() {
    let td = TempDir::new().unwrap();
    oryx_bench()
        .args(["init", "--hash", "yrbLx", "--no-skill"])
        .current_dir(td.path())
        .assert()
        .success();
    assert!(td.path().join("kb.toml").is_file());
    assert!(td.path().join("pulled").is_dir());
    assert!(!td.path().join("layout.toml").exists());
}

#[test]
fn skill_install_writes_files() {
    let td = TempDir::new().unwrap();
    oryx_bench()
        .args(["init", "--blank", "--geometry", "voyager", "--no-skill"])
        .current_dir(td.path())
        .assert()
        .success();
    oryx_bench()
        .args(["skill", "install"])
        .current_dir(td.path())
        .assert()
        .success();
    assert!(td
        .path()
        .join(".claude/skills/oryx-bench/SKILL.md")
        .is_file());
    assert!(td
        .path()
        .join(".claude/skills/oryx-bench/reference/lint-rules.md")
        .is_file());
}

#[test]
fn setup_runs_without_project() {
    oryx_bench()
        .arg("setup")
        .assert()
        .success()
        .stdout(predicate::str::contains("Toolchain detection"));
}

/// Make a local-mode project with a layout that triggers `lt-on-high-freq`.
fn init_project_with_lt_on_bspc(td: &tempfile::TempDir) {
    oryx_bench()
        .args(["init", "--blank", "--geometry", "voyager", "--no-skill"])
        .current_dir(td.path())
        .assert()
        .success();
    // Overwrite layout.toml with a layout that has LT on right thumb BSPC.
    std::fs::write(
        td.path().join("layout.toml"),
        r#"
[meta]
title = "test"
geometry = "voyager"

[[layers]]
name = "Main"
position = 0
[layers.keys]
R_thumb_outer = { tap = "LT(SymNum, BSPC)" }

[[layers]]
name = "SymNum"
position = 1
[layers.keys]
"#,
    )
    .unwrap();
}

#[test]
fn lint_text_format_reports_lt_on_high_freq() {
    let td = TempDir::new().unwrap();
    init_project_with_lt_on_bspc(&td);
    oryx_bench()
        .arg("lint")
        .current_dir(td.path())
        .assert()
        .failure() // exit code 1 — has errors
        .stdout(predicate::str::contains("lt-on-high-freq"));
}

#[test]
fn lint_json_format_emits_parseable_json() {
    let td = TempDir::new().unwrap();
    init_project_with_lt_on_bspc(&td);
    let assert = oryx_bench()
        .args(["lint", "--format", "json"])
        .current_dir(td.path())
        .assert()
        .failure();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(stdout).expect("valid json");
    assert!(parsed
        .iter()
        .any(|i| i.get("rule_id").and_then(|s| s.as_str()) == Some("lt-on-high-freq")));
}

#[test]
fn lint_strict_returns_exit_2_on_warning_only() {
    let td = TempDir::new().unwrap();
    oryx_bench()
        .args(["init", "--blank", "--geometry", "voyager", "--no-skill"])
        .current_dir(td.path())
        .assert()
        .success();
    // Layout with an orphaned mod-tap (warning, not error).
    std::fs::write(
        td.path().join("layout.toml"),
        r#"
[meta]
title = "test"
geometry = "voyager"

[[layers]]
name = "Main"
position = 0
[layers.keys]
L_pinky_home = { hold = "LSFT" }
"#,
    )
    .unwrap();

    // Default lint exits 0 (warnings don't fail).
    oryx_bench()
        .arg("lint")
        .current_dir(td.path())
        .assert()
        .success();

    // --strict turns warnings into a non-zero exit (we use exit code 2).
    oryx_bench()
        .args(["lint", "--strict"])
        .current_dir(td.path())
        .assert()
        .code(2);
}

#[test]
fn lint_rule_filter_runs_only_matching_rule() {
    let td = TempDir::new().unwrap();
    init_project_with_lt_on_bspc(&td);
    let assert = oryx_bench()
        .args(["lint", "--rule", "lt-on-high-freq"])
        .current_dir(td.path())
        .assert()
        .failure();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();
    // Output mentions the targeted rule and not the unreachable-layer one.
    assert!(stdout.contains("lt-on-high-freq"));
    assert!(!stdout.contains("unreachable-layer"));
}

#[test]
fn help_subcommand_lists_commands() {
    oryx_bench()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("lint"))
        .stdout(predicate::str::contains("skill"));
}
