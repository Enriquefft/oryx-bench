//! `layer-name-collision` — two layers whose titles sanitize to the same C identifier.

use std::collections::HashMap;

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::naming::sanitize_c_ident;

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "layer-name-collision"
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn description(&self) -> &'static str {
        "Two layers whose titles sanitize to the same C identifier. For example, `\"Sym + Num\"` and `\"Sym Num\"` both sanitize to `SYM_NUM`."
    }
    fn why_bad(&self) -> &'static str {
        "The generator can't produce a valid `enum layers` with duplicate names; build fails."
    }
    fn fix_example(&self) -> &'static str {
        "Rename one of the colliding layers in Oryx (or `layout.toml`) so their sanitized identifiers differ."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let mut by_ident: HashMap<String, Vec<&str>> = HashMap::new();
        for layer in &ctx.layout.layers {
            by_ident
                .entry(sanitize_c_ident(&layer.name))
                .or_default()
                .push(layer.name.as_str());
        }
        let mut out = Vec::new();
        for (ident, names) in by_ident {
            if names.len() > 1 {
                out.push(Issue {
                    rule_id: self.id().to_string(),
                    severity: self.severity(),
                    message: format!(
                        "layer name collision: {} sanitize to '{ident}'",
                        names.join(", ")
                    ),
                    layer: None,
                    position_index: None,
                });
            }
        }
        out
    }
}
