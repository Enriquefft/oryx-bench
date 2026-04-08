//! Embedded Claude Code skill installer.
//!
//! The skill files live canonically in `skills/oryx-bench/` at the repo
//! root. `include_str!` bundles them into the binary so the installer
//! doesn't need any external registry.
//!
//! Install location:
//! - **Project-local default**: `<project>/.claude/skills/oryx-bench/`
//! - **Global (discouraged)**: `~/.claude/skills/oryx-bench/` via `--global`

pub mod embedded;

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::util::fs as fsx;

/// Install the skill at `<project>/.claude/skills/oryx-bench/`.
pub fn install_project_local(project_root: &Path, force: bool) -> Result<PathBuf> {
    let target = project_root.join(".claude/skills/oryx-bench");
    install_at(&target, force)
}

/// Install the skill globally at `~/.claude/skills/oryx-bench/`.
pub fn install_global(force: bool) -> Result<PathBuf> {
    let target = global_skill_dir()?;
    install_at(&target, force)
}

/// Remove the project-local install.
pub fn remove_project_local(project_root: &Path) -> Result<()> {
    let target = project_root.join(".claude/skills/oryx-bench");
    remove_at(&target)
}

/// Remove the global install.
pub fn remove_global() -> Result<()> {
    let target = global_skill_dir()?;
    remove_at(&target)
}

/// Cross-platform path to `<home>/.claude/skills/oryx-bench/`.
///
/// Uses the `directories` crate so the code Just Works on Linux
/// (`$HOME`), macOS (`$HOME`), and Windows (`%USERPROFILE%`). The
/// prior implementation hardcoded `HOME` which broke on Windows and
/// in CI environments that only set `USERPROFILE`.
fn global_skill_dir() -> Result<PathBuf> {
    let base = directories::BaseDirs::new()
        .context("could not determine user home directory (HOME/USERPROFILE unset?)")?;
    Ok(base.home_dir().join(".claude/skills/oryx-bench"))
}

fn install_at(target: &Path, force: bool) -> Result<PathBuf> {
    fsx::ensure_dir(target)?;
    let reference = target.join("reference");
    fsx::ensure_dir(&reference)?;

    let files: &[(&str, &str)] = &[
        ("SKILL.md", embedded::SKILL_MD),
        ("reference/workflows.md", embedded::WORKFLOWS_MD),
        (
            "reference/overlay-cookbook.md",
            embedded::OVERLAY_COOKBOOK_MD,
        ),
        ("reference/lint-rules.md", embedded::LINT_RULES_MD),
        (
            "reference/command-reference.md",
            embedded::COMMAND_REFERENCE_MD,
        ),
    ];
    for (rel, content) in files {
        let path = target.join(rel);
        if path.exists() && !force {
            bail!("refusing to overwrite {} (use --force)", path.display());
        }
        fsx::atomic_write(&path, content.as_bytes())?;
    }
    Ok(target.to_path_buf())
}

fn remove_at(target: &Path) -> Result<()> {
    if target.exists() {
        std::fs::remove_dir_all(target)
            .with_context(|| format!("removing {}", target.display()))?;
    }
    Ok(())
}

/// Generate the command-reference markdown from the live clap definitions.
pub fn gen_command_reference_markdown() -> String {
    let cmd = crate::cli::command();
    let mut out = String::new();
    out.push_str(
        "# Command reference\n\n\
         > **This file is GENERATED at build time** by the `xtask` binary from the\n\
         > clap CLI definitions in `src/cli.rs`. Do not edit by hand — run\n\
         > `cargo xtask gen-skill-docs` to regenerate. CI verifies the file is\n\
         > up-to-date.\n\n---\n\n",
    );
    // Top-level help.
    out.push_str("## `oryx-bench`\n\n```\n");
    let top_help = render_long_help(&cmd);
    out.push_str(&top_help);
    out.push_str("\n```\n\n---\n\n");
    // Per-subcommand.
    let mut sub_names: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .collect();
    sub_names.sort();
    for name in sub_names {
        let cmd = crate::cli::command();
        let Some(sub) = cmd.find_subcommand(&name).cloned() else {
            continue;
        };
        out.push_str(&format!("## `oryx-bench {}`\n\n", sub.get_name()));
        if let Some(about) = sub.get_about() {
            out.push_str(&format!("{about}\n\n"));
        }
        out.push_str("```\n");
        out.push_str(&render_long_help(&sub));
        out.push_str("\n```\n\n---\n\n");
    }
    out
}

fn render_long_help(cmd: &clap::Command) -> String {
    // Clone so we can mutate; clap::Command::render_long_help is idempotent anyway.
    let mut cmd = cmd.clone();
    cmd.render_long_help().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn install_project_local_writes_all_files() {
        let td = TempDir::new().unwrap();
        let path = install_project_local(td.path(), false).unwrap();
        assert!(path.join("SKILL.md").is_file());
        assert!(path.join("reference/workflows.md").is_file());
        assert!(path.join("reference/overlay-cookbook.md").is_file());
        assert!(path.join("reference/lint-rules.md").is_file());
        assert!(path.join("reference/command-reference.md").is_file());
    }

    #[test]
    fn install_refuses_overwrite_without_force() {
        let td = TempDir::new().unwrap();
        install_project_local(td.path(), false).unwrap();
        let err = install_project_local(td.path(), false).unwrap_err();
        assert!(err.to_string().contains("refusing to overwrite"));
    }

    #[test]
    fn install_force_overwrites() {
        let td = TempDir::new().unwrap();
        install_project_local(td.path(), false).unwrap();
        install_project_local(td.path(), true).unwrap();
    }

    #[test]
    fn remove_clears_project_install() {
        let td = TempDir::new().unwrap();
        install_project_local(td.path(), false).unwrap();
        remove_project_local(td.path()).unwrap();
        assert!(!td.path().join(".claude/skills/oryx-bench").exists());
    }
}
