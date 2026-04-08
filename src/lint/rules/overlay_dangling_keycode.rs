//! `overlay-dangling-keycode` — features.toml key override references a keycode
//! not bound anywhere in the visual layout.

use crate::lint::{Issue, LintContext, LintRule, Severity};

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "overlay-dangling-keycode"
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn description(&self) -> &'static str {
        "`overlay/features.toml` references a keycode (e.g., for a key override) that isn't bound anywhere in the visual layout."
    }
    fn why_bad(&self) -> &'static str {
        "A key override on a key that doesn't exist on the keyboard is silently dead — the override never fires."
    }
    fn fix_example(&self) -> &'static str {
        "Either bind the keycode in Oryx, or remove the override."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let mut out = Vec::new();
        for ko in &ctx.features.key_overrides {
            let kc = &ko.key;
            let found = ctx
                .layout
                .layers
                .iter()
                .any(|l| l.keys.iter().any(|k| k.references_keycode(kc)));
            if !found {
                out.push(Issue {
                    rule_id: self.id().to_string(),
                    severity: self.severity(),
                    message: format!(
                        "key_override on keycode '{kc}' — not bound in any visual layer"
                    ),
                    layer: None,
                    position_index: None,
                });
            }
        }
        out
    }
}
