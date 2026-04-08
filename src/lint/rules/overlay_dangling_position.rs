//! `overlay-dangling-position` — features.toml references a position/binding
//! that doesn't exist in the visual layout.

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::geometry;

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "overlay-dangling-position"
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn description(&self) -> &'static str {
        "`overlay/features.toml` references a position name (e.g., `L_pinky_home`) that doesn't exist in the current geometry, OR references a binding (`LT(SymNum, BSPC)`) where no key in the visual layout has that binding."
    }
    fn why_bad(&self) -> &'static str {
        "The generator can't resolve the reference. Build will fail or (worse) silently apply the feature to nothing."
    }
    fn fix_example(&self) -> &'static str {
        "Either fix the position/binding name in the TOML, or update Oryx to add the missing binding."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let mut out = Vec::new();
        let Some(geom) = geometry::get(ctx.layout.geometry.as_str()) else {
            return out;
        };
        // Combo position names must exist in the geometry.
        for combo in &ctx.features.combos {
            for pos in &combo.keys {
                if geom.position_to_index(pos).is_none() {
                    out.push(Issue {
                        rule_id: self.id().to_string(),
                        severity: self.severity(),
                        message: format!(
                            "combo references unknown position '{pos}' for geometry '{}'",
                            geom.id()
                        ),
                        layer: None,
                        position_index: None,
                    });
                }
            }
        }
        // Achordion bindings referencing layer names that don't exist.
        let known_layer_names: Vec<&str> =
            ctx.layout.layers.iter().map(|l| l.name.as_str()).collect();
        if let Some(achordion) = &ctx.features.achordion {
            for entry in &achordion.timeout {
                if let Some(layer) = extract_lt_layer(&entry.binding) {
                    if !known_layer_names.iter().any(|k| k == &layer) {
                        out.push(Issue {
                            rule_id: self.id().to_string(),
                            severity: self.severity(),
                            message: format!(
                                "achordion.timeout binding references unknown layer '{layer}'"
                            ),
                            layer: None,
                            position_index: None,
                        });
                    }
                }
            }
            for entry in &achordion.no_streak {
                if let Some(layer) = extract_lt_layer(&entry.binding) {
                    if !known_layer_names.iter().any(|k| k == &layer) {
                        out.push(Issue {
                            rule_id: self.id().to_string(),
                            severity: self.severity(),
                            message: format!(
                                "achordion.no_streak binding references unknown layer '{layer}'"
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

/// Extract the layer name from a `"LT(Name, KC)"` binding string, if any.
fn extract_lt_layer(binding: &str) -> Option<String> {
    let rest = binding.trim().strip_prefix("LT(")?.strip_suffix(')')?;
    let (layer, _rest) = rest.split_once(',')?;
    Some(layer.trim().to_string())
}
