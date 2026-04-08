//! `unreferenced-custom-keycode` — overlay defines a macro slot nothing binds.

use std::collections::HashSet;

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::canonical::CanonicalAction;

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "unreferenced-custom-keycode"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn description(&self) -> &'static str {
        "An overlay defines a custom keycode (`[[macros]]` with a `slot` or a `.zig` dispatch) but no layer in the visual layout binds that USERnn slot."
    }
    fn why_bad(&self) -> &'static str {
        "Dead code."
    }
    fn fix_example(&self) -> &'static str {
        "Either bind it in Oryx, or remove from `features.toml`."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let mut bound: HashSet<u8> = HashSet::new();
        for layer in &ctx.layout.layers {
            for key in &layer.keys {
                for slot in [&key.tap, &key.hold, &key.double_tap, &key.tap_hold] {
                    if let Some(CanonicalAction::Custom(n)) = slot {
                        bound.insert(*n);
                    }
                }
            }
        }
        let mut out = Vec::new();
        for m in &ctx.features.macros {
            if let Some(slot) = &m.slot {
                if let Some(n) = slot.strip_prefix("USER").and_then(|s| s.parse::<u8>().ok()) {
                    if !bound.contains(&n) {
                        out.push(Issue {
                            rule_id: self.id().to_string(),
                            severity: self.severity(),
                            message: format!(
                                "macro '{}' defined for slot {slot} but nothing binds it",
                                m.name
                            ),
                            layer: None,
                            position_index: None,
                        });
                    }
                }
            }
        }
        out
    }
}
