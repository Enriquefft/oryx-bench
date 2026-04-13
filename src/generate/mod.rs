//! Codegen layer.
//!
//! Translates [`CanonicalLayout`] + [`FeaturesToml`] + the contents of
//! `overlay/` into the C source files QMK consumes:
//!
//! - `keymap.c`        — `LAYOUT_<board>(...)` arrays + `enum layers`
//! - `_features.c`     — Tier 1 declarative feature bodies (achordion, key overrides, combos, macros) + `process_record_user` dispatch
//! - `_features.h`     — declarations shared between `keymap.c` and `_features.c` (custom-keycode enum, etc.)
//! - `config.h`        — `[config]` defines from features.toml
//! - `rules.mk`        — feature flags + `SRC +=` entries for `overlay/*.{c,zig}`
//!
//! Each emitter is a pure function from inputs to a `String`. The
//! orchestrator [`generate_all`] composes them and returns a [`Generated`]
//! bundle the build backend writes to disk.
//!
//! **Single source of truth invariant**: every file the build pipeline
//! stages into the keymap directory is owned by this module. The build
//! backend never invents headers — that way both translation units
//! reference the same set of generator-emitted symbols.

pub mod config_h;
pub mod features;
pub mod keymap;
pub mod rules_mk;

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;

use crate::schema::canonical::{CanonicalAction, CanonicalLayout};
use crate::schema::features::FeaturesToml;
use crate::schema::geometry::Geometry;
use crate::schema::naming::sanitize_c_ident;

/// All generated source files for one build.
#[derive(Debug, Clone)]
pub struct Generated {
    pub keymap_c: String,
    pub features_c: String,
    pub features_h: String,
    pub config_h: String,
    pub rules_mk: String,
}

/// Translate the canonical layout + features + overlay into C source files.
///
/// `overlay_dir` is walked to discover Tier 2 (`*.zig`) and Tier 2′ (`*.c`)
/// files for `SRC +=` entries in `rules.mk`. It is `None` when there is no
/// overlay/ directory yet (e.g. fresh init projects).
pub fn generate_all(
    layout: &CanonicalLayout,
    features: &FeaturesToml,
    geom: &dyn Geometry,
    overlay_dir: Option<&Path>,
) -> Result<Generated> {
    let layer_table = build_layer_table(layout);
    let custom_keycodes = build_custom_keycode_table(features);
    let tap_dances = build_tap_dance_table(layout)?;

    // Guard: if the layout uses double_tap keys but the user explicitly
    // opted out of tap_dance in features, the generated firmware will be
    // broken (TD() macros emitted but TAP_DANCE_ENABLE = no).
    if !tap_dances.is_empty() {
        if let Some(false) = features.features.get("tap_dance") {
            anyhow::bail!(
                "layout uses double_tap keys (tap dance) but features \
                 explicitly set tap_dance = false. Remove the tap_dance \
                 setting or set it to true to enable tap dance support."
            );
        }
    }

    let keymap_c =
        keymap::emit_keymap_c(layout, geom, &layer_table, &custom_keycodes, &tap_dances)?;
    let features_c = features::emit_features_c(
        features,
        &layer_table,
        &custom_keycodes,
        layout,
        &tap_dances,
    )?;
    let features_h = features::emit_features_h(&custom_keycodes, &tap_dances);
    let config_h = config_h::emit_config_h(features)?;
    let rules_mk = rules_mk::emit_rules_mk(features, overlay_dir, !tap_dances.is_empty())?;

    Ok(Generated {
        keymap_c,
        features_c,
        features_h,
        config_h,
        rules_mk,
    })
}

/// Sanitized layer-name → (enum_ident, position). Used by the keymap and
/// features emitters to resolve symbolic layer references to C identifiers.
pub type LayerTable = BTreeMap<String, LayerEntry>;

#[derive(Debug, Clone)]
pub struct LayerEntry {
    pub ident: String,
    pub position: u8,
}

