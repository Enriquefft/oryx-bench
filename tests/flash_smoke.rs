//! End-to-end tests for `oryx-bench flash`. No real hardware involved —
//! we drop a synthetic firmware.bin into the project's build cache and
//! exercise the user-facing surface (`--dry-run`, `--backend`, the
//! "build first" error path).

use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

fn oryx_bench() -> Command {
    Command::cargo_bin("oryx-bench").expect("binary built")
}

/// Initialize a local-mode project and stage a fake firmware.bin in
/// `.oryx-bench/build/firmware.bin`. The flash freshness check is
/// bypassed in tests by always passing `--force`, since we're not
/// going through a real `oryx-bench build` to populate `build.sha`.
fn init_with_firmware(td: &TempDir) {
    oryx_bench()
        .args(["init", "--blank", "--geometry", "voyager", "--no-skill"])
        .current_dir(td.path())
        .assert()
        .success();
    let build_dir = td.path().join(".oryx-bench/build");
    std::fs::create_dir_all(&build_dir).unwrap();
    // 1024 bytes of synthetic firmware so the dry-run output has a real size.
    let bytes = vec![0xAAu8; 1024];
    std::fs::write(build_dir.join("firmware.bin"), &bytes).unwrap();
}

#[test]
fn flash_dry_run_prints_plan_and_exits_zero() {
    let td = TempDir::new().unwrap();
    init_with_firmware(&td);
    oryx_bench()
        .args(["flash", "--dry-run", "--force"])
        .current_dir(td.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Would flash"))
        .stdout(predicate::str::contains("size:      1024 bytes"))
        .stdout(predicate::str::contains("ZSA Voyager"))
        .stdout(predicate::str::contains("sha256:"));
}

#[test]
fn flash_without_built_firmware_bails() {
    let td = TempDir::new().unwrap();
    oryx_bench()
        .args(["init", "--blank", "--geometry", "voyager", "--no-skill"])
        .current_dir(td.path())
        .assert()
        .success();
    // Without --force, the freshness check should fail first because
    // there's no build cache. Either way, the user must run `oryx-bench
    // build`.
    oryx_bench()
        .args(["flash", "--dry-run", "--force"])
        .current_dir(td.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("oryx-bench build"));
}

#[test]
fn flash_freshness_check_refuses_without_build_cache() {
    let td = TempDir::new().unwrap();
    oryx_bench()
        .args(["init", "--blank", "--geometry", "voyager", "--no-skill"])
        .current_dir(td.path())
        .assert()
        .success();
    // Stage a firmware.bin but no build.sha — the freshness check
    // should refuse with a message pointing at `oryx-bench build`.
    let build_dir = td.path().join(".oryx-bench/build");
    std::fs::create_dir_all(&build_dir).unwrap();
    std::fs::write(build_dir.join("firmware.bin"), [0u8; 16]).unwrap();
    oryx_bench()
        .args(["flash", "--dry-run"])
        .current_dir(td.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no build cache"));
}

#[test]
fn flash_backend_keymapp_dry_run_shows_keymapp_label() {
    let td = TempDir::new().unwrap();
    init_with_firmware(&td);
    oryx_bench()
        .args(["flash", "--dry-run", "--backend", "keymapp", "--force"])
        .current_dir(td.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Keymapp"));
}

#[test]
fn flash_backend_clap_rejects_unknown_variant() {
    // clap rejects unknown ValueEnum variants at argument-parse time
    // with a message that lists the legal values. We assert on the
    // bad value name and one of the legal alternatives so the test
    // doesn't pin clap's exact wording. (The runtime
    // "wally-cli not on PATH" path is covered by unit tests in
    // src/flash/mod.rs::tests via the Environment trait.)
    let td = TempDir::new().unwrap();
    init_with_firmware(&td);
    oryx_bench()
        .args([
            "flash",
            "--dry-run",
            "--backend",
            "not-a-backend",
            "--force",
        ])
        .current_dir(td.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not-a-backend"))
        .stderr(predicate::str::contains("keymapp"));
}

#[test]
fn flash_without_yes_and_with_no_stdin_bails_safely() {
    // Without --yes and with stdin closed, the prompt reads EOF and
    // treats it as "no" — we never call into the backend.
    let td = TempDir::new().unwrap();
    init_with_firmware(&td);
    let assert = oryx_bench()
        .args(["flash", "--backend", "keymapp", "--force"])
        .current_dir(td.path())
        .stdin(std::process::Stdio::null())
        .assert()
        .success();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();
    assert!(
        stdout.contains("Aborted"),
        "expected 'Aborted' in stdout (EOF should be treated as 'no'), got: {stdout}"
    );
}
