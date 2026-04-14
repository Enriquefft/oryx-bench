//! `oryx-bench explain POSITION` — cross-layer view of a single position.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

use crate::config::Project;
use crate::lint;
use crate::schema::geometry;

#[derive(Parser, Debug)]
pub struct Args {
    /// Position name (e.g. R_thumb_outer, L_pinky_home).
    pub position: String,
}

pub fn run(args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    let project = Project::discover(project_override.as_deref())?;
    let layout = project.canonical_layout()?;

    let geom = geometry::get(project.cfg.layout.geometry.as_str())
        .with_context(|| format!("unknown geometry '{}'", project.cfg.layout.geometry))?;
    let Some(idx) = geom.position_to_index(&args.position) else {
        anyhow::bail!(
            "no position named '{}' in geometry '{}'",
            args.position,
            geom.id()
        );
    };

    println!("Position: {} (matrix index {idx})", args.position);
    println!();

    // Run lint once so we can annotate each layer with any issues on this position.
    let issues = lint::run_all(&layout, &project)?;

    for layer in &layout.layers {
        let binding = layer
            .keys
            .get(idx)
            .map(|k| k.display())
            .unwrap_or_else(|| "—".to_string());
        let tags: Vec<String> = issues
            .iter()
            .filter(|i| i.layer.as_deref() == Some(layer.name.as_str()))
            .filter(|i| i.position_index == Some(idx))
            .map(|i| format!("{} {}", crate::util::term::WARN, i.rule_id))
            .collect();
        let tags_str = if tags.is_empty() {
            String::new()
        } else {
            format!("    {}", tags.join(" "))
        };
        println!("  {:<10} {binding}{tags_str}", format!("{}:", layer.name));
    }
    Ok(ExitCode::from(0))
}
