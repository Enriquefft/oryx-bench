//! `duplicate-action` — two positions on the same layer producing the same effect.

use std::collections::HashMap;

use crate::lint::{Issue, LintContext, LintRule, Severity};

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "duplicate-action"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn description(&self) -> &'static str {
        "Two positions on the same layer producing the same effect."
    }
    fn why_bad(&self) -> &'static str {
        "Often intentional (e.g., Backspace bound on a thumb and duplicated in the symbol layer's row 2 so you can erase while holding the layer key). Flagged as info so you can review and accept."
    }
    fn fix_example(&self) -> &'static str {
        "Review and accept, or remove the duplicate if unintended."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let mut out = Vec::new();
        for layer in &ctx.layout.layers {
            let mut seen: HashMap<String, usize> = HashMap::new();
            for (idx, key) in layer.keys.iter().enumerate() {
                let disp = key.display();
                if disp == "KC_NO" || disp == "KC_TRNS" {
                    continue;
                }
                if let Some(prev) = seen.get(&disp) {
                    out.push(Issue {
                        rule_id: self.id().to_string(),
                        severity: self.severity(),
                        message: format!(
                            "duplicate binding '{disp}' at positions {prev} and {idx}"
                        ),
                        layer: Some(layer.name.clone()),
                        position_index: Some(idx),
                    });
                } else {
                    seen.insert(disp, idx);
                }
            }
        }
        out
    }
}
