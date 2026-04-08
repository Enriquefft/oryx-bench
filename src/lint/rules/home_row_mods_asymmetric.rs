//! `home-row-mods-asymmetric` — home-row mods on one half but not the other.

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::canonical::CanonicalAction;
use crate::schema::geometry::{self, Hand};

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "home-row-mods-asymmetric"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn description(&self) -> &'static str {
        "Home-row mods on the left half but not the right (or vice versa)."
    }
    fn why_bad(&self) -> &'static str {
        "Asymmetric mods make muscle memory harder."
    }
    fn fix_example(&self) -> &'static str {
        "Either accept the asymmetry, or mirror the stack by editing the visual layout in Oryx (or `layout.toml`) so both halves use the same mod order."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let Some(geom) = geometry::get(ctx.layout.geometry.as_str()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for layer in &ctx.layout.layers {
            let mut left = 0;
            let mut right = 0;
            for (idx, key) in layer.keys.iter().enumerate() {
                // A "home-row mod" is anything that resolves to a mod
                // when held: ModTap directly, or an orphaned mod-tap
                // (tap=None, hold=Modifier).
                let is_mod_tap = matches!(&key.tap, Some(CanonicalAction::ModTap { .. }))
                    || matches!(&key.hold, Some(CanonicalAction::ModTap { .. }))
                    || (key.tap.is_none()
                        && matches!(&key.hold, Some(CanonicalAction::Modifier(_))));
                if is_mod_tap {
                    match geom.hand(idx) {
                        Some(Hand::Left) => left += 1,
                        Some(Hand::Right) => right += 1,
                        None => {}
                    }
                }
            }
            if (left > 0 && right == 0) || (right > 0 && left == 0) {
                out.push(Issue {
                    rule_id: self.id().to_string(),
                    severity: self.severity(),
                    message: format!(
                        "asymmetric home-row mods on layer '{}': {left} left, {right} right",
                        layer.name
                    ),
                    layer: Some(layer.name.clone()),
                    position_index: None,
                });
            }
        }
        out
    }
}
