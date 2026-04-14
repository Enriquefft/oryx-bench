//! `oryx-bench watch` — live runtime indicator.
//!
//! Thin clap glue: parse args, load the canonical layout when it's
//! useful, dispatch to a headless path or the GUI. Project discovery
//! is *not* forced on the scriptable paths (`--once`, `--layer-only`)
//! so they work as ad-hoc sanity checks anywhere on the filesystem —
//! but a *broken* project (present but unparseable) still errors so
//! corrupt `kb.toml` is never silently indistinguishable from "no
//! project here".

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

use crate::config::Project;
use crate::schema::canonical::CanonicalLayout;
use crate::watch::{gui::App as WatchApp, headless, ConnectOptions};

#[derive(Parser, Debug)]
pub struct Args {
    /// One poll, print status, exit. Scriptable; never opens a window.
    #[arg(long, conflicts_with_all = ["layer_only", "set_layer", "reset_layers"])]
    pub once: bool,

    /// Stream layer changes to stdout, one line per change. Scriptable;
    /// never opens a window. Runs until Ctrl-C.
    #[arg(long, conflicts_with_all = ["once", "set_layer", "reset_layers"])]
    pub layer_only: bool,

    /// Lock the keyboard to layer N, confirm via a LAYER event, exit.
    /// Scriptable; never opens a window.
    #[arg(
        long,
        value_name = "N",
        conflicts_with_all = ["once", "layer_only", "reset_layers"],
    )]
    pub set_layer: Option<u8>,

    /// Release every host-driven layer lock (one UnsetLayer per layer
    /// known to the project layout; falls back to the full `0..=15`
    /// range when no project is reachable). Exits immediately after
    /// the last write. Useful when a prior session left the keyboard
    /// stuck on a lock.
    #[arg(long, conflicts_with_all = ["once", "layer_only", "set_layer"])]
    pub reset_layers: bool,

    /// Select a specific keyboard by serial number or HID device node
    /// path (e.g. `/dev/hidraw3`). Defaults to the first matching
    /// ZSA raw-HID device.
    #[arg(long, value_name = "SERIAL_OR_PATH")]
    pub device: Option<String>,

    /// Per-operation timeout (handshake reads). Milliseconds.
    #[arg(long, default_value_t = 2000)]
    pub timeout_ms: u64,
}

pub fn run(args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    let opts = ConnectOptions {
        device_override: args.device.clone(),
        timeout: Duration::from_millis(args.timeout_ms),
    };

    // Headless paths render richer output when a project layout is
    // available (maps layer indices to names), but don't require one —
    // running `oryx-bench watch --once` from any shell must work.
    // However, a project that *exists but fails to parse* is a bug,
    // not a fallback condition: surface it.
    if args.once {
        let layout = load_project_layout_tolerantly(project_override.as_deref())?;
        return headless::run_once(layout.as_ref(), &opts);
    }
    if args.layer_only {
        let layout = load_project_layout_tolerantly(project_override.as_deref())?;
        return headless::run_layer_stream(layout, &opts);
    }
    if let Some(n) = args.set_layer {
        return headless::run_set_layer(n, &opts);
    }
    if args.reset_layers {
        let layout = load_project_layout_tolerantly(project_override.as_deref())?;
        return headless::run_reset_layers(layout.as_ref(), &opts);
    }

    // Interactive paths need a project — the window renders the layout.
    let project = Project::discover(project_override.as_deref())?;
    let layout = project
        .canonical_layout()
        .context("loading canonical layout for watch GUI")?;
    let geometry = crate::schema::geometry::get(layout.geometry.as_str()).with_context(|| {
        format!(
            "unknown geometry '{}' in project kb.toml",
            layout.geometry.as_str()
        )
    })?;

    WatchApp::run(layout, geometry, opts)?;
    Ok(ExitCode::from(0))
}

/// Discover-and-load, but keep "no project here" separate from "broken
/// project": the former is fine on headless paths (print numeric layer
/// indices), the latter is a hard error.
fn load_project_layout_tolerantly(
    project_override: Option<&std::path::Path>,
) -> Result<Option<CanonicalLayout>> {
    match Project::discover(project_override) {
        Err(_) => Ok(None),
        Ok(project) => project
            .canonical_layout()
            .map(Some)
            .context("loading canonical layout"),
    }
}
