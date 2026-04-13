//! `lt-on-high-freq` — Layer-tap on Backspace/Space/Enter/Delete/Tab/Esc.
//!
//! The canonical "LT on high-frequency key" footgun. Achordion is the
//! canonical fix, not moving the key.

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::canonical::CanonicalAction;

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "lt-on-high-freq"
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn oryx_severity(&self) -> Severity {
        // LT on a high-frequency key is a valid Oryx design choice. The
        // user configured it in the Oryx UI and it compiles fine in QMK.
        // Only flag as an error once the user takes ownership (detach).
        Severity::Warning
    }
    fn description(&self) -> &'static str {
        "Layer-tap (`LT(layer, key)`) where `key` is one of `KC_BSPC`, `KC_SPC`, `KC_ENT`, `KC_DEL`, `KC_TAB`, or `KC_ESC`."
    }
    fn why_bad(&self) -> &'static str {
        "Tap-hold resolves on a tapping term. Below it = tap; above = hold. For high-frequency keys you press hundreds of times an hour, the boundary is hit constantly: a fast Backspace burst crosses the term and triggers the layer; a brief intentional layer hold falls below the term and injects a stray Backspace."
    }
    fn fix_example(&self) -> &'static str {
        "Almost never \"move the key\" — add achordion to `overlay/features.toml`. Achordion forces tap-hold to only resolve as hold when the next key is on the opposite hand. See `reference/overlay-cookbook.md#achordion`."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let achordion = ctx.features.achordion.as_ref().filter(|a| a.enabled);
        let mut out = Vec::new();
        for layer in &ctx.layout.layers {
            for (idx, key) in layer.keys.iter().enumerate() {
                if let Some(lt @ CanonicalAction::Lt { tap, .. }) = &key.tap {
                    if let Some(kc) = tap.tap_keycode() {
                        if kc.is_high_frequency() {
                            if let Some(a) = achordion {
                                let binding = lt.display();
                                if let Some(t) = a.timeout.iter().find(|t| t.binding == binding) {
                                    out.push(Issue {
                                        rule_id: self.id().to_string(),
                                        severity: Severity::Info,
                                        message: format!(
                                            "LT on high-frequency key {} — achordion configured (timeout {}ms)",
                                            kc.canonical_name(),
                                            t.ms,
                                        ),
                                        layer: Some(layer.name.clone()),
                                        position_index: Some(idx),
                                    });
                                    continue;
                                }
                            }
                            out.push(Issue {
                                rule_id: self.id().to_string(),
                                severity: self.severity(),
                                message: format!(
                                    "LT on high-frequency key {} — add achordion",
                                    kc.canonical_name()
                                ),
                                layer: Some(layer.name.clone()),
                                position_index: Some(idx),
                            });
                        }
                    }
                }
            }
        }
        out
    }
}
