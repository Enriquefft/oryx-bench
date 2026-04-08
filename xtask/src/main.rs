//! xtask — codegen of skill reference files from the live registries.
//!
//! Walks `oryx_bench::lint::rules::registry()` and `oryx_bench::cli::command()`
//! at runtime and emits `skills/oryx-bench/reference/lint-rules.md` and
//! `skills/oryx-bench/reference/command-reference.md`.
//!
//! This replaces the `build.rs` trap (build scripts cannot easily compile
//! and execute downstream code — which is what we'd need to walk the lint
//! rule registry and invoke clap to render markdown).

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Regenerate skills/oryx-bench/reference/{lint-rules,command-reference}.md
    GenSkillDocs {
        /// Check that files are up-to-date without modifying them
        #[arg(long)]
        check: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::GenSkillDocs { check } => xtask_gen_skill_docs::run(check),
    }
}

mod xtask_gen_skill_docs {
    use std::fs;
    use std::path::{Path, PathBuf};

    use anyhow::{bail, Context, Result};

    pub fn run(check: bool) -> Result<()> {
        let skills_dir = project_root().join("skills/oryx-bench/reference");
        fs::create_dir_all(&skills_dir)
            .with_context(|| format!("creating {}", skills_dir.display()))?;

        let lint_md = oryx_bench::lint::gen_lint_rules_markdown();
        let cmd_md = oryx_bench::cli::gen_command_reference_markdown();

        if check {
            check_or_regenerate(&skills_dir.join("lint-rules.md"), &lint_md)?;
            check_or_regenerate(&skills_dir.join("command-reference.md"), &cmd_md)?;
            println!("skill docs are up-to-date");
        } else {
            atomic_write(&skills_dir.join("lint-rules.md"), &lint_md)?;
            atomic_write(&skills_dir.join("command-reference.md"), &cmd_md)?;
            println!("regenerated lint-rules.md + command-reference.md");
        }
        Ok(())
    }

    fn check_or_regenerate(path: &Path, expected: &str) -> Result<()> {
        let actual =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        if actual != expected {
            bail!(
                "{} is out of date. Run `cargo xtask gen-skill-docs` to regenerate.",
                path.display()
            );
        }
        Ok(())
    }

    /// Crash-safe write: persist via a sibling tempfile + rename so a
    /// killed `xtask gen-skill-docs` never leaves a partially-written
    /// markdown file in the skills tree.
    fn atomic_write(path: &Path, contents: &str) -> Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(parent)
            .with_context(|| format!("creating tempfile in {}", parent.display()))?;
        std::io::Write::write_all(&mut tmp, contents.as_bytes())
            .with_context(|| format!("writing tempfile for {}", path.display()))?;
        tmp.as_file()
            .sync_all()
            .with_context(|| format!("fsync tempfile for {}", path.display()))?;
        tmp.persist(path)
            .map_err(|e| e.error)
            .with_context(|| format!("renaming temp into place: {}", path.display()))?;
        Ok(())
    }

    fn project_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask has a parent workspace")
            .to_path_buf()
    }
}
