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
    fn oryx_severity(&self) -> Severity {
        // Oryx defaults all layers to "Layer" — collisions are expected
        // on a fresh pull. Purely informational until the user renames
        // them after detach.
        Severity::Info
    }
    fn description(&self) -> &'static str {
        "Two layers whose titles sanitize to the same C identifier. For example, `\"Sym + Num\"` and `\"Sym Num\"` both sanitize to `SYM_NUM`."
    }
    fn why_bad(&self) -> &'static str {
        "Layer references in `layout.toml` (e.g. `LT(Layer, KC_ENT)`) use the original name. When two layers share a name, the reference is ambiguous — codegen can only resolve it to one of them. The generated C enum is auto-disambiguated (`LAYER_1`, `LAYER_2`), but the `layout.toml` name lookup silently picks one, and the other layer becomes unreferenceable by name."
    }
    fn fix_example(&self) -> &'static str {
        "Rename the colliding layers in `layout.toml` to have distinct names (e.g. `Symbols`, `Nav`). In Oryx mode, rename them in the Oryx UI and re-pull."
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
