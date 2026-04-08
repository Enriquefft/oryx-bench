//! `unbound-tapping-term` — `[[tapping_term_per_key]]` references a
//! binding that doesn't exist anywhere in the visual layout.
//!
//! Catches the same class of bug as `overlay-dangling-keycode`, scoped
//! to the per-key tapping term overrides instead of key overrides.

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::canonical::CanonicalAction;
use crate::schema::layout::parse_action;

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "unbound-tapping-term"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn description(&self) -> &'static str {
        "`[[tapping_term_per_key]]` references a binding that doesn't exist anywhere in the visual layout."
    }
    fn why_bad(&self) -> &'static str {
        "The `get_tapping_term` switch case will never fire because no key in the layout matches the binding. The override is dead code that takes flash space."
    }
    fn fix_example(&self) -> &'static str {
        "Either bind the keycode in Oryx (or `layout.toml`), or remove the `[[tapping_term_per_key]]` entry."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let mut out = Vec::new();
        for entry in &ctx.features.tapping_term_per_key {
            let parsed = parse_action(&entry.binding);
            let found = ctx.layout.layers.iter().any(|l| {
                l.keys.iter().any(|k| {
                    key_action_equals(&k.tap, &parsed) || key_action_equals(&k.hold, &parsed)
                })
            });
            if !found {
                out.push(Issue {
                    rule_id: self.id().to_string(),
                    severity: self.severity(),
                    message: format!(
                        "tapping_term_per_key references unbound binding '{}'",
                        entry.binding
                    ),
                    layer: None,
                    position_index: None,
                });
            }
        }
        out
    }
}

fn key_action_equals(slot: &Option<CanonicalAction>, want: &CanonicalAction) -> bool {
    let Some(action) = slot else {
        return false;
    };
    action.display() == want.display()
}