fn build_layer_table(layout: &CanonicalLayout) -> LayerTable {
    use std::collections::{BTreeMap, HashSet};

    // Single-pass: assign sanitized idents with a global uniqueness
    // guarantee. When a base ident is already taken, we append an
    // incrementing counter starting from the layer position until we
    // find a free name. This prevents cross-group collisions (e.g.
    // "Layer" disambiguated to "LAYER_1" colliding with a pre-existing
    // "Layer 1" that also sanitizes to "LAYER_1").
    let mut t = BTreeMap::new();
    let mut assigned: HashSet<String> = HashSet::new();

    for layer in &layout.layers {
        let base_ident = sanitize_c_ident(&layer.name);
        let ident = if assigned.contains(&base_ident) {
            let mut candidate = format!("{}_{}", base_ident, layer.position);
            let mut counter = layer.position as usize;
            while assigned.contains(&candidate) {
                counter += 1;
                candidate = format!("{}_{}", base_ident, counter);
            }
            candidate
        } else {
            base_ident
        };
        assigned.insert(ident.clone());
        t.insert(
            layer.name.clone(),
            LayerEntry {
                ident,
                position: layer.position,
            },
        );
    }

    t
}

/// Macro slot name (e.g. "USER01") → CK_<NAME>. Used to emit the
/// `enum custom_keycodes` and the dispatch in `process_record_user`.
pub type CustomKeycodeTable = BTreeMap<String, CustomKeycodeEntry>;

#[derive(Debug, Clone)]
pub struct CustomKeycodeEntry {
    /// "CK_EMAIL" — the symbol the C source uses.
    pub ident: String,
    /// "you@example.com" — the SEND_STRING body.
    pub body: String,
}

fn build_custom_keycode_table(features: &FeaturesToml) -> CustomKeycodeTable {
    let mut t = BTreeMap::new();
    for m in &features.macros {
        let slot = m.slot.clone().unwrap_or_else(|| m.name.clone());
        t.insert(
            slot,
            CustomKeycodeEntry {
                ident: m.name.clone(),
                body: m.sends.clone(),
            },
        );
    }
    t
}

/// Maps each key that uses `double_tap` to a QMK tap-dance index.
///
/// QMK tap dances are global resources: they live in a separate
/// `tap_dance_actions[]` array, referenced by `TD(n)` index. Each key
/// that uses a tap dance needs a unique index. The table centralizes
/// index assignment so that `emit_keymaps_array` can emit `TD(n)` in
/// the keymap and `emit_features_c` can emit the corresponding
/// `ACTION_TAP_DANCE_DOUBLE(kc1, kc2)` entry.
pub type TapDanceTable = Vec<TapDanceEntry>;

#[derive(Debug, Clone)]
pub struct TapDanceEntry {
    /// 0-based index into `tap_dance_actions[]` in the generated C.
    pub td_index: usize,
    /// Layer position (`CanonicalLayer::position`) where this tap dance lives.
    pub layer_position: u8,
    /// Key index within the layer's `keys[]` array.
    pub key_index: usize,
    /// Single-tap action (first arg to `ACTION_TAP_DANCE_DOUBLE`).
    /// `None` means `KC_NO`.
    pub single_tap: Option<CanonicalAction>,
    /// Double-tap action (second arg to `ACTION_TAP_DANCE_DOUBLE`).
    pub double_tap: CanonicalAction,
}

