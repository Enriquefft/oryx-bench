//! `oryx-bench show` — render a layer (or all) as an ASCII split-grid.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

use crate::config::Project;
use crate::pull;
use crate::render;

#[derive(Parser, Debug)]
pub struct Args {
    /// Layer name (case-insensitive). Default: render all layers.
    pub layer: Option<String>,

    /// Show position names instead of keycodes.
    #[arg(long)]
    pub names: bool,

    /// Skip the auto-pull check (read from local cache only).
    #[arg(long)]
    pub no_pull: bool,
}

pub fn run(args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    let project = Project::discover(project_override.as_deref())?;

    // Auto-pull unless --no-pull or local mode. Surfaces both Pulled and
    // failure outcomes — silent network errors hide bugs.
    if !args.no_pull && project.is_oryx_mode() {
        match pull::auto_pull(&project) {
            Ok(pull::PullOutcome::Pulled { to, .. }) => {
                eprintln!("(auto-pulled to revision {to})");
            }
            Ok(_) => {}
            Err(e) => eprintln!("warning: auto-pull failed: {e:#}"),
        }
    }

    let layout = project
        .canonical_layout()
        .context("loading canonical layout")?;
    let geometry = project.cfg.layout.geometry.as_str();
    let geom = crate::schema::geometry::get(geometry)
        .with_context(|| format!("unknown geometry '{geometry}'"))?;

    let opts = render::RenderOptions {
        show_position_names: args.names,
    };

    match args.layer {
        Some(want) => {
            let Some(layer) = layout
                .layers
                .iter()
                .find(|l| l.name.eq_ignore_ascii_case(&want))
            else {
                anyhow::bail!("no layer named '{want}' in current layout");
            };
            println!(
                "{}",
                render::ascii::render_layer(geom, layer, &layout.layers, &opts)
            );
        }
        None => {
            for (i, layer) in layout.layers.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                println!("== {} (position {}) ==", layer.name, layer.position);
                println!(
                    "{}",
                    render::ascii::render_layer(geom, layer, &layout.layers, &opts)
                );
            }
        }
    }
    Ok(ExitCode::from(0))
}
