//! The internal canonical layout representation.
//!
//! Both [`super::oryx::Layout`] (Oryx mode) and
//! [`super::layout::LayoutFile`] (local mode) deserialize into this type.
//! The rest of the codebase operates on [`CanonicalLayout`] only.

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use super::geometry::GeometryName;
use super::keycode::{Keycode, Modifier};
use super::{layout, oryx};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalLayout {
    pub geometry: GeometryName,
    pub title: String,
    pub layers: Vec<CanonicalLayer>,
    #[serde(default)]
    pub combos: Vec<CanonicalCombo>,
    #[serde(default)]
    pub config: BTreeMap<String, serde_json::Value>,
}

/// A combo as it lives in the canonical layout. Position names refer to
/// matrix indices on the keyboard's geometry; `sends` is the keycode the
/// combo emits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalCombo {
    pub keys: Vec<String>,
    pub sends: String,
    #[serde(default)]
    pub layer: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalLayer {
    pub name: String,
    pub position: u8,
    pub keys: Vec<CanonicalKey>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CanonicalKey {
    #[serde(default)]
    pub tap: Option<CanonicalAction>,
    #[serde(default)]
    pub hold: Option<CanonicalAction>,
    #[serde(default)]
    pub double_tap: Option<CanonicalAction>,
    #[serde(default)]
    pub tap_hold: Option<CanonicalAction>,
    #[serde(default)]
    pub tapping_term: Option<u32>,
    #[serde(default)]
    pub custom_label: Option<String>,
}

/// Highest valid `n` for a `CanonicalAction::Custom(n)` slot.
///
/// QMK declares `USER00..USER31` (32 slots) in `keycodes.h`. Anything
/// above this is not a valid QMK identifier and would either be a
/// silent codegen drop (old behavior, fixed) or an undefined-symbol
/// link error from `arm-none-eabi-ld` (current behavior without the
/// bound). Both parsers (`oryx_action_to_canonical` and
/// `schema::layout::parse_action`) reject out-of-range values at
/// parse time so the lint can surface them as `unknown-keycode`
/// instead of failing at codegen.
pub const MAX_USER_KEYCODE_SLOT: u32 = 31;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum CanonicalAction {
    /// Plain keycode.
    Keycode(Keycode),
    /// `MO(layer)` — momentary.
    Mo { layer: LayerRef },
    /// `TG(layer)` — toggle.
    Tg { layer: LayerRef },
    /// `TO(layer)`.
    To { layer: LayerRef },
    /// `TT(layer)`.
    Tt { layer: LayerRef },
    /// `DF(layer)`.
    Df { layer: LayerRef },
    /// `LT(layer, tap)` — layer-tap.
    Lt {
        layer: LayerRef,
        tap: Box<CanonicalAction>,
    },
    /// Mod-tap, e.g. `LCTL_T(KC_A)`.
    ModTap {
        mod_: Modifier,
        tap: Box<CanonicalAction>,
    },
    /// Modifier-wrapped keycode, e.g. `LCTL(LSFT(KC_TAB))`. Used for
    /// the Oryx UI's "send X with Ctrl+Shift held" feature, which
    /// arrives in the GraphQL response as a regular `tap` action with
    /// a non-null `modifiers` field listing which modifiers should
    /// wrap the base keycode. Codegen renders this as nested QMK
    /// modifier macros so the firmware emits the right keystroke.
    Modified {
        mods: Vec<Modifier>,
        base: Box<CanonicalAction>,
    },
    /// Plain modifier.
    Modifier(Modifier),
    /// `USERnn` custom keycode.
    Custom(u8),
    /// `KC_TRNS`. (Also representable as `Keycode(KcTransparent)`.)
    Transparent,
    /// `KC_NO`.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LayerRef {
    Name(String),
    Index(u8),
}

impl LayerRef {
    pub fn as_name(&self) -> Option<&str> {
        match self {
            LayerRef::Name(n) => Some(n.as_str()),
            LayerRef::Index(_) => None,
        }
    }

    pub fn as_index(&self) -> Option<u8> {
        match self {
            LayerRef::Name(_) => None,
            LayerRef::Index(i) => Some(*i),
        }
    }
}

impl CanonicalAction {
    /// A user-friendly representation for rendering and `explain`.
    pub fn display(&self) -> String {
        match self {
            CanonicalAction::Keycode(k) => k.canonical_name().into_owned(),
            CanonicalAction::Mo { layer } => format!("MO({})", render_layer_ref(layer)),
            CanonicalAction::Tg { layer } => format!("TG({})", render_layer_ref(layer)),
            CanonicalAction::To { layer } => format!("TO({})", render_layer_ref(layer)),
            CanonicalAction::Tt { layer } => format!("TT({})", render_layer_ref(layer)),
            CanonicalAction::Df { layer } => format!("DF({})", render_layer_ref(layer)),
            CanonicalAction::Lt { layer, tap } => {
                format!("LT({}, {})", render_layer_ref(layer), tap.display())
            }
            CanonicalAction::ModTap { mod_, tap } => {
                format!("{}_T({})", mod_.canonical_name(), tap.display())
            }
            CanonicalAction::Modified { mods, base } => render_mod_wrappers(mods, &base.display()),
            CanonicalAction::Modifier(m) => m.canonical_name().to_string(),
            CanonicalAction::Custom(n) => format!("USER{:02}", n),
            CanonicalAction::Transparent => "KC_TRNS".into(),
            CanonicalAction::None => "KC_NO".into(),
        }
    }

    /// Return the underlying keycode (for hold/LT actions, returns the `tap`).
    pub fn tap_keycode(&self) -> Option<&Keycode> {
        match self {
            CanonicalAction::Keycode(k) => Some(k),
            CanonicalAction::Lt { tap, .. } | CanonicalAction::ModTap { tap, .. } => {
                tap.tap_keycode()
            }
            CanonicalAction::Modified { base, .. } => base.tap_keycode(),
            _ => None,
        }
    }

    pub fn layer_ref(&self) -> Option<&LayerRef> {
        match self {
            CanonicalAction::Mo { layer }
            | CanonicalAction::Tg { layer }
            | CanonicalAction::To { layer }
            | CanonicalAction::Tt { layer }
            | CanonicalAction::Df { layer }
            | CanonicalAction::Lt { layer, .. } => Some(layer),
            _ => None,
        }
    }
}

/// Wrap an inner-action display string in nested QMK modifier macros:
/// `mods=[Lctl, Lsft], inner="KC_TAB"` → `"LCTL(LSFT(KC_TAB))"`. Empty
/// modifier list returns the inner unchanged.
fn render_mod_wrappers(mods: &[Modifier], inner: &str) -> String {
    let mut out = inner.to_string();
    for m in mods.iter().rev() {
        out = format!("{}({})", m.canonical_name(), out);
    }
    out
}

fn render_layer_ref(r: &LayerRef) -> String {
    match r {
        LayerRef::Name(n) => n.clone(),
        LayerRef::Index(i) => i.to_string(),
    }
}

impl CanonicalKey {
    /// Compact display: "<tap>[/<hold>]" or the single action.
    pub fn display(&self) -> String {
        match (&self.tap, &self.hold) {
            (Some(CanonicalAction::Lt { layer, tap }), _) => {
                format!("LT({}, {})", render_layer_ref(layer), tap.display())
            }
            (Some(t), Some(h)) => format!("{}/{}", t.display(), h.display()),
            (Some(t), None) => t.display(),
            (None, Some(h)) => format!("/{}", h.display()),
            (None, None) => "KC_NO".into(),
        }
    }

    /// True if any of the tap/hold/double_tap/tap_hold actions reference
    /// the named keycode (case-insensitive). Used by `find` and the lint rules.
    ///
    /// Accepts both `KC_BSPC` and the bare `BSPC` form, and matches both
    /// `Keycode(k)` and `Modifier(m)` representations — for `KC_LCTL`
    /// the canonical converter normalizes to `Modifier(Lctl)`, so the
    /// search must be aware of both shapes.
    pub fn references_keycode(&self, name: &str) -> bool {
        let upper = name.trim().to_ascii_uppercase();
        let with_prefix = if upper.starts_with("KC_") {
            upper.clone()
        } else {
            format!("KC_{upper}")
        };
        let matches = |a: &Option<CanonicalAction>| {
            a.as_ref()
                .map(|a| {
                    action_matches_keycode_name(a, &upper)
                        || action_matches_keycode_name(a, &with_prefix)
                })
                .unwrap_or(false)
        };
        matches(&self.tap)
            || matches(&self.hold)
            || matches(&self.double_tap)
            || matches(&self.tap_hold)
    }
}

/// Recursive matcher: handles `Keycode`, `Modifier`, and the wrapper
/// variants (`Lt`, `ModTap`, `Modified`).
fn action_matches_keycode_name(action: &CanonicalAction, want: &str) -> bool {
    match action {
        CanonicalAction::Keycode(k) => k.canonical_name().eq_ignore_ascii_case(want),
        CanonicalAction::Modifier(m) => {
            let qmk = format!("KC_{}", m.canonical_name());
            qmk.eq_ignore_ascii_case(want) || m.canonical_name().eq_ignore_ascii_case(want)
        }
        CanonicalAction::Lt { tap, .. } | CanonicalAction::ModTap { tap, .. } => {
            action_matches_keycode_name(tap, want)
        }
        CanonicalAction::Modified { base, .. } => action_matches_keycode_name(base, want),
        _ => false,
    }
}

impl CanonicalLayout {
    /// Convert an Oryx GraphQL response into the canonical representation.
    pub fn from_oryx(layout: &oryx::Layout) -> Result<Self> {
        let mut layers = Vec::with_capacity(layout.revision.layers.len());
        for layer in &layout.revision.layers {
            layers.push(CanonicalLayer {
                name: layer.title.clone(),
                position: layer.position,
                keys: layer.keys.iter().map(oryx_key_to_canonical).collect(),
            });
        }
        // Second pass: resolve numeric `layer` indices to names where possible.
        let index_to_name: Vec<(u8, String)> = layers
            .iter()
            .map(|l| (l.position, l.name.clone()))
            .collect();
        for layer in &mut layers {
            for key in &mut layer.keys {
                for action in [
                    &mut key.tap,
                    &mut key.hold,
                    &mut key.double_tap,
                    &mut key.tap_hold,
                ]
                .into_iter()
                .flatten()
                {
                    resolve_layer_refs(action, &index_to_name);
                }
            }
        }
        // Carry Oryx UI combos through canonicalization. The 2026-Q2
        // Oryx schema models combos as `Combo { keyIndices, layerIdx,
        // trigger, ... }`, all of which need geometry and layer context
        // to be projected into the canonical (position-name + layer-name)
        // shape. We resolve those here so the combo translator stays a
        // pure function of (combo, geometry, layer index→name table).
        let geometry = super::geometry::get(&layout.geometry).ok_or_else(|| {
            anyhow!(
                "unknown geometry '{}' returned by Oryx for layout {}",
                layout.geometry,
                layout.hash_id
            )
        })?;
        let combos = match &layout.revision.combos {
            None => Vec::new(),
            Some(list) => list
                .iter()
                .map(|c| oryx_combo_to_canonical(c, geometry, &index_to_name))
                .collect::<Result<Vec<_>>>()?,
        };

        Ok(CanonicalLayout {
            geometry: GeometryName::from_str(&layout.geometry),
            title: layout.title.clone(),
            layers,
            combos,
            config: layout
                .revision
                .config
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        })
    }

    /// Convert a local-mode layout.toml into the canonical representation.
    pub fn from_local(layout: &layout::LayoutFile) -> Result<Self> {
        let geom_name = layout.meta.geometry.as_str();
        let geom = super::geometry::get(geom_name)
            .ok_or_else(|| anyhow!("unknown geometry '{geom_name}' in layout.toml [meta]"))?;
        let key_count = geom.matrix_key_count();

        // Build a name → layer map so we can resolve `inherit = "Main"`
        // by copying the base layer's keys into the overlay layer's
        // unspecified positions as KC_TRNS (transparent).
        let by_name: BTreeMap<&str, &super::layout::LayerEntry> =
            layout.layers.iter().map(|l| (l.name.as_str(), l)).collect();

        let mut out_layers = Vec::with_capacity(layout.layers.len());
        for layer in &layout.layers {
            // Default fill: KC_NO unless inherit is set, in which case KC_TRNS.
            let mut keys: Vec<CanonicalKey> = if layer.inherit.is_some() {
                vec![
                    CanonicalKey {
                        tap: Some(CanonicalAction::Transparent),
                        ..Default::default()
                    };
                    key_count
                ]
            } else {
                vec![CanonicalKey::default(); key_count]
            };
            // Sanity-check that the inherit target exists.
            if let Some(parent) = &layer.inherit {
                if !by_name.contains_key(parent.as_str()) {
                    return Err(anyhow!(
                        "layer '{}' inherits from unknown layer '{parent}'",
                        layer.name
                    ));
                }
            }
            for (pos, action) in &layer.keys {
                let idx = geom.position_to_index(pos).ok_or_else(|| {
                    anyhow!(
                        "unknown position '{pos}' in layer '{}' for geometry '{geom_name}'",
                        layer.name
                    )
                })?;
                keys[idx] = action.to_canonical_key();
            }
            out_layers.push(CanonicalLayer {
                name: layer.name.clone(),
                position: layer.position,
                keys,
            });
        }
        Ok(CanonicalLayout {
            geometry: GeometryName::from_str(&layout.meta.geometry),
            title: layout.meta.title.clone(),
            layers: out_layers,
            combos: Vec::new(),
            config: BTreeMap::new(),
        })
    }

    pub fn layer_by_name(&self, name: &str) -> Option<&CanonicalLayer> {
        self.layers.iter().find(|l| l.name == name)
    }
}

fn resolve_layer_refs(action: &mut CanonicalAction, idx_to_name: &[(u8, String)]) {
    let resolve = |r: &mut LayerRef| {
        if let LayerRef::Index(i) = *r {
            if let Some((_, name)) = idx_to_name.iter().find(|(p, _)| *p == i) {
                *r = LayerRef::Name(name.clone());
            }
        }
    };
    match action {
        CanonicalAction::Mo { layer }
        | CanonicalAction::Tg { layer }
        | CanonicalAction::To { layer }
        | CanonicalAction::Tt { layer }
        | CanonicalAction::Df { layer } => resolve(layer),
        CanonicalAction::Lt { layer, tap } => {
            resolve(layer);
            resolve_layer_refs(tap, idx_to_name);
        }
        CanonicalAction::ModTap { tap, .. } => resolve_layer_refs(tap, idx_to_name),
        CanonicalAction::Modified { base, .. } => resolve_layer_refs(base, idx_to_name),
        _ => {}
    }
}

fn oryx_key_to_canonical(k: &oryx::Key) -> CanonicalKey {
    let tap = k.tap.as_ref().map(oryx_action_to_canonical);
    let hold = k.hold.as_ref().map(oryx_action_to_canonical);
    let double_tap = k.double_tap.as_ref().map(oryx_action_to_canonical);
    let tap_hold = k.tap_hold.as_ref().map(oryx_action_to_canonical);

    // Normalize tap+hold combinations into a single combinator on `tap`,
    // so the rest of the codebase only ever sees one shape per concept.
    //
    // - tap=KC_X + hold=MO(N)        → tap=LT(N, X), hold=None
    // - tap=KC_X + hold=Modifier(M)  → tap=ModTap{M, X}, hold=None
    // - tap=KC_X + hold=KC_<MOD>     → tap=ModTap{M, X}, hold=None  (Oryx may emit this shape)
    let (tap, hold) = match (tap, hold) {
        (Some(CanonicalAction::Keycode(kc)), Some(CanonicalAction::Mo { layer })) => (
            Some(CanonicalAction::Lt {
                layer,
                tap: Box::new(CanonicalAction::Keycode(kc)),
            }),
            None,
        ),
        (Some(CanonicalAction::Keycode(kc)), Some(CanonicalAction::Modifier(m))) => (
            Some(CanonicalAction::ModTap {
                mod_: m,
                tap: Box::new(CanonicalAction::Keycode(kc)),
            }),
            None,
        ),
        other => other,
    };

    CanonicalKey {
        tap,
        hold,
        double_tap,
        tap_hold,
        tapping_term: k.tapping_term,
        custom_label: k.custom_label.clone(),
    }
}

/// Project an Oryx GraphQL `Combo` into the canonical
/// (position-name + layer-name + emitted-keycode) shape.
///
/// All inputs are required and validated:
///
/// - `keyIndices` is translated to position-name strings via the
///   geometry's `index_to_position` table. An out-of-range index is a
///   loud error rather than a silent drop, because a combo with one of
///   its chord keys missing would change the firmware's behavior in a
///   way the user did not intend.
/// - `layerIdx` is translated to a human-readable layer name by looking
///   up the layer with the matching `position` field in the revision's
///   layer list. An unknown index is a loud error, same reasoning.
/// - `trigger` is re-deserialized as an `oryx::Action` (it has the same
///   JSON shape as a regular key action) and run through
///   `oryx_action_to_canonical`, then rendered to its canonical
///   keycode-string form for `CanonicalCombo.sends`. A trigger that
///   fails to deserialize is a loud error — silently dropping it would
///   produce a combo that fires nothing.
///
/// `timeout_ms` is not yet exposed by the live schema; future schemas
/// can populate it via the `extra` bag without changing this signature.
///
/// **`layerIdx` resolution policy** (semantic ambiguity):
///
/// The 2026-Q2 Oryx schema models `layerIdx` as a non-null `Int!`
/// without documenting whether it's the **0-based array index** into
/// `revision.layers[]` or the **`position` field value** of the
/// referenced layer. A read-only investigation against the public
/// catalog (`z4A0O`, `XgYB9`, `alBEv`, others) found that every
/// public layout has `layers[i].position == i`, making the two
/// interpretations observationally equivalent — but only by
/// convention, not by guarantee.
///
/// To stay correct under either interpretation AND to detect a
/// future schema divergence the moment it appears, we resolve
/// `layerIdx` against **both** views and require they agree. If
/// either succeeds alone, we use it (with a warning if it's the
/// fallback path). If they disagree on the layer name, we error
/// loudly with the offending data so the user can file an issue
/// rather than silently flashing a combo on the wrong layer.
fn oryx_combo_to_canonical(
    c: &oryx::Combo,
    geometry: &dyn super::geometry::Geometry,
    layer_index_to_name: &[(u8, String)],
) -> Result<CanonicalCombo> {
    let keys = c
        .key_indices
        .iter()
        .map(|&idx| {
            geometry
                .index_to_position(idx as usize)
                .map(String::from)
                .ok_or_else(|| {
                    anyhow!(
                        "Oryx combo references key index {idx}, which is out of range \
                         for geometry '{}' (max {})",
                        geometry.id(),
                        geometry.matrix_key_count().saturating_sub(1)
                    )
                })
        })
        .collect::<Result<Vec<_>>>()?;

    let layer = resolve_combo_layer(c.layer_idx, layer_index_to_name)?;

    // The `trigger` JSON has the same shape as a key action: a `code`
    // string with optional `modifier(s)`. Re-deserializing through
    // `oryx::Action` keeps the keycode-translation logic in one place
    // (single source of truth: `oryx_action_to_canonical`).
    let trigger_action: oryx::Action = serde_json::from_value(c.trigger.clone())
        .map_err(|e| anyhow!("Oryx combo `trigger` is not a valid Action JSON: {e}"))?;
    let sends = oryx_action_to_canonical(&trigger_action).display();

    Ok(CanonicalCombo {
        keys,
        sends,
        layer: Some(layer),
        timeout_ms: None,
    })
}

/// Resolve a `Combo.layerIdx` value into a layer name under both
/// candidate semantics, returning an error if they disagree.
///
/// See the doc on [`oryx_combo_to_canonical`] for the semantic
/// ambiguity background. The decision matrix:
///
/// | by_position | by_array_idx | resolution                                     |
/// |-------------|--------------|------------------------------------------------|
/// | Some(p)     | Some(a) p==a | `Ok(p)` — both interpretations agree (typical) |
/// | Some(p)     | Some(a) p!=a | `Err(...)` — schema divergence; refuse to guess |
/// | Some(p)     | None         | `Ok(p)` — only `position` matches              |
/// | None        | Some(a)      | `Ok(a)` + tracing::warn — fallback fired       |
/// | None        | None         | `Err(...)` — combo points at nothing           |
fn resolve_combo_layer(
    layer_idx: u8,
    layer_index_to_name: &[(u8, String)],
) -> Result<String> {
    let by_position = layer_index_to_name
        .iter()
        .find(|(p, _)| *p == layer_idx)
        .map(|(_, n)| n.clone());
    let by_array_idx = layer_index_to_name
        .get(layer_idx as usize)
        .map(|(_, n)| n.clone());

    match (by_position, by_array_idx) {
        (Some(p), Some(a)) if p == a => Ok(p),
        (Some(p), Some(a)) => Err(anyhow!(
            "Oryx combo layerIdx={layer_idx} is ambiguous: the layer at array \
             index {layer_idx} is '{a}' but the layer with position={layer_idx} \
             is '{p}'. This means Oryx has shipped a layout where layers[i].position != i, \
             which oryx-bench has not seen before. Please file a bug with the layout hashId."
        )),
        (Some(p), None) => Ok(p),
        (None, Some(a)) => {
            tracing::warn!(
                layer_idx,
                resolved = %a,
                "Oryx combo layerIdx had no matching `position` field; \
                 falling back to array-index lookup. If this fires, our \
                 layerIdx interpretation may need updating — see the doc \
                 on `resolve_combo_layer` in src/schema/canonical.rs."
            );
            Ok(a)
        }
        (None, None) => Err(anyhow!(
            "Oryx combo references layerIdx={layer_idx}, which matches \
             neither a layer position nor a layer array index in the \
             revision (known positions: {:?}, layer count: {})",
            layer_index_to_name
                .iter()
                .map(|(p, _)| *p)
                .collect::<Vec<_>>(),
            layer_index_to_name.len()
        )),
    }
}

fn oryx_action_to_canonical(a: &oryx::Action) -> CanonicalAction {
    // Dispatch by the `code` field, then optionally wrap the result in
    // a `Modified` if the action carries a non-empty `modifiers`
    // field. The `modifiers` field is Oryx's encoding for "this base
    // keycode should be sent with these modifiers held" — for example,
    // `tap = KC_TAB` with `modifiers.leftCtrl=true, leftShift=true`
    // means "emit LCS(KC_TAB)". Without this wrapping, multi-mod
    // combos silently lose their mods at codegen time.
    let base = base_action_from_oryx(a);
    let mods = parse_oryx_modifiers(a);
    if mods.is_empty() {
        base
    } else {
        CanonicalAction::Modified {
            mods,
            base: Box::new(base),
        }
    }
}

/// Parse Oryx's `Action.modifier` (a single string) and `Action.modifiers`
/// (an object map of leftCtrl/leftShift/etc → bool) into a sorted,
/// deduplicated list of [`Modifier`]s. Returns an empty vec when
/// neither field is set.
fn parse_oryx_modifiers(a: &oryx::Action) -> Vec<Modifier> {
    use std::collections::BTreeSet;
    let mut set: BTreeSet<&'static str> = BTreeSet::new();

    if let Some(s) = &a.modifier {
        if let Some(m) = oryx_mod_token_to_label(s) {
            set.insert(m);
        }
    }
    if let Some(value) = &a.modifiers {
        if let Some(obj) = value.as_object() {
            for (key, on) in obj {
                if on.as_bool() != Some(true) {
                    continue;
                }
                if let Some(m) = oryx_mod_field_to_label(key) {
                    set.insert(m);
                }
            }
        } else if let Some(arr) = value.as_array() {
            // Older Oryx releases sent modifiers as ["LCTL", "LSFT"].
            for entry in arr {
                if let Some(s) = entry.as_str() {
                    if let Some(m) = oryx_mod_token_to_label(s) {
                        set.insert(m);
                    }
                }
            }
        }
    }

    set.into_iter().filter_map(Modifier::from_str).collect()
}

/// Map Oryx's `modifiers` object key (`leftCtrl`, `rightShift`, …) to
/// the corresponding QMK modifier token.
fn oryx_mod_field_to_label(key: &str) -> Option<&'static str> {
    match key {
        "leftCtrl" => Some("LCTL"),
        "leftShift" => Some("LSFT"),
        "leftAlt" => Some("LALT"),
        "leftGui" => Some("LGUI"),
        "rightCtrl" => Some("RCTL"),
        "rightShift" => Some("RSFT"),
        "rightAlt" => Some("RALT"),
        "rightGui" => Some("RGUI"),
        _ => None,
    }
}

