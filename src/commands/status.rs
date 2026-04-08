//! `oryx-bench status` — one-screen overview.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use crate::config::Project;
use crate::lint::{self, Severity};
use crate::pull;
use crate::util::toolchain;

#[derive(Parser, Debug)]
pub struct Args {
    /// Skip the metadata query (useful offline).
    #[arg(long)]
    pub no_pull: bool,
}

pub fn run(args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    let project = Project::discover(project_override.as_deref())?;

    let mode = if project.is_oryx_mode() {
        format!(
            "Oryx (hash {}, geometry {})",
            project.cfg.layout.hash_id.as_deref().unwrap_or("?"),
            project.cfg.layout.geometry
        )
    } else {
        format!("local ({})", project.cfg.layout.geometry)
    };

    println!(
        "Project:  {}",
        project
            .root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
    );
    println!("Mode:     {mode}");
    println!();

    println!("Sources:");
    if project.is_oryx_mode() {
        let p = project.pulled_revision_path();
        if let Ok(meta) = std::fs::metadata(&p) {
            if let Ok(m) = meta.modified() {
                if let Ok(d) = m.elapsed() {
                    println!("  pulled/revision.json   {}", humantime::format_duration(d));
                }
            }
        } else {
            println!("  pulled/revision.json   (missing — run `oryx-bench pull`)");
        }
    } else if let Some(p) = project.local_layout_path() {
        if p.is_file() {
            println!("  layout.toml            present");
        }
    }
    if project.overlay_features_path().is_file() {
        println!("  overlay/features.toml  present");
    }
    println!();

    println!("Sync:");
    if args.no_pull || !project.is_oryx_mode() {
        println!("  (skipped)");
    } else {
        match pull::check_metadata_only(&project) {
            Ok(pull::MetadataStatus::UpToDate) => println!("  ✓ Up to date with Oryx"),
            Ok(pull::MetadataStatus::Stale { remote_hash }) => {
                println!(
                    "  ⚠ Oryx has revision {remote_hash}; local is older. Run `oryx-bench pull`."
                );
            }
            Ok(pull::MetadataStatus::Cached) => println!("  ✓ Metadata cached"),
            Err(e) => println!("  (metadata check failed: {e})"),
        }
    }
    println!();

    println!("Lint:");
    match super::show::load_layout_for_explain(&project) {
        Ok(layout) => match lint::run_all(&layout, &project) {
            Ok(issues) => {
                let errors = issues
                    .iter()
                    .filter(|i| matches!(i.severity, Severity::Error))
                    .count();
                let warnings = issues
                    .iter()
                    .filter(|i| matches!(i.severity, Severity::Warning))
                    .count();
                let info = issues
                    .iter()
                    .filter(|i| matches!(i.severity, Severity::Info))
                    .count();
                println!("  {errors} error(s), {warnings} warning(s), {info} info");
            }
            Err(e) => println!("  (lint failed: {e:#})"),
        },
        Err(e) => println!("  (could not load layout: {e:#})"),
    }
    println!();

    println!("Toolchain:");
    let tc = toolchain::detect();
    for (name, found) in tc.summary() {
        println!("  {name:<12} {}", if found { "✓" } else { "—" });
    }

    Ok(ExitCode::from(0))
}
