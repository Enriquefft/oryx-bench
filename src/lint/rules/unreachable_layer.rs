//! `unreachable-layer` — a layer with no MO/LT/TG/TO/TT/DF reference.

use std::collections::HashSet;

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::canonical::{CanonicalAction, LayerRef};

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "unreachable-layer"
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn description(&self) -> &'static str {
        "A layer with no `MO`, `LT`, `TG`, `TO`, `TT`, or `DF` reference from any reachable layer."
    }
    fn why_bad(&self) -> &'static str {
        "A layer that can't be activated is dead code — it consumes firmware space and mental overhead."
    }
    fn fix_example(&self) -> &'static str {
        "In Oryx (or `layout.toml`), either delete the unreachable layer or add an activation key (MO/LT/TG/TO/TT/DF) pointing at it from a layer that is already reachable."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        // Collect referenced layer names/indices.
        let mut referenced: HashSet<String> = HashSet::new();
        for layer in &ctx.layout.layers {
            for key in &layer.keys {
                for action in [&key.tap, &key.hold, &key.double_tap, &key.tap_hold]
                    .into_iter()
                    .flatten()
                {
                    collect_refs(action, &mut referenced, &ctx.layout.layers);
                }
            }
        }
        let mut out = Vec::new();
        for layer in &ctx.layout.layers {
            // Layer 0 (the base) is always reachable.
            if layer.position == 0 {
                continue;
            }
            if !referenced.contains(&layer.name) {
                out.push(Issue {
                    rule_id: self.id().to_string(),
                    severity: self.severity(),
                    message: format!("layer '{}' is unreachable", layer.name),
                    layer: Some(layer.name.clone()),
                    position_index: None,
                });
            }
        }
        out
    }
}

fn collect_refs(
    action: &CanonicalAction,
    out: &mut HashSet<String>,
    layers: &[crate::schema::canonical::CanonicalLayer],
) {
    let add = |r: &LayerRef, out: &mut HashSet<String>| match r {
        LayerRef::Name(n) => {
            out.insert(n.clone());
        }
        LayerRef::Index(i) => {
            if let Some(l) = layers.iter().find(|l| l.position == *i) {
                out.insert(l.name.clone());
            }
        }
    };
    match action {
        CanonicalAction::Mo { layer }
        | CanonicalAction::Tg { layer }
        | CanonicalAction::To { layer }
        | CanonicalAction::Tt { layer }
        | CanonicalAction::Df { layer } => add(layer, out),
        CanonicalAction::Lt { layer, tap } => {
            add(layer, out);
            collect_refs(tap, out, layers);
        }
        CanonicalAction::ModTap { tap, .. } => collect_refs(tap, out, layers),
        _ => {}
    }
}