/// Map Oryx's `modifier` string field (`"LCTL"`, `"LSFT"`, …) to the
/// corresponding QMK modifier token. Tolerates the long forms
/// (`"left_ctrl"`, etc.) too.
fn oryx_mod_token_to_label(s: &str) -> Option<&'static str> {
    let upper = s.to_ascii_uppercase();
    match upper.as_str() {
        "LCTL" | "LCTRL" | "LEFT_CTRL" => Some("LCTL"),
        "LSFT" | "LSHIFT" | "LEFT_SHIFT" => Some("LSFT"),
        "LALT" | "LEFT_ALT" => Some("LALT"),
        "LGUI" | "LEFT_GUI" => Some("LGUI"),
        "RCTL" | "RCTRL" | "RIGHT_CTRL" => Some("RCTL"),
        "RSFT" | "RSHIFT" | "RIGHT_SHIFT" => Some("RSFT"),
        "RALT" | "RIGHT_ALT" => Some("RALT"),
        "RGUI" | "RIGHT_GUI" => Some("RGUI"),
        _ => None,
    }
}

/// Translate just the `code` field of an Oryx action into a base
/// `CanonicalAction`, ignoring the modifiers wrapper. The caller is
/// responsible for wrapping in [`CanonicalAction::Modified`] if the
/// action's `modifier`/`modifiers` fields are non-empty.
fn base_action_from_oryx(a: &oryx::Action) -> CanonicalAction {
    match a.code.as_str() {
        "KC_NO" => CanonicalAction::None,
        "KC_TRANSPARENT" | "KC_TRNS" => CanonicalAction::Transparent,
        "MO" => CanonicalAction::Mo {
            layer: LayerRef::Index(a.layer.unwrap_or(0)),
        },
        "TG" => CanonicalAction::Tg {
            layer: LayerRef::Index(a.layer.unwrap_or(0)),
        },
        "TO" => CanonicalAction::To {
            layer: LayerRef::Index(a.layer.unwrap_or(0)),
        },
        "TT" => CanonicalAction::Tt {
            layer: LayerRef::Index(a.layer.unwrap_or(0)),
        },
        "DF" => CanonicalAction::Df {
            layer: LayerRef::Index(a.layer.unwrap_or(0)),
        },
        // USERnn custom keycode slots. QMK declares USER00..USER31
        // (32 slots) in `keycodes.h`; anything beyond that is not a
        // valid QMK identifier and would produce an undefined-symbol
        // link error if we let it through to codegen as a literal.
        // Out-of-range values fall through to `Keycode::Other` so the
        // unknown-keycode lint surfaces them at the user's first lint
        // run instead of a silent codegen drop or a confusing QMK
        // build failure.
        code if code.starts_with("USER") => {
            if let Ok(n) = code.trim_start_matches("USER").parse::<u8>() {
                if (n as u32) <= MAX_USER_KEYCODE_SLOT {
                    return CanonicalAction::Custom(n);
                }
            }
            CanonicalAction::Keycode(Keycode::from_str(code))
        }
        code if code.ends_with("_T") && code.contains('_') => {
            let prefix = code.trim_end_matches("_T");
            if let Some(m) = Modifier::from_str(prefix) {
                return CanonicalAction::ModTap {
                    mod_: m,
                    tap: Box::new(CanonicalAction::None),
                };
            }
            CanonicalAction::Keycode(Keycode::from_str(code))
        }
        code => {
            // Try modifier first (e.g. "KC_LCTL"), then fall back to keycode.
            if let Some(m) = Modifier::from_str(code.trim_start_matches("KC_")) {
                CanonicalAction::Modifier(m)
            } else {
                CanonicalAction::Keycode(Keycode::from_str(code))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_fixture_to_canonical() {
        let raw = include_str!("../../examples/voyager-dvorak/pulled/revision.json");
        let oryx_layout: oryx::Layout = serde_json::from_str(raw).unwrap();
        let canonical = CanonicalLayout::from_oryx(&oryx_layout).unwrap();
        assert_eq!(canonical.layers.len(), 4);
        assert!(canonical.layers.iter().any(|l| l.name == "Main"));
    }

    #[test]
    fn fixture_contains_lt_on_bspc() {
        // The fixture is the canonical "LT-on-high-frequency-key" demo;
        // we should see at least one LT with BSPC or DEL in the tap slot.
        let raw = include_str!("../../examples/voyager-dvorak/pulled/revision.json");
        let oryx_layout: oryx::Layout = serde_json::from_str(raw).unwrap();
        let canonical = CanonicalLayout::from_oryx(&oryx_layout).unwrap();
        let mut found = false;
        for layer in &canonical.layers {
            for key in &layer.keys {
                if let Some(CanonicalAction::Lt { tap, .. }) = &key.tap {
                    if let Some(kc) = tap.tap_keycode() {
                        if kc.is_high_frequency() {
                            found = true;
                        }
                    }
                }
            }
        }
        assert!(
            found,
            "fixture should contain at least one LT on a high-freq key"
        );
    }

    #[test]
    fn mod_tap_collapse_tap_keycode_plus_hold_modifier() {
        use std::collections::HashMap;
        // Synthesize an Oryx Key with tap=KC_A, hold=KC_LCTL — this is the
        // shape Oryx emits for `LCTL_T(KC_A)`. We assert the canonical pass
        // collapses it into ModTap{Lctl, KcA}.
        let key = oryx::Key {
            tap: Some(oryx::Action {
                code: "KC_A".into(),
                layer: None,
                modifier: None,
                modifiers: None,
                macro_: None,
                extra: HashMap::new(),
            }),
            hold: Some(oryx::Action {
                code: "KC_LCTL".into(),
                layer: None,
                modifier: None,
                modifiers: None,
                macro_: None,
                extra: HashMap::new(),
            }),
            double_tap: None,
            tap_hold: None,
            tapping_term: None,
            custom_label: None,
            icon: None,
            emoji: None,
            glow_color: None,
            extra: HashMap::new(),
        };
        let canonical = oryx_key_to_canonical(&key);
        match &canonical.tap {
            Some(CanonicalAction::ModTap { mod_, tap }) => {
                assert_eq!(mod_, &Modifier::Lctl);
                assert_eq!(tap.display(), "KC_A");
            }
            other => panic!("expected ModTap, got {other:?}"),
        }
        assert!(canonical.hold.is_none(), "hold should be collapsed");
    }

    #[test]
    fn user_keycode_parses_to_custom_in_oryx_action() {
        use std::collections::HashMap;
        let action = oryx::Action {
            code: "USER03".into(),
            layer: None,
            modifier: None,
            modifiers: None,
            macro_: None,
            extra: HashMap::new(),
        };
        let canonical = oryx_action_to_canonical(&action);
        match canonical {
            CanonicalAction::Custom(n) => assert_eq!(n, 3),
            other => panic!("expected Custom(3), got {other:?}"),
        }
    }

    #[test]
    fn user_keycode_at_max_slot_parses_to_custom() {
        use std::collections::HashMap;
        let action = oryx::Action {
            code: "USER31".into(),
            layer: None,
            modifier: None,
            modifiers: None,
            macro_: None,
            extra: HashMap::new(),
        };
        match oryx_action_to_canonical(&action) {
            CanonicalAction::Custom(n) => assert_eq!(n, 31),
            other => panic!("expected Custom(31), got {other:?}"),
        }
    }

    #[test]
    fn user_keycode_above_max_slot_falls_back_to_other() {
        // QMK only has USER00..USER31; out-of-range parses through to
        // Keycode::Other so the unknown-keycode lint surfaces it
        // instead of letting codegen produce an undeclared identifier.
        use std::collections::HashMap;
        let action = oryx::Action {
            code: "USER42".into(),
            layer: None,
            modifier: None,
            modifiers: None,
            macro_: None,
            extra: HashMap::new(),
        };
        match oryx_action_to_canonical(&action) {
            CanonicalAction::Keycode(crate::schema::keycode::Keycode::Other(s)) => {
                assert_eq!(s, "USER42");
            }
            other => panic!("expected Keycode::Other(USER42), got {other:?}"),
        }
    }
}
