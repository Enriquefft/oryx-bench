//! `unknown-keycode` — a code that doesn't match any catalogued QMK keycode.

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::keycode::Keycode;

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "unknown-keycode"
    }
    fn severity(&self) -> Severity {
        // Forward-compat invariant: Keycode::Other(_) is the catch-all for
        // keycodes we haven't catalogued yet. The codegen layer emits the
        // literal verbatim, so build still works for QMK-valid names. We
        // surface this as a *warning* so users can review, not an error
        // that blocks lint from passing on real layouts.
        Severity::Warning
    }
    fn description(&self) -> &'static str {
        "A `code` field in pulled JSON that doesn't match any catalogued QMK keycode."
    }
    fn why_bad(&self) -> &'static str {
        "Either Oryx introduced a new code we haven't catalogued, or the JSON is corrupt. The generator emits the literal string into the generated `keymap.c`, which compiles if QMK knows the symbol but lint can't reason about it (high-frequency-key detection, vowel detection, etc.)."
    }
    fn fix_example(&self) -> &'static str {
        "File an issue with the unknown code name. We add it to `src/schema/keycode.rs` and ship a new release. As a workaround, the catch-all `Keycode::Other(String)` preserves the literal so manual intervention is possible."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let mut out = Vec::new();
        for layer in &ctx.layout.layers {
            for (idx, key) in layer.keys.iter().enumerate() {
                for action in [&key.tap, &key.hold, &key.double_tap, &key.tap_hold]
                    .into_iter()
                    .flatten()
                {
                    if let Some(Keycode::Other(name)) = action.tap_keycode() {
                        out.push(Issue {
                            rule_id: self.id().to_string(),
                            severity: self.severity(),
                            message: format!("unknown keycode '{name}'"),
                            layer: Some(layer.name.clone()),
                            position_index: Some(idx),
                        });
                    }
                }
            }
        }
        out
    }
}
