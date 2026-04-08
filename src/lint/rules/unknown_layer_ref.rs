//! `unknown-layer-ref` — layer action pointing to a nonexistent layer.

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::canonical::{CanonicalAction, LayerRef};

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "unknown-layer-ref"
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn description(&self) -> &'static str {
        "A layer-affecting action (`MO`, `LT`, `TG`, etc.) whose `layer` field points to a nonexistent layer index."
    }
    fn why_bad(&self) -> &'static str {
        "Build will fail (or, worse, silently activate the wrong layer if the index is technically valid in QMK's numbering)."
    }
    fn fix_example(&self) -> &'static str {
        "Fix the dangling reference by editing the visual layout in Oryx (or `layout.toml`). Either rebind the offending key to a layer that exists, or add the missing layer."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let known_names: Vec<&str> = ctx.layout.layers.iter().map(|l| l.name.as_str()).collect();
        let known_positions: Vec<u8> = ctx.layout.layers.iter().map(|l| l.position).collect();

        let mut out = Vec::new();
        for layer in &ctx.layout.layers {
            for (idx, key) in layer.keys.iter().enumerate() {
                for action in [&key.tap, &key.hold, &key.double_tap, &key.tap_hold]
                    .into_iter()
                    .flatten()
                {
                    check_action(
                        action,
                        &known_names,
                        &known_positions,
                        layer.name.as_str(),
                        idx,
                        self.id(),
                        self.severity(),
                        &mut out,
                    );
                }
            }
        }
        out
    }
}

#[allow(clippy::too_many_arguments)]
fn check_action(
    action: &CanonicalAction,
    known_names: &[&str],
    known_positions: &[u8],
    layer: &str,
    idx: usize,
    rule_id: &'static str,
    severity: crate::lint::Severity,
    out: &mut Vec<Issue>,
) {
    let check_ref = |r: &LayerRef, out: &mut Vec<Issue>| match r {
        LayerRef::Name(n) => {
            if !known_names.iter().any(|k| k == n) {
                out.push(Issue {
                    rule_id: rule_id.to_string(),
                    severity,
                    message: format!("unknown layer name '{n}'"),
                    layer: Some(layer.to_string()),
                    position_index: Some(idx),
                });
            }
        }
        LayerRef::Index(i) => {
            if !known_positions.contains(i) {
                out.push(Issue {
                    rule_id: rule_id.to_string(),
                    severity,
                    message: format!("unknown layer index {i}"),
                    layer: Some(layer.to_string()),
                    position_index: Some(idx),
                });
            }
        }
    };
    match action {
        CanonicalAction::Mo { layer: r }
        | CanonicalAction::Tg { layer: r }
        | CanonicalAction::To { layer: r }
        | CanonicalAction::Tt { layer: r }
        | CanonicalAction::Df { layer: r } => check_ref(r, out),
        CanonicalAction::Lt { layer: r, tap } => {
            check_ref(r, out);
            check_action(
                tap,
                known_names,
                known_positions,
                layer,
                idx,
                rule_id,
                severity,
                out,
            );
        }
        CanonicalAction::ModTap { tap, .. } => {
            check_action(
                tap,
                known_names,
                known_positions,
                layer,
                idx,
                rule_id,
                severity,
                out,
            );
        }
        _ => {}
    }
}
