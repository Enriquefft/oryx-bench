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
        Severity::Warning
    }
    fn description(&self) -> &'static str {
        "Two layers whose titles sanitize to the same C identifier. For example, `\"Sym + Num\"` and `\"Sym Num\"` both sanitize to `SYM_NUM`."
    }
    fn why_bad(&self) -> &'static str {
        "The codegen auto-disambiguates colliding layer names by appending the layer position (e.g. LAYER_1, LAYER_2). This works, but unique layer names improve readability of the generated C code."
    }
    fn fix_example(&self) -> &'static str {
        "Rename the colliding layers in Oryx (or `layout.toml`) to have distinct names so the generated enum members are self-documenting."
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
