//! `oryx-bench skill install|remove` — manage the Claude Code skill.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::config::Project;
use crate::skill;

#[derive(Parser, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub cmd: Sub,
}

#[derive(Subcommand, Debug)]
pub enum Sub {
    /// Install the skill at `./.claude/skills/oryx-bench/` (or `~/.claude/...` with --global).
    Install {
        /// Install to ~/.claude/skills/ instead of project-local.
        #[arg(long)]
        global: bool,
        /// Overwrite existing files.
        #[arg(long)]
        force: bool,
    },
    /// Remove the skill from the project (or global install with --global).
    Remove {
        #[arg(long)]
        global: bool,
    },
}

pub fn run(args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    match args.cmd {
        Sub::Install { global, force } => {
            let ok = crate::util::term::OK;
            if global {
                let path = skill::install_global(force)?;
                println!("{ok} Installed skill to {}", path.display());
                println!("  note: global install pollutes context budget in unrelated Claude Code sessions.");
            } else {
                let project = Project::discover(project_override.as_deref())?;
                let path = skill::install_project_local(&project.root, force)?;
                println!("{ok} Installed skill to {}", path.display());
            }
        }
        Sub::Remove { global } => {
            let ok = crate::util::term::OK;
            if global {
                skill::remove_global()?;
                println!("{ok} Removed global skill install.");
            } else {
                let project = Project::discover(project_override.as_deref())?;
                skill::remove_project_local(&project.root)?;
                println!("{ok} Removed project-local skill install.");
            }
        }
    }
    Ok(ExitCode::from(0))
}
