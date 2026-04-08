//! clap CLI definitions + dispatch.
//!
//! Defines the full subcommand surface and the global flags. Each
//! subcommand's `Args` struct lives in `commands/<name>.rs` next to
//! its `run()` function — this module is purely the glue.

use std::process::ExitCode;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};

use crate::commands;

#[derive(Parser, Debug)]
#[command(
    name = "oryx-bench",
    version,
    about = "Workbench for ZSA keyboard layouts — Oryx-friendly, not Oryx-required",
    long_about = None,
)]
pub struct Cli {
    /// Path to the project root (default: discover from cwd).
    #[arg(long, global = true)]
    pub project: Option<std::path::PathBuf>,

    /// Color mode.
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,

    /// Increase logging verbosity (repeatable).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Detect toolchain (qmk, gcc-arm, zig, docker, wally-cli, keymapp). Idempotent.
    Setup(commands::setup::Args),
    /// Create a project skeleton.
    Init(commands::init::Args),
    /// Switch a local-mode project to Oryx mode.
    Attach(commands::attach::Args),
    /// Switch an Oryx-mode project to local mode. One-way.
    Detach(commands::detach::Args),
    /// Manually fetch live state from Oryx. Usually unnecessary thanks to auto-pull.
    Pull(commands::pull::Args),
    /// Render a layer (or all) as an ASCII split-grid keyboard.
    Show(commands::show::Args),
    /// Cross-layer view of a single position.
    Explain(commands::explain::Args),
    /// Search across all layers.
    Find(commands::find::Args),
    /// Run static analysis.
    Lint(commands::lint::Args),
    /// One-screen overview of project, sync, and lint state.
    Status(commands::status::Args),
    /// Compile firmware via the bundled Docker image.
    Build(commands::build::Args),
    /// Flash firmware to a connected keyboard.
    Flash(commands::flash::Args),
    /// Install / remove the project-local Claude Code skill.
    Skill(commands::skill::Args),
    /// Semantic diff vs git ref.
    Diff(commands::diff::Args),
    /// Re-run lint with the current keycode catalog. Use after `cargo install --force oryx-bench`.
    UpgradeCheck(commands::upgrade_check::Args),
}

/// Entry point invoked from `main.rs`.
pub fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    dispatch(cli)
}

fn dispatch(cli: Cli) -> Result<ExitCode> {
    let project_override = cli.project.clone();
    match cli.cmd {
        Command::Setup(args) => commands::setup::run(args),
        Command::Init(args) => commands::init::run(args),
        Command::Attach(args) => commands::attach::run(args, project_override),
        Command::Detach(args) => commands::detach::run(args, project_override),
        Command::Pull(args) => commands::pull::run(args, project_override),
        Command::Show(args) => commands::show::run(args, project_override),
        Command::Explain(args) => commands::explain::run(args, project_override),
        Command::Find(args) => commands::find::run(args, project_override),
        Command::Lint(args) => commands::lint::run(args, project_override),
        Command::Status(args) => commands::status::run(args, project_override),
        Command::Build(args) => commands::build::run(args, project_override),
        Command::Flash(args) => commands::flash::run(args, project_override),
        Command::Skill(args) => commands::skill::run(args, project_override),
        Command::Diff(args) => commands::diff::run(args, project_override),
        Command::UpgradeCheck(args) => commands::upgrade_check::run(args, project_override),
    }
}

/// Returns the underlying clap `Command` tree so xtask (and tests) can
/// walk it for help rendering.
pub fn command() -> clap::Command {
    Cli::command()
}

/// Generate the skills/oryx-bench/reference/command-reference.md markdown
/// from the clap definitions. Called by `xtask gen-skill-docs`.
pub fn gen_command_reference_markdown() -> String {
    crate::skill::gen_command_reference_markdown()
}