/// Scan the layout for keys with `double_tap` set and build the
/// tap-dance table. Errors on unsupported combinations:
///
/// - `hold + double_tap` (no tap): needs `ACTION_TAP_DANCE_FN_ADVANCED`
/// - `tap + hold + double_tap`: three-way conflict
/// - `tap_hold` present: no QMK equivalent
fn build_tap_dance_table(layout: &CanonicalLayout) -> anyhow::Result<TapDanceTable> {
    let mut table = Vec::new();
    let mut td_index: usize = 0;

    for layer in &layout.layers {
        for (key_idx, key) in layer.keys.iter().enumerate() {
            let Some(dt) = &key.double_tap else { continue };

            if key.tap_hold.is_some() {
                anyhow::bail!(
                    "tap_hold is not yet supported at layer '{}' position {} — \
                     no QMK equivalent exists in oryx-bench's current codegen",
                    layer.name,
                    key_idx
                );
            }

            match (&key.tap, &key.hold) {
                // Case 1: double_tap only (no tap, no hold)
                (None, None) => {
                    table.push(TapDanceEntry {
                        td_index,
                        layer_position: layer.position,
                        key_index: key_idx,
                        single_tap: None,
                        double_tap: dt.clone(),
                    });
                    td_index += 1;
                }
                // Case 2: tap + double_tap (no hold)
                (Some(t), None) => {
                    table.push(TapDanceEntry {
                        td_index,
                        layer_position: layer.position,
                        key_index: key_idx,
                        single_tap: Some(t.clone()),
                        double_tap: dt.clone(),
                    });
                    td_index += 1;
                }
                // Case 3: hold + double_tap (no tap) — not yet supported
                (None, Some(_)) => {
                    anyhow::bail!(
                        "hold + double_tap without tap at layer '{}' position {} — \
                         requires ACTION_TAP_DANCE_FN_ADVANCED which is not yet supported",
                        layer.name,
                        key_idx
                    );
                }
                // Case 4: tap + hold + double_tap — not yet supported
                (Some(_), Some(_)) => {
                    anyhow::bail!(
                        "tap + hold + double_tap at layer '{}' position {} — \
                         three-way tap dance is not yet supported",
                        layer.name,
                        key_idx
                    );
                }
            }
        }
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::canonical::CanonicalLayer;
    use crate::schema::geometry;

    fn empty_layout() -> CanonicalLayout {
        CanonicalLayout {
            geometry: "voyager".into(),
            title: "test".into(),
            layers: vec![CanonicalLayer {
                name: "Main".into(),
                position: 0,
                keys: vec![Default::default(); 52],
            }],
            combos: Vec::new(),
            config: Default::default(),
        }
    }

    #[test]
    fn generate_all_produces_all_files() {
        let layout = empty_layout();
        let features = FeaturesToml::default();
        let geom = geometry::get("voyager").unwrap();
        let out = generate_all(&layout, &features, geom, None).unwrap();
        assert!(out.keymap_c.contains("LAYOUT_voyager"));
        assert!(out.config_h.contains("#pragma once"));
        assert!(out.rules_mk.contains("# Generated"));
    }

    #[test]
    fn layer_table_uses_sanitized_idents() {
        let mut layout = empty_layout();
        layout.layers.push(CanonicalLayer {
            name: "Sym + Num".into(),
            position: 1,
            keys: vec![Default::default(); 52],
        });
        let table = build_layer_table(&layout);
        assert_eq!(table["Sym + Num"].ident, "SYM_NUM");
    }

    #[test]
    fn layer_table_disambiguates_collisions_by_position() {
        // Two layers whose names both sanitize to "LAYER".
        let layout = CanonicalLayout {
            geometry: "voyager".into(),
            title: "test".into(),
            layers: vec![
                CanonicalLayer {
                    name: "Main".into(),
                    position: 0,
                    keys: vec![Default::default(); 52],
                },
                CanonicalLayer {
                    name: "Layer".into(),
                    position: 1,
                    keys: vec![Default::default(); 52],
                },
                CanonicalLayer {
                    name: "Layer ".into(), // trailing space: also sanitizes to "LAYER"
                    position: 2,
                    keys: vec![Default::default(); 52],
                },
                CanonicalLayer {
                    name: "Gaming".into(),
                    position: 3,
                    keys: vec![Default::default(); 52],
                },
            ],
            combos: Vec::new(),
            config: Default::default(),
        };
        let table = build_layer_table(&layout);

        // Non-colliding layers keep their original sanitized ident.
        assert_eq!(table["Main"].ident, "MAIN");
        assert_eq!(table["Gaming"].ident, "GAMING");

        // First occurrence keeps the base name; subsequent ones get
        // _{counter} appended (starting from layer position, incrementing
        // until a free ident is found).
        assert_eq!(table["Layer"].ident, "LAYER");
        assert_eq!(table["Layer "].ident, "LAYER_2");
    }

    #[test]
    fn layer_table_no_disambiguation_when_no_collision() {
        // Normal layout with unique sanitized idents — no suffixes added.
        let mut layout = empty_layout();
        layout.layers.push(CanonicalLayer {
            name: "Gaming".into(),
            position: 1,
            keys: vec![Default::default(); 52],
        });
        let table = build_layer_table(&layout);
        assert_eq!(table["Main"].ident, "MAIN");
        assert_eq!(table["Gaming"].ident, "GAMING");
    }

    // ────────────────────────────────────────────────────────────────
    // build_tap_dance_table — covers supported and unsupported cases.
    // ────────────────────────────────────────────────────────────────

    fn layout_with_double_tap_key(
        tap: Option<CanonicalAction>,
        hold: Option<CanonicalAction>,
        double_tap: Option<CanonicalAction>,
    ) -> CanonicalLayout {
        use crate::schema::canonical::CanonicalKey;
        let mut keys = vec![CanonicalKey::default(); 52];
        keys[10] = CanonicalKey {
            tap,
            hold,
            double_tap,
            ..Default::default()
        };
        CanonicalLayout {
            geometry: "voyager".into(),
            title: "test".into(),
            layers: vec![CanonicalLayer {
                name: "Main".into(),
                position: 0,
                keys,
            }],
            combos: Vec::new(),
            config: Default::default(),
        }
    }

    #[test]
    fn build_tap_dance_table_double_tap_only() {
        use crate::schema::canonical::{CanonicalAction, LayerRef};
        let layout = layout_with_double_tap_key(
            None,
            None,
            Some(CanonicalAction::To {
                layer: LayerRef::Name("Main".into()),
            }),
        );
        let table = build_tap_dance_table(&layout).unwrap();
        assert_eq!(table.len(), 1);
        assert_eq!(table[0].td_index, 0);
        assert_eq!(table[0].layer_position, 0);
        assert_eq!(table[0].key_index, 10);
        assert!(table[0].single_tap.is_none());
    }

    #[test]
    fn build_tap_dance_table_tap_plus_double_tap() {
        use crate::schema::canonical::{CanonicalAction, LayerRef};
        use crate::schema::keycode::Keycode;
        let layout = layout_with_double_tap_key(
            Some(CanonicalAction::Keycode(Keycode::KcA)),
            None,
            Some(CanonicalAction::To {
                layer: LayerRef::Name("Main".into()),
            }),
        );
        let table = build_tap_dance_table(&layout).unwrap();
        assert_eq!(table.len(), 1);
        assert!(table[0].single_tap.is_some());
    }

    #[test]
    fn build_tap_dance_table_empty_when_no_double_tap() {
        let layout = empty_layout();
        let table = build_tap_dance_table(&layout).unwrap();
        assert!(table.is_empty());
    }

    #[test]
    fn build_tap_dance_table_hold_plus_double_tap_errors() {
        use crate::schema::canonical::{CanonicalAction, LayerRef};
        use crate::schema::keycode::Modifier;
        let layout = layout_with_double_tap_key(
            None,
            Some(CanonicalAction::Modifier(Modifier::Lsft)),
            Some(CanonicalAction::To {
                layer: LayerRef::Name("Main".into()),
            }),
        );
        let err = build_tap_dance_table(&layout).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("hold + double_tap") || msg.contains("not yet supported"),
            "unexpected: {msg}"
        );
    }

    #[test]
    fn build_tap_dance_table_tap_hold_plus_double_tap_errors() {
        use crate::schema::canonical::{CanonicalAction, LayerRef};
        use crate::schema::keycode::{Keycode, Modifier};
        let layout = layout_with_double_tap_key(
            Some(CanonicalAction::Keycode(Keycode::KcA)),
            Some(CanonicalAction::Modifier(Modifier::Lsft)),
            Some(CanonicalAction::To {
                layer: LayerRef::Name("Main".into()),
            }),
        );
        let err = build_tap_dance_table(&layout).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("three-way") || msg.contains("not yet supported"),
            "unexpected: {msg}"
        );
    }

    #[test]
    fn build_tap_dance_table_tap_hold_field_errors() {
        // tap_hold without double_tap is silently ignored — no TD entry.
        use crate::schema::canonical::{CanonicalAction, CanonicalKey, LayerRef};
        let mut keys = vec![CanonicalKey::default(); 52];
        keys[10] = CanonicalKey {
            tap_hold: Some(CanonicalAction::To {
                layer: LayerRef::Name("Main".into()),
            }),
            ..Default::default()
        };
        let layout = CanonicalLayout {
            geometry: "voyager".into(),
            title: "test".into(),
            layers: vec![CanonicalLayer {
                name: "Main".into(),
                position: 0,
                keys,
            }],
            combos: Vec::new(),
            config: Default::default(),
        };
        let table = build_tap_dance_table(&layout).unwrap();
        assert!(
            table.is_empty(),
            "tap_hold without double_tap should not create TD entry"
        );
    }

    #[test]
    fn build_tap_dance_table_double_tap_plus_tap_hold_errors() {
        // double_tap + tap_hold together should error.
        use crate::schema::canonical::{CanonicalAction, CanonicalKey, LayerRef};
        let mut keys = vec![CanonicalKey::default(); 52];
        keys[10] = CanonicalKey {
            double_tap: Some(CanonicalAction::To {
                layer: LayerRef::Name("Main".into()),
            }),
            tap_hold: Some(CanonicalAction::Keycode(
                crate::schema::keycode::Keycode::KcA,
            )),
            ..Default::default()
        };
        let layout = CanonicalLayout {
            geometry: "voyager".into(),
            title: "test".into(),
            layers: vec![CanonicalLayer {
                name: "Main".into(),
                position: 0,
                keys,
            }],
            combos: Vec::new(),
            config: Default::default(),
        };
        let err = build_tap_dance_table(&layout).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("tap_hold"), "expected tap_hold error: {msg}");
    }

    #[test]
    fn double_tap_key_emits_td_in_keymap() {
        use crate::schema::canonical::{CanonicalAction, CanonicalKey, CanonicalLayer, LayerRef};

        let mut keys = vec![CanonicalKey::default(); 52];
        keys[10] = CanonicalKey {
            double_tap: Some(CanonicalAction::To {
                layer: LayerRef::Name("Main".into()),
            }),
            ..Default::default()
        };
        let layout = CanonicalLayout {
            geometry: "voyager".into(),
            title: "test".into(),
            layers: vec![CanonicalLayer {
                name: "Main".into(),
                position: 0,
                keys,
            }],
            combos: Vec::new(),
            config: Default::default(),
        };
        let geom = geometry::get("voyager").unwrap();
        let gen = generate_all(&layout, &FeaturesToml::default(), geom, None).unwrap();
        assert!(
            gen.keymap_c.contains("TD(0)"),
            "keymap should contain TD(0) for double_tap key"
        );
        assert!(
            gen.features_c
                .contains("tap_dance_action_t tap_dance_actions[]"),
            "features.c should contain tap_dance_actions array"
        );
        assert!(
            gen.features_c
                .contains("ACTION_TAP_DANCE_DOUBLE(KC_NO, TO(MAIN))"),
            "features.c should contain the tap dance entry, got:\n{}",
            gen.features_c
        );
        assert!(
            gen.features_h.contains("enum tap_dance_ids"),
            "features.h should contain tap_dance_ids enum"
        );
        assert!(
            gen.rules_mk.contains("TAP_DANCE_ENABLE = yes"),
            "rules.mk should auto-enable tap dance"
        );
    }
}
