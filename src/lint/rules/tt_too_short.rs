//! `tt-too-short` — effective TAPPING_TERM < 150ms with mod-taps in use.

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::canonical::CanonicalAction;

/// QMK's compiled-in default `TAPPING_TERM` (200ms) when the user
/// hasn't set one in `features.toml [config]`.
const QMK_DEFAULT_TAPPING_TERM_MS: u32 = 200;

/// The lower bound below which tap-hold disambiguation becomes
/// unreliable even for fast typists — at or below this value, the
/// rule fires. The choice of 150ms reflects empirical reports from
/// QMK community testing across home-row mod users; values below
/// it produce constant misfires regardless of typing speed.
const TAPPING_TERM_MINIMUM_MS: u32 = 150;

/// What we recommend the user *raise* the term to when the rule
/// fires. Strictly higher than the minimum so that fixing the warning
/// also gives a comfortable safety margin.
const TAPPING_TERM_RECOMMENDED_MS: u32 = 180;

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "tt-too-short"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn description(&self) -> &'static str {
        "Effective `TAPPING_TERM` is strictly below the 150ms disambiguation minimum (after considering `features.toml [config]` overrides) when any mod-tap or layer-tap is in the layout."
    }
    fn why_bad(&self) -> &'static str {
        "Below the minimum, the tap/hold boundary is too tight even for fast typists. Constant misfires."
    }
    fn fix_example(&self) -> &'static str {
        // Recommended is intentionally HIGHER than the 150ms threshold so
        // that fixing the warning gives a comfortable safety margin
        // rather than landing the user right back on the boundary. The
        // 200–220ms "sweet spot" reflects typical home-row-mod typing
        // speeds with achordion enabled — see the overlay cookbook.
        "Set `tapping_term_ms` in `[config]` of `features.toml` to at least 180ms. 200–220ms is the sweet spot for most users."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        // Any mod-tap or layer-tap triggers the rule.
        let uses_tap_hold = ctx.layout.layers.iter().any(|l| {
            l.keys.iter().any(|k| {
                matches!(
                    &k.tap,
                    Some(CanonicalAction::Lt { .. } | CanonicalAction::ModTap { .. })
                )
            })
        });
        if !uses_tap_hold {
            return Vec::new();
        }
        // Default QMK tapping term is 200ms. features.toml overrides it.
        // If the user's tapping_term_ms value is malformed, surface
        // that as its own rule-scoped issue rather than swallowing —
        // the `unbound-tapping-term` path is a separate rule, but we
        // still need this one to fail visibly instead of pretending
        // the default is in effect.
        let effective = match ctx.features.tapping_term_ms() {
            Ok(Some(ms)) => ms,
            Ok(None) => QMK_DEFAULT_TAPPING_TERM_MS,
            Err(e) => {
                return vec![Issue {
                    rule_id: self.id().to_string(),
                    severity: Severity::Error,
                    message: format!("invalid tapping_term_ms in features.toml: {e}"),
                    layer: None,
                    position_index: None,
                }];
            }
        };
        if effective < TAPPING_TERM_MINIMUM_MS {
            vec![Issue {
                rule_id: self.id().to_string(),
                severity: self.severity(),
                message: format!(
                    "effective TAPPING_TERM = {effective}ms (below {TAPPING_TERM_MINIMUM_MS}ms minimum); increase to ≥{TAPPING_TERM_RECOMMENDED_MS}ms"
                ),
                layer: None,
                position_index: None,
            }]
        } else {
            Vec::new()
        }
    }
}
