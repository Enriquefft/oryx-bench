//! `orphaned-mod-tap` — key with `tap: null` and `hold: <plain modifier>`.

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::canonical::CanonicalAction;

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "orphaned-mod-tap"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn description(&self) -> &'static str {
        "A key with `tap: null` and `hold: <plain modifier>`. This is the encoding Oryx produces when you start with a mod-tap and clear the tap action."
    }
    fn why_bad(&self) -> &'static str {
        "Functionally works as a plain modifier, but the encoding signals \"this used to be a mod-tap\" and creates code-review confusion."
    }
    fn fix_example(&self) -> &'static str {
        "In Oryx (or `layout.toml`), remove the mod-tap and re-add the same position as a plain modifier."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let mut out = Vec::new();
        for layer in &ctx.layout.layers {
            for (idx, key) in layer.keys.iter().enumerate() {
                if key.tap.is_none() {
                    if let Some(CanonicalAction::Modifier(_)) = &key.hold {
                        out.push(Issue {
                            rule_id: self.id().to_string(),
                            severity: self.severity(),
                            message: "orphaned mod-tap: tap=null, hold=plain modifier".into(),
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
