//! `oryx-bench build` — compile firmware via the configured backend.
//!
//! Pipeline:
//! 1. Auto-pull (unless `--no-pull` or local mode).
//! 2. Load canonical layout + features.toml.
//! 3. Run all generators → `Generated` bundle.
//! 4. Hand off to the build backend.
//! 5. Print the output path / size / sha256 / cache-hit status.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

use crate::build;
use crate::config::Project;
use crate::generate;
use crate::pull;
use crate::schema::features::FeaturesToml;
use crate::schema::geometry;
use crate::util::fs as fsx;

#[derive(Parser, Debug)]
pub struct Args {
    /// Show what would be built; don't actually build.
    #[arg(long)]
    pub dry_run: bool,
    /// Build with LTO, strip.
    #[arg(long)]
    pub release: bool,
    /// Save the generated overlay C source for inspection.
    #[arg(long)]
    pub emit_overlay_c: bool,
    /// Skip auto-pull.
    #[arg(long)]
    pub no_pull: bool,
}

pub fn run(args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    let project = Project::discover(project_override.as_deref())?;

    if !args.no_pull && project.is_oryx_mode() {
        if let Err(e) = pull::auto_pull(&project) {
            eprintln!("warning: auto-pull failed: {e:#}");
        }
    }

    let layout = project.canonical_layout()?;
    let geom = geometry::get(project.cfg.layout.geometry.as_str())
        .with_context(|| format!("unknown geometry '{}'", project.cfg.layout.geometry))?;
    let features = FeaturesToml::load_or_default(&project.overlay_features_path())?;

    let overlay_dir = project.overlay_dir();
    let generated = generate::generate_all(
        &layout,
        &features,
        geom,
        if overlay_dir.exists() {
            Some(overlay_dir.as_path())
        } else {
            None
        },
    )?;

    if args.emit_overlay_c {
        let dump = build::build_dir(&project).join("emitted");
        fsx::ensure_dir(&dump)?;
        fsx::atomic_write(&dump.join("keymap.c"), generated.keymap_c.as_bytes())?;
        fsx::atomic_write(&dump.join("_features.c"), generated.features_c.as_bytes())?;
        fsx::atomic_write(&dump.join("config.h"), generated.config_h.as_bytes())?;
        fsx::atomic_write(&dump.join("rules.mk"), generated.rules_mk.as_bytes())?;
        println!(
            "{} Wrote generated C to {}",
            crate::util::term::OK,
            dump.display()
        );
    }

    let outcome = build::build(&project, &generated, args.dry_run)?;
    if args.dry_run {
        println!(
            "Would build:\n  inputs sha256:  {}\n  cache hit:      {}\n  firmware path:  {}",
            outcome.sha256,
            outcome.from_cache,
            outcome.firmware_bin.display()
        );
    } else {
        println!(
            "{} Built firmware\n  path:    {}\n  size:    {} bytes\n  sha256:  {}\n  cached:  {}",
            crate::util::term::OK,
            outcome.firmware_bin.display(),
            outcome.size_bytes,
            outcome.sha256,
            outcome.from_cache
        );
    }
    Ok(ExitCode::from(0))
}
