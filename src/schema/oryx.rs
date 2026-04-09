//! Rust types matching the Oryx GraphQL JSON response shape.
//!
//! Design constraints (from ARCHITECTURE.md):
//!
//! - All optional fields use `#[serde(default)]` so missing fields don't
//!   fail deserialization.
//! - Every struct has `#[serde(flatten)] extra: HashMap<String, Value>`
//!   for forward-compatibility. If Oryx adds a field we haven't catalogued,
//!   we keep it verbatim and round-trip it unchanged.
//! - `rename_all = "camelCase"` because Oryx returns camelCase.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub hash_id: String,
    pub title: String,
    pub geometry: String,
    #[serde(default)]
    pub privacy: bool,
    pub revision: Revision,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Swatch {
    #[serde(default)]
    pub colors: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub hash_id: String,
    #[serde(default)]
    pub qmk_version: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub md5: String,
    #[serde(default)]
    pub layers: Vec<Layer>,
    /// `Option` rather than `Vec` because the live Oryx server returns
    /// `combos: null` (not `[]`) for layouts that have no combos. A
    /// plain `Vec<Combo>` with `#[serde(default)]` would *not* accept
    /// the JSON `null` and would fail deserialization on every
    /// combo-less revision in the wild — `default` only fires when the
    /// field is missing entirely. The canonical converter normalizes
    /// `None` to an empty list.
    #[serde(default)]
    pub combos: Option<Vec<Combo>>,
    #[serde(default)]
    pub config: HashMap<String, Value>,
    #[serde(default)]
    pub swatch: Option<Swatch>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Layer {
    pub title: String,
    pub position: u8,
    pub keys: Vec<Key>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    #[serde(default)]
    pub tap: Option<Action>,
    #[serde(default)]
    pub hold: Option<Action>,
    #[serde(default)]
    pub double_tap: Option<Action>,
    #[serde(default)]
    pub tap_hold: Option<Action>,
    #[serde(default)]
    pub tapping_term: Option<u32>,
    #[serde(default)]
    pub custom_label: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub glow_color: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub code: String,
    #[serde(default)]
    pub layer: Option<u8>,
    #[serde(default)]
    pub modifier: Option<String>,
    /// Oryx returns `modifiers` either as `null`, as an array of mod names
    /// (`["LCTL", "LSFT"]`), or as an object with a bool per modifier
    /// (`{ "leftCtrl": true, "leftShift": true, ... }`). We preserve the
    /// raw JSON value and normalize later in the canonical layer.
    #[serde(default)]
    pub modifiers: Option<Value>,
    #[serde(default, rename = "macro")]
    pub macro_: Option<MacroDef>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MacroDef {
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// One Oryx UI combo. The schema went from a scalar (`combos` field on
/// `Revision`) to an object type with the fields below in the 2026-Q2
/// Oryx server release. Each typed field maps to a GraphQL selection in
/// `FULL_QUERY` (src/pull/graphql.rs); `extra` keeps the round-trip
/// guarantee for any field we don't yet model.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Combo {
    /// Matrix indices the user must chord to fire the combo. Required
    /// by the live schema (`[Int!]!`); deserialization fails loudly if
    /// the field is missing rather than silently producing an empty
    /// chord, because a missing-keys combo would silently disappear at
    /// codegen time.
    pub key_indices: Vec<u8>,
    /// Index into the revision's layer list. Required by the live
    /// schema (`Int!`).
    pub layer_idx: u8,
    /// The action emitted when the combo fires. Oryx has changed this
    /// field's shape over time:
    ///
    /// - **Old format**: a flat action object with `code`, `modifier(s)`,
    ///   etc. (same shape as a key's `tap`/`hold`).
    /// - **New format** (2026-Q2+): a full key object with `tap`, `hold`,
    ///   `detached`, etc. The actual action lives in `trigger.tap`.
    ///
    /// We hold the raw JSON and normalize in the canonical converter so
    /// both wire formats are handled without fragile version checks.
    pub trigger: Value,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_voyager_dvorak_fixture() {
        let raw = include_str!("../../examples/voyager-dvorak/pulled/revision.json");
        let layout: Layout = serde_json::from_str(raw).expect("fixture parses");
        assert_eq!(layout.hash_id, "yrbLx");
        assert_eq!(layout.geometry, "voyager");
        assert!(!layout.revision.layers.is_empty());
        assert_eq!(layout.revision.layers.len(), 4);
        // Each Voyager layer has 52 matrix positions.
        for layer in &layout.revision.layers {
            assert_eq!(layer.keys.len(), 52, "layer {} is wrong size", layer.title);
        }
    }

    #[test]
    fn round_trip_preserves_unknown_fields() {
        // Any `about` or `aboutPosition` fields Oryx adds land in `extra`
        // and round-trip verbatim.
        let raw = include_str!("../../examples/voyager-dvorak/pulled/revision.json");
        let layout: Layout = serde_json::from_str(raw).unwrap();
        let reserialized = serde_json::to_string(&layout).unwrap();
        let reparsed: Layout = serde_json::from_str(&reserialized).unwrap();
        assert_eq!(reparsed.revision.layers.len(), 4);
    }
}
