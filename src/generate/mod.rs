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

use crate::schema::canonical::CanonicalLayout;
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

    let keymap_c = keymap::emit_keymap_c(layout, geom, &layer_table, &custom_keycodes)?;
    let features_c = features::emit_features_c(features, &layer_table, &custom_keycodes, layout)?;
    let features_h = features::emit_features_h(&custom_keycodes);
    let config_h = config_h::emit_config_h(features)?;
    let rules_mk = rules_mk::emit_rules_mk(features, overlay_dir)?;

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
    let mut t = BTreeMap::new();
    for layer in &layout.layers {
        let ident = sanitize_c_ident(&layer.name);
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
}
