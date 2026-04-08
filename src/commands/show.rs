//! `oryx-bench show` — render a layer (or all) as an ASCII split-grid.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

use crate::config::Project;
use crate::pull;
use crate::render;
use crate::schema::canonical::CanonicalLayout;

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

    let layout = load_layout(&project).context("loading canonical layout")?;
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
            println!("{}", render::ascii::render_layer(geom, layer, &opts));
        }
        None => {
            for (i, layer) in layout.layers.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                println!("== {} (position {}) ==", layer.name, layer.position);
                println!("{}", render::ascii::render_layer(geom, layer, &opts));
            }
        }
    }
    Ok(ExitCode::from(0))
}

pub(crate) fn load_layout_for_explain(project: &Project) -> Result<CanonicalLayout> {
    load_layout(project)
}

fn load_layout(project: &Project) -> Result<CanonicalLayout> {
    if project.is_oryx_mode() {
        let path = project.pulled_revision_path();
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}: run `oryx-bench pull` first", path.display()))?;
        let oryx: crate::schema::oryx::Layout =
            serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        CanonicalLayout::from_oryx(&oryx)
    } else if let Some(path) = project.local_layout_path() {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let local: crate::schema::layout::LayoutFile =
            toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        CanonicalLayout::from_local(&local)
    } else {
        anyhow::bail!("kb.toml has neither [layout] hash_id nor [layout.local] file");
    }
}
