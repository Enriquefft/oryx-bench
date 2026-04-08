//! Skill file bodies embedded via `include_str!`.
//!
//! The canonical source of truth lives at `skills/oryx-bench/` at the repo
//! root. The binary bundles them so the installer can write them to
//! `.claude/skills/oryx-bench/` without an external registry.
//!
//! The `tests/skill_drift.rs` test asserts that these constants match the
//! on-disk files byte-for-byte.

pub const SKILL_MD: &str = include_str!("../../skills/oryx-bench/SKILL.md");
pub const WORKFLOWS_MD: &str = include_str!("../../skills/oryx-bench/reference/workflows.md");
pub const OVERLAY_COOKBOOK_MD: &str =
    include_str!("../../skills/oryx-bench/reference/overlay-cookbook.md");
pub const LINT_RULES_MD: &str = include_str!("../../skills/oryx-bench/reference/lint-rules.md");
pub const COMMAND_REFERENCE_MD: &str =
    include_str!("../../skills/oryx-bench/reference/command-reference.md");

/// Compile-time assertion that none of the embedded skill files are
/// empty. If any of them are, the binary fails to build — surfacing the
/// problem at compile time instead of producing a useless install at runtime.
const _: () = {
    assert!(!SKILL_MD.is_empty(), "SKILL.md must not be empty");
    assert!(!WORKFLOWS_MD.is_empty(), "workflows.md must not be empty");
    assert!(
        !OVERLAY_COOKBOOK_MD.is_empty(),
        "overlay-cookbook.md must not be empty"
    );
    assert!(!LINT_RULES_MD.is_empty(), "lint-rules.md must not be empty");
    assert!(
        !COMMAND_REFERENCE_MD.is_empty(),
        "command-reference.md must not be empty"
    );
};
