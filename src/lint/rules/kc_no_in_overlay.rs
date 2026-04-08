//! `kc-no-in-overlay` — `KC_NO` on a non-base layer where the base has a real binding.

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::canonical::CanonicalAction;

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "kc-no-in-overlay"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn description(&self) -> &'static str {
        "A non-base layer position bound to `KC_NO` (dead key) when the base layer at the same position has a real binding. Almost always the user meant `KC_TRANSPARENT` (fall-through)."
    }
    fn why_bad(&self) -> &'static str {
        "`KC_NO` does nothing; `KC_TRANSPARENT` falls through to the next active layer. They look identical in Oryx's grid view but produce wildly different behavior."
    }
    fn fix_example(&self) -> &'static str {
        "In Oryx (or `layout.toml`), open the affected layer and set the position to \"Transparent\" instead of \"Empty\"."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let Some(base) = ctx.layout.layers.iter().find(|l| l.position == 0) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for layer in &ctx.layout.layers {
            if layer.position == 0 {
                continue;
            }
            for (idx, key) in layer.keys.iter().enumerate() {
                let is_kc_no = matches!(&key.tap, Some(CanonicalAction::None))
                    || matches!(
                        &key.tap,
                        Some(CanonicalAction::Keycode(k))
                            if matches!(k, crate::schema::keycode::Keycode::KcNo)
                    );
                if is_kc_no {
                    let base_has_binding = base
                        .keys
                        .get(idx)
                        .map(|k| {
                            !matches!(&k.tap, Some(CanonicalAction::None) | None)
                                || !matches!(&k.hold, Some(CanonicalAction::None) | None)
                        })
                        .unwrap_or(false);
                    if base_has_binding {
                        out.push(Issue {
                            rule_id: self.id().to_string(),
                            severity: self.severity(),
                            message: format!(
                                "KC_NO on layer '{}' position {idx}; base layer has a real binding — probably meant KC_TRNS",
                                layer.name
                            ),
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
