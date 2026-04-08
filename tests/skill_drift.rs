//! Asserts the `include_str!`-bundled skill files match the on-disk
//! `skills/oryx-bench/` tree byte-for-byte. Caught at test time so CI
//! can flag drift before merge.
//!
//! Run `cargo xtask gen-skill-docs` to regenerate any files that drift.

#[test]
fn skill_md_matches_on_disk() {
    let bundled = include_str!("../src/../skills/oryx-bench/SKILL.md");
    let disk = std::fs::read_to_string("skills/oryx-bench/SKILL.md").unwrap();
    assert_eq!(
        bundled, disk,
        "bundled SKILL.md has drifted from on-disk version"
    );
}

#[test]
fn lint_rules_md_matches_generated_output() {
    let generated = oryx_bench::lint::gen_lint_rules_markdown();
    let disk = std::fs::read_to_string("skills/oryx-bench/reference/lint-rules.md").unwrap();
    assert_eq!(
        generated.trim(),
        disk.trim(),
        "lint-rules.md is stale — run `cargo xtask gen-skill-docs`"
    );
}

#[test]
fn command_reference_md_matches_generated_output() {
    let generated = oryx_bench::skill::gen_command_reference_markdown();
    let disk = std::fs::read_to_string("skills/oryx-bench/reference/command-reference.md").unwrap();
    assert_eq!(
        generated.trim(),
        disk.trim(),
        "command-reference.md is stale — run `cargo xtask gen-skill-docs`"
    );
}
