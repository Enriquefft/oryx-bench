//! `oryx-bench pull` — manually fetch live state from Oryx.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Result};
use clap::Parser;

use crate::config::Project;
use crate::pull::{self, PullOutcome};

#[derive(Parser, Debug)]
pub struct Args {
    /// Specific revision hash, or "latest" (defaults to kb.toml).
    #[arg(long)]
    pub revision: Option<String>,

    /// Bypass the 60s metadata cache and the `auto_pull = never` setting.
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    let project = Project::discover(project_override.as_deref())?;
    if !project.is_oryx_mode() {
        if project.is_local_mode() {
            println!("local mode: nothing to pull.");
            return Ok(ExitCode::from(0));
        }
        bail!("project kb.toml has no [layout] hash_id and no [layout.local] file");
    }

    let outcome = pull::pull_now(&project, args.revision.as_deref(), args.force)?;
    let ok = crate::util::term::OK;
    match outcome {
        PullOutcome::Pulled { from, to } => {
            println!("{ok} Pulled: {} → {to}", from.unwrap_or_else(|| "?".into()));
        }
        PullOutcome::UpToDate => println!("{ok} Already up to date with Oryx."),
        PullOutcome::CacheHit => println!("{ok} Recent metadata check cached, nothing to do."),
        PullOutcome::Skipped => println!("auto_pull is disabled in kb.toml; nothing to do."),
    }
    Ok(ExitCode::from(0))
}
