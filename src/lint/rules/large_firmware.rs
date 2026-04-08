//! `large-firmware` — info-level reminder when the most-recent build
//! produced a `.bin` close to the target board's flash budget.
//!
//! Reads `.oryx-bench/build/firmware.bin` if present. Doesn't run a
//! build itself.

use crate::build;
use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::geometry;

pub struct Rule;

/// Minimum absolute headroom (4 KB) before the warning fires.
/// Establishes the floor on tiny boards like the Voyager (64 KB) so
/// the warning kicks in at 60 KB / 93.75%.
const WARNING_HEADROOM_FLOOR_BYTES: u64 = 4 * 1024;

/// Fractional headroom (1/16 of the budget) — used for boards big
/// enough that 4 KB is a rounding error. On a hypothetical 16 MB
/// board the rule fires at 15 MB (93.75%) instead of 16 MB - 4 KB
/// (99.97%, useless).
///
/// `effective_headroom = max(WARNING_HEADROOM_FLOOR_BYTES, budget / WARNING_HEADROOM_DIVISOR)`
/// gives the right behavior across both scales without per-board
/// tuning. The denominator is `16` (12.5%) because anything tighter
/// produces noise on small boards and anything looser misses real
/// pressure on large boards. The 1/16 = 6.25% figure also matches
/// typical embedded "approaching the limit" warnings in tools like
/// platformio's size reports.
const WARNING_HEADROOM_DIVISOR: u64 = 16;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "large-firmware"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn description(&self) -> &'static str {
        "The most-recent build produced a firmware image close to the target board's flash budget."
    }
    fn why_bad(&self) -> &'static str {
        "The board has a fixed flash size; once you cross the budget the build fails to link. Approaching it gradually is fine, but you'll want to know which feature flag last pushed you over."
    }
    fn fix_example(&self) -> &'static str {
        "Run `oryx-bench build --emit-overlay-c` and inspect the generated rules.mk for feature flags you don't use. Disable any feature you're not actually consuming. Avoid `MOUSEKEY_ENABLE` if you're not using mouse keys."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let firmware = build::firmware_path(ctx.project);
        let Ok(meta) = std::fs::metadata(&firmware) else {
            return Vec::new();
        };
        let size = meta.len();

        // Pull the budget from the board's geometry trait so this rule
        // doesn't need a per-board branch when we add a second board.
        // If the geometry is unknown, surface a diagnostic instead of
        // silently no-op'ing — a future canonical-layout pass might
        // produce a layout with an unrecognized geometry slug, and the
        // user deserves a hint that this rule couldn't run, not silence.
        let Some(geom) = geometry::get(ctx.layout.geometry.as_str()) else {
            return vec![Issue {
                rule_id: self.id().to_string(),
                severity: Severity::Info,
                message: format!(
                    "skipped: cannot determine flash budget for unknown geometry '{}'",
                    ctx.layout.geometry
                ),
                layer: None,
                position_index: None,
            }];
        };
        let budget = geom.flash_budget_bytes();
        // Headroom = max(absolute floor, fraction of budget). Floor
        // wins on small boards (Voyager 64 KB → 4 KB headroom);
        // fraction wins on large boards. See the const definitions
        // for the rationale on each value.
        let headroom = WARNING_HEADROOM_FLOOR_BYTES.max(budget / WARNING_HEADROOM_DIVISOR);
        let warning_threshold = budget.saturating_sub(headroom);

        if size >= warning_threshold {
            let pct = (size as f64 / budget as f64) * 100.0;
            let kb = budget / 1024;
            vec![Issue {
                rule_id: self.id().to_string(),
                severity: self.severity(),
                message: format!(
                    "firmware is {size} bytes ({pct:.1}% of the {} ~{kb}KB budget)",
                    geom.display_name()
                ),
                layer: None,
                position_index: None,
            }]
        } else {
            Vec::new()
        }
    }
}
