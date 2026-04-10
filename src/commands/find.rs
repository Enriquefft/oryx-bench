//! `oryx-bench find QUERY` — search across all layers.
//!
//! Query syntax:
//!   KC_<NAME>           positions sending this keycode
//!   layer:<NAME>        all bindings on a layer
//!   hold:<KEYCODE>      keys with this on hold
//!   anti:<RULE_ID>      instances of a lint rule
//!   position:<NAME>     same as `explain`

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use crate::config::Project;
use crate::lint;
use crate::schema::canonical::CanonicalAction;
use crate::schema::keycode::{Keycode, Modifier};

#[derive(Parser, Debug)]
pub struct Args {
    /// Query string.
    pub query: String,
}

pub fn run(args: Args, project_override: Option<PathBuf>) -> Result<ExitCode> {
    let project = Project::discover(project_override.as_deref())?;
    let layout = super::show::load_layout_for_explain(&project)?;

    let query = args.query.trim();
    if let Some(name) = query.strip_prefix("layer:") {
        let Some(layer) = layout
            .layers
            .iter()
            .find(|l| l.name.eq_ignore_ascii_case(name))
        else {
            anyhow::bail!("no layer named '{name}'");
        };
        let geom = crate::schema::geometry::get(project.cfg.layout.geometry.as_str());
        for (i, key) in layer.keys.iter().enumerate() {
            // Skip empty keys (KC_NO with no hold/double_tap/tap_hold).
            if key.tap.is_none() && key.hold.is_none() && key.double_tap.is_none() && key.tap_hold.is_none() {
                continue;
            }
            if matches!(&key.tap, Some(CanonicalAction::None)) && key.hold.is_none() && key.double_tap.is_none() && key.tap_hold.is_none() {
                continue;
            }
            let pos = geom
                .as_ref()
                .and_then(|g| g.index_to_position(i))
                .unwrap_or("?");
            println!("  {pos:>16} [{i:>2}] {}", key.display());
        }
    } else if let Some(kc) = query.strip_prefix("hold:") {
        let normalized = normalize_keycode_query(kc);
        for layer in &layout.layers {
            for (i, key) in layer.keys.iter().enumerate() {
                if let Some(hold) = &key.hold {
                    if action_matches_keycode(hold, &normalized) {
                        println!("  {:<15} [{i:>2}] {}", layer.name, key.display());
                    }
                }
            }
        }
    } else if let Some(rule) = query.strip_prefix("anti:") {
        let issues = lint::run_all(&layout, &project)?;
        for issue in issues.iter().filter(|i| i.rule_id == rule) {
            println!(
                "  {}  layer={} pos={}  {}",
                issue.rule_id,
                issue.layer.as_deref().unwrap_or("-"),
                issue
                    .position_index
                    .map(|i: usize| i.to_string())
                    .unwrap_or_else(|| "-".into()),
                issue.message
            );
        }
    } else if let Some(pos) = query.strip_prefix("position:") {
        // Delegate to explain but return exit 0 ourselves — explain's
        // lint annotations shouldn't make find report failure.
        super::explain::run(
            super::explain::Args {
                position: pos.to_string(),
            },
            project_override,
        )?;
    } else {
        // Plain keycode search. Accepts long-form aliases (BACKSPACE,
        // SPACE, LSHIFT), with or without the KC_ prefix.
        let normalized = normalize_keycode_query(query);
        let mut any = false;
        for layer in &layout.layers {
            for (i, key) in layer.keys.iter().enumerate() {
                if key.references_keycode(&normalized) {
                    println!("  {:<15} [{i:>2}] {}", layer.name, key.display());
                    any = true;
                }
            }
        }
        if !any {
            anyhow::bail!(
                "no matches for '{query}'. Try KC_BSPC, layer:Main, hold:LSHIFT, anti:lt-on-high-freq, or position:R_thumb_outer"
            );
        }
    }
    Ok(ExitCode::from(0))
}

/// Normalize a user query to the canonical keycode form. Resolves
/// long-form aliases (BACKSPACE → KC_BSPC, LSHIFT → KC_LSFT) through
/// the keycode and modifier catalogs so the user doesn't have to
/// memorize QMK short names.
fn normalize_keycode_query(q: &str) -> String {
    let upper = q.trim().to_ascii_uppercase();
    let bare = upper.strip_prefix("KC_").unwrap_or(&upper);

    // Try the keycode catalog — handles aliases like BACKSPACE→KC_BSPC,
    // SPACE→KC_SPACE, ENTER→KC_ENTER, etc.
    let kc = Keycode::from_str(bare);
    if !matches!(kc, Keycode::Other(_)) {
        return kc.canonical_name().into_owned();
    }

    // Try modifier aliases: LSHIFT→LSFT, LCTRL→LCTL, etc.
    if let Some(m) = Modifier::from_str(bare) {
        return format!("KC_{}", m.canonical_name());
    }

    // Fall back to the raw uppercased form with KC_ prefix.
    if upper.starts_with("KC_") {
        upper
    } else {
        format!("KC_{upper}")
    }
}

fn action_matches_keycode(action: &CanonicalAction, code: &str) -> bool {
    match action {
        CanonicalAction::Keycode(kc) => kc.canonical_name().eq_ignore_ascii_case(code),
        CanonicalAction::Modifier(m) => {
            let qmk = format!("KC_{}", m.canonical_name());
            qmk.eq_ignore_ascii_case(code) || m.canonical_name().eq_ignore_ascii_case(code)
        }
        CanonicalAction::ModTap { tap, .. } | CanonicalAction::Lt { tap, .. } => {
            action_matches_keycode(tap, code)
        }
        _ => false,
    }
}
