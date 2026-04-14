//! `oryx-bench upgrade-check` — re-run lint with the current keycode catalog.
//!
//! After bumping `oryx-bench`, this command re-runs lint against the
//! project and surfaces:
//!
//! - Any keycodes that fall through to `Keycode::Other` (which means QMK
//!   may have renamed or removed them, or this release of `oryx-bench`
//!   doesn't yet catalog them).
//! - The current set of registered lint rules, so users can see what's
//!   new since their last release.
//! - A summary of error/warning/info counts.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use crate::config::Project;
use crate::lint::{self, Severity};
use crate::schema::canonical::CanonicalAction;
use crate::schema::keycode::Keycode;

#[derive(Parser, Debug)]
pub struct Args {}

pub fn run(_args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    let project = Project::discover(project_override.as_deref())?;
    let layout = project.canonical_layout()?;

    println!("== oryx-bench upgrade check ==");
    println!("oryx-bench version: {}", env!("CARGO_PKG_VERSION"));
    println!();

    // Walk the layout looking for Keycode::Other instances.
    let mut unknown: Vec<(String, String, usize)> = Vec::new();
    for layer in &layout.layers {
        for (idx, key) in layer.keys.iter().enumerate() {
            for action in [&key.tap, &key.hold, &key.double_tap, &key.tap_hold]
                .into_iter()
                .flatten()
            {
                if let Some(name) = uncatalogued_keycode(action) {
                    unknown.push((layer.name.clone(), name, idx));
                }
            }
        }
    }

    if unknown.is_empty() {
        println!(
            "{} Every keycode in the layout is catalogued.",
            crate::util::term::OK
        );
    } else {
        println!(
            "{} Found {} uncatalogued keycode(s):",
            crate::util::term::WARN,
            unknown.len()
        );
        for (layer, kc, idx) in &unknown {
            println!("  layer '{layer}' position {idx}: {kc}");
        }
        println!();
        println!(
            "  These may have been renamed in QMK, removed, or simply not yet known to oryx-bench."
        );
        println!("  Codegen still emits them verbatim, so the firmware may build fine — but lint");
        println!("  rules can't reason about them. File an issue with the keycode names if you");
        println!("  expect them to be catalogued.");
    }
    println!();

    // Re-run lint and summarize.
    let issues = lint::run_all(&layout, &project)?;
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
    println!("Lint: {errors} error(s), {warnings} warning(s), {info} info");
    println!();

    println!("Registered lint rules ({}):", lint::rules::registry().len());
    for rule in lint::rules::registry() {
        println!(
            "  {} ({:?}) — {}",
            rule.id(),
            rule.severity(),
            rule.description()
        );
    }
    Ok(ExitCode::from(0))
}

/// Walk an action tree looking for `Keycode::Other(name)` and return
/// the first such name. Returns `None` for fully-catalogued bindings.
fn uncatalogued_keycode(action: &CanonicalAction) -> Option<String> {
    match action {
        CanonicalAction::Keycode(Keycode::Other(name)) => Some(name.clone()),
        CanonicalAction::Lt { tap, .. } | CanonicalAction::ModTap { tap, .. } => {
            uncatalogued_keycode(tap)
        }
        _ => None,
    }
}
