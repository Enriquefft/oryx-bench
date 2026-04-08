//! `oryx-newer-than-build` — the most-recent canonical layout differs
//! from what the last build saw.
//!
//! Compares the input sha256 of the *current* canonical layout +
//! features.toml + overlay/ against the sha stored at
//! `.oryx-bench/build/build.sha` by the most recent successful build.
//! Fires when they differ — meaning the user has pulled or edited
//! something but hasn't rebuilt yet.

use crate::build;
use crate::generate;
use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::features::FeaturesToml;
use crate::schema::geometry;

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "oryx-newer-than-build"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn description(&self) -> &'static str {
        "The current canonical layout + overlay differs from the inputs the most-recent build saw."
    }
    fn why_bad(&self) -> &'static str {
        "You pulled fresh state from Oryx (or edited an overlay file), but the firmware on your keyboard is still based on the previous inputs. Flashing now would re-flash the stale firmware."
    }
    fn fix_example(&self) -> &'static str {
        "`oryx-bench build` (and then `flash` after review)."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        // No build cache → no signal. Nothing to compare against.
        let cached = match std::fs::read_to_string(build::build_sha_path(ctx.project)) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        // Re-derive the current input sha. We need to actually run the
        // generators against the canonical layout to compute the same
        // hash that `build` would store.
        let Some(geom) = geometry::get(ctx.layout.geometry.as_str()) else {
            return Vec::new();
        };
        let features =
            FeaturesToml::load_or_default(&ctx.project.overlay_features_path()).unwrap_or_default();
        let overlay_dir = ctx.project.overlay_dir();
        let overlay_arg = if overlay_dir.exists() {
            Some(overlay_dir.as_path())
        } else {
            None
        };
        let generated = match generate::generate_all(ctx.layout, &features, geom, overlay_arg) {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let current = match build::input_sha(&generated, overlay_arg) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        if current.trim() == cached.trim() {
            Vec::new()
        } else {
            vec![Issue {
                rule_id: self.id().to_string(),
                severity: self.severity(),
                message: format!(
                    "current inputs sha {} differs from last-build sha {} — run `oryx-bench build`",
                    short(&current),
                    short(cached.trim()),
                ),
                layer: None,
                position_index: None,
            }]
        }
    }
}

/// Truncate a sha for human-readable output.
fn short(s: &str) -> String {
    s.chars().take(8).collect()
}
