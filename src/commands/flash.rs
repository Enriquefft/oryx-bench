//! `oryx-bench flash` — flash the built firmware to the keyboard.
//!
//! Pipeline:
//! 1. Discover the project; locate `.oryx-bench/build/firmware.bin`.
//! 2. Verify the build is up-to-date (canonical layout + features +
//!    overlay sha matches `.oryx-bench/build/build.sha`). Refuse if
//!    stale unless `--force` is set.
//! 3. Build a [`flash::FlashPlan`] (size, sha256, target, backend).
//! 4. If `--dry-run`, print the plan and exit 0.
//! 5. Otherwise print the plan and ask the user `[y/N]` (unless `--yes`).
//! 6. On confirmation, dispatch to the backend.
//!
//! `flash` **never auto-pulls** — flashing is the moment of commitment;
//! you flash exactly what you just looked at, no surprises (per spec).

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::Parser;

use crate::build;
use crate::config::Project;
use crate::flash;
use crate::generate;
use crate::schema::features::FeaturesToml;
use crate::schema::geometry;

#[derive(Parser, Debug)]
pub struct Args {
    /// Print what would be flashed; don't flash.
    #[arg(long)]
    pub dry_run: bool,
    /// Skip the CLI's confirmation prompt. Still requires explicit
    /// conversational approval when used by an agent — this flag only
    /// bypasses the in-process `[y/N]` prompt.
    #[arg(long)]
    pub yes: bool,
    /// Backend selection. `auto` (default) prefers wally-cli if installed
    /// and falls back to the Keymapp GUI handoff.
    #[arg(long, value_enum, default_value_t = flash::BackendChoice::Auto)]
    pub backend: flash::BackendChoice,
    /// Flash even if the firmware on disk doesn't match the current
    /// canonical inputs (i.e. someone forgot to rebuild after a pull
    /// or overlay edit). Off by default; use only when you know what
    /// you're doing.
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    let project = Project::discover(project_override.as_deref())?;
    let firmware_path = build::firmware_path(&project);

    // Freshness check: refuse to flash a stale .bin unless --force.
    if !args.force {
        check_firmware_is_fresh(&project)?;
    }

    let geom = project_geometry(&project)?;
    let backend = flash::detect_backend(args.backend)?;
    let plan = flash::plan(&firmware_path, backend, geom)?;

    if args.dry_run {
        println!("Would flash:");
        println!("{}", flash::render_plan(&plan));
        return Ok(ExitCode::from(0));
    }

    println!("About to flash:");
    println!("{}", flash::render_plan(&plan));
    println!();

    if !args.yes && !confirm()? {
        println!("Aborted.");
        return Ok(ExitCode::from(0));
    }

    flash::execute(&plan)?;
    println!("{} Flash complete.", crate::util::term::OK);
    Ok(ExitCode::from(0))
}

/// Compare the build cache's recorded input sha against the current
/// input sha (re-derived from the canonical layout + features.toml +
/// overlay/). Errors with an actionable message if they differ.
fn check_firmware_is_fresh(project: &Project) -> Result<()> {
    let build_sha_path = build::build_sha_path(project);
    let cached = std::fs::read_to_string(&build_sha_path).with_context(|| {
        format!(
            "no build cache at {} — run `oryx-bench build` first",
            build_sha_path.display()
        )
    })?;

    let layout = super::show::load_layout_for_explain(project)?;
    let features = FeaturesToml::load_or_default(&project.overlay_features_path())?;
    // Both call sites of "look up the geometry from the project"
    // route through this single helper. Previously this site
    // duplicated the registry-lookup + with_context boilerplate
    // from `run`, leaving two sources of truth for the same error
    // message format.
    let geom = project_geometry(project)?;
    let overlay_dir = project.overlay_dir();
    let overlay_arg = if overlay_dir.exists() {
        Some(overlay_dir.as_path())
    } else {
        None
    };
    let generated = generate::generate_all(&layout, &features, geom, overlay_arg)?;
    let current = build::input_sha(&generated, overlay_arg)?;

    if cached.trim() != current.trim() {
        bail!(
            "firmware on disk is stale: build cache sha {} differs from current inputs sha {}. \
             Run `oryx-bench build` to refresh, or pass --force to flash anyway.",
            short(cached.trim()),
            short(&current),
        );
    }
    Ok(())
}

/// Resolve the project's configured geometry to a trait object,
/// surfacing a user-actionable error (with the list of supported
/// boards) on miss. Single helper so the same lookup boilerplate
/// doesn't drift between `run` and `check_firmware_is_fresh`.
fn project_geometry(project: &Project) -> Result<&'static dyn crate::schema::geometry::Geometry> {
    geometry::get(project.cfg.layout.geometry.as_str()).with_context(|| {
        format!(
            "unknown geometry '{}' in kb.toml — supported: {}",
            project.cfg.layout.geometry,
            geometry::supported_slugs()
        )
    })
}

fn short(s: &str) -> String {
    s.chars().take(8).collect()
}

/// Read `[y/N]` from stdin. Returns false on EOF or any non-`y/yes` answer.
/// Case-insensitive — `Yes`, `Y`, `yes`, `YES` all count.
fn confirm() -> Result<bool> {
    print!("Continue? [y/N] ");
    io::stdout()
        .flush()
        .context("flushing stdout before confirmation prompt")?;
    let mut line = String::new();
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let n = handle.read_line(&mut line)?;
    if n == 0 {
        return Ok(false);
    }
    let trimmed = line.trim();
    Ok(trimmed.eq_ignore_ascii_case("y") || trimmed.eq_ignore_ascii_case("yes"))
}
