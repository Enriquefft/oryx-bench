//! `oryx-bench setup` — toolchain detection. Idempotent. No state changes.

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use crate::util::toolchain;

#[derive(Parser, Debug)]
pub struct Args {
    /// Print each tool's `--version` output (or the equivalent flag),
    /// not just whether it was found on PATH. Useful when debugging
    /// version-mismatch issues with the docker build backend.
    #[arg(long = "full", short = 'f')]
    pub full: bool,
}

pub fn run(args: Args) -> Result<ExitCode> {
    let report = toolchain::detect();
    println!("{}", report.render(args.full));
    Ok(ExitCode::from(0))
}
