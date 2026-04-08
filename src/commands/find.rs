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
        for (i, key) in layer.keys.iter().enumerate() {
            println!("  [{:>2}] {}", i, key.display());
        }
    } else if let Some(kc) = query.strip_prefix("hold:") {
        for layer in &layout.layers {
            for (i, key) in layer.keys.iter().enumerate() {
                if let Some(hold) = &key.hold {
                    if action_matches_keycode(hold, kc) {
                        println!("  {:<15} [{:>2}] {}", layer.name, i, key.display());
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
        return super::explain::run(
            super::explain::Args {
                position: pos.to_string(),
            },
            project_override,
        );
    } else {
        // Plain keycode search. Accepts upper or lowercase, with or
        // without the `KC_` prefix. Bare letters/digits like "A" or "1"
        // are normalized to `KC_A` / `KC_1`.
        let normalized = normalize_keycode_query(query);
        let mut any = false;
        for layer in &layout.layers {
            for (i, key) in layer.keys.iter().enumerate() {
                if key.references_keycode(&normalized) {
                    println!("  {:<15} [{:>2}] {}", layer.name, i, key.display());
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

fn normalize_keycode_query(q: &str) -> String {
    let upper = q.trim().to_ascii_uppercase();
    if upper.starts_with("KC_") {
        upper
    } else {
        format!("KC_{upper}")
    }
}

fn action_matches_keycode(action: &CanonicalAction, code: &str) -> bool {
    match action {
        CanonicalAction::Keycode(kc) => kc.canonical_name().eq_ignore_ascii_case(code),
        CanonicalAction::Modifier(m) => m.canonical_name().eq_ignore_ascii_case(code),
        CanonicalAction::ModTap { tap, .. } | CanonicalAction::Lt { tap, .. } => {
            action_matches_keycode(tap, code)
        }
        _ => false,
    }
}
