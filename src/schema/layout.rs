//! Local-mode visual layout (`layout.toml`) schema.
//!
//! Each layer addresses keys by symbolic position name (e.g. `L_pinky_home`).
//! Unspecified positions default to `KC_NO`, unless the layer has
//! `inherit = "<other-layer>"` in which case they default to `KC_TRNS`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::canonical::{CanonicalAction, CanonicalKey, LayerRef};
use super::keycode::{Keycode, Modifier};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutFile {
    pub meta: Meta,
    #[serde(default, rename = "layers")]
    pub layers: Vec<LayerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub title: String,
    pub geometry: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerEntry {
    pub name: String,
    pub position: u8,
    /// Inherit from another layer; unspecified keys become `KC_TRNS`.
    #[serde(default)]
    pub inherit: Option<String>,
    #[serde(default)]
    pub keys: BTreeMap<String, KeyEntry>,
}

/// A single key binding in `layout.toml`.
///
/// Accepts two forms:
///   `L_pinky_home = "A"`                               (compact tap-only)
///   `L_pinky_home = { tap = "X", hold = "MO(Sym)" }`   (verbose)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeyEntry {
    Compact(String),
    Verbose {
        #[serde(default)]
        tap: Option<String>,
        #[serde(default)]
        hold: Option<String>,
        #[serde(default)]
        double_tap: Option<String>,
        #[serde(default)]
        tap_hold: Option<String>,
        #[serde(default)]
        tapping_term: Option<u32>,
        #[serde(default)]
        custom_label: Option<String>,
    },
}

impl KeyEntry {
    pub fn to_canonical_key(&self) -> CanonicalKey {
        match self {
            KeyEntry::Compact(s) => CanonicalKey {
                tap: Some(parse_action(s)),
                ..Default::default()
            },
            KeyEntry::Verbose {
                tap,
                hold,
                double_tap,
                tap_hold,
                tapping_term,
                custom_label,
            } => CanonicalKey {
                tap: tap.as_deref().map(parse_action),
                hold: hold.as_deref().map(parse_action),
                double_tap: double_tap.as_deref().map(parse_action),
                tap_hold: tap_hold.as_deref().map(parse_action),
                tapping_term: *tapping_term,
                custom_label: custom_label.clone(),
            },
        }
    }
}

/// Parse a binding string like `"A"`, `"KC_BSPC"`, `"MO(SymNum)"`,
/// `"LT(SymNum, BSPC)"`, `"LSFT"`, `"LCTL_T(A)"`.
pub fn parse_action(s: &str) -> CanonicalAction {
    let s = s.trim();
    // Layer references: MO(X), LT(X, Y), TG(X), TO(X), TT(X), DF(X)
    if let Some(rest) = s.strip_prefix("MO(").and_then(|r| r.strip_suffix(')')) {
        return CanonicalAction::Mo {
            layer: parse_layer_ref(rest.trim()),
        };
    }
    if let Some(rest) = s.strip_prefix("TG(").and_then(|r| r.strip_suffix(')')) {
        return CanonicalAction::Tg {
            layer: parse_layer_ref(rest.trim()),
        };
    }
    if let Some(rest) = s.strip_prefix("TO(").and_then(|r| r.strip_suffix(')')) {
        return CanonicalAction::To {
            layer: parse_layer_ref(rest.trim()),
        };
    }
    if let Some(rest) = s.strip_prefix("TT(").and_then(|r| r.strip_suffix(')')) {
        return CanonicalAction::Tt {
            layer: parse_layer_ref(rest.trim()),
        };
    }
    if let Some(rest) = s.strip_prefix("DF(").and_then(|r| r.strip_suffix(')')) {
        return CanonicalAction::Df {
            layer: parse_layer_ref(rest.trim()),
        };
    }
    if let Some(rest) = s.strip_prefix("LT(").and_then(|r| r.strip_suffix(')')) {
        let (lref, tap) = split_once_top(rest, ',').unwrap_or((rest, ""));
        return CanonicalAction::Lt {
            layer: parse_layer_ref(lref.trim()),
            tap: Box::new(parse_action(tap.trim())),
        };
    }
    // USER custom keycode slots: USER00..USER31. Out-of-range values
    // fall through to the generic keycode parser so the lint catches
    // them as unknown-keycode instead of producing a silent codegen
    // drop or a confusing QMK undefined-symbol link error. See
    // `MAX_USER_KEYCODE_SLOT` in canonical.rs for the rationale.
    if let Some(rest) = s.strip_prefix("USER") {
        if let Ok(n) = rest.parse::<u8>() {
            if (n as u32) <= super::canonical::MAX_USER_KEYCODE_SLOT {
                return CanonicalAction::Custom(n);
            }
        }
    }
    // Mod-tap: LCTL_T(A), LSFT_T(KC_A), etc.
    if let Some(open) = s.find('(') {
        if let Some(close) = s.rfind(')') {
            let head = &s[..open];
            let body = &s[open + 1..close];
            if let Some(prefix) = head.strip_suffix("_T") {
                if let Some(m) = Modifier::from_str(prefix) {
                    return CanonicalAction::ModTap {
                        mod_: m,
                        tap: Box::new(parse_action(body.trim())),
                    };
                }
            }
        }
    }
    // Plain: modifier?
    if let Some(m) = Modifier::from_str(s) {
        return CanonicalAction::Modifier(m);
    }
    // Plain keycode.
    CanonicalAction::Keycode(Keycode::from_str(s))
}

fn parse_layer_ref(s: &str) -> LayerRef {
    if let Ok(n) = s.parse::<u8>() {
        LayerRef::Index(n)
    } else {
        LayerRef::Name(s.to_string())
    }
}

/// Render a [`CanonicalLayout`] back into the textual `layout.toml`
/// format. Used by `oryx-bench detach` to convert pulled JSON into
/// hand-editable TOML.
///
/// Each binding is rendered in compact form (`L_pinky_home = "A"`) when
/// only `tap` is set with a plain keycode/modifier; verbose form
/// (`{ tap = "...", hold = "..." }`) otherwise. Empty positions
/// (`KC_NO` and `KC_TRNS`) are omitted to keep the output small.
///
/// Errors instead of returning a placeholder string when the geometry
/// is unknown — the previous behavior of returning `"# unknown
/// geometry — could not render\n"` was a silent data-loss footgun
/// because `detach` would write that as a real `layout.toml` and then
/// delete `pulled/`.
pub fn render_layout_toml(layout: &super::canonical::CanonicalLayout) -> anyhow::Result<String> {
    use std::fmt::Write;

    let geom = super::geometry::get(layout.geometry.as_str()).ok_or_else(|| {
        anyhow::anyhow!(
            "cannot render layout.toml: unknown geometry '{}'. Adding a new geometry is documented in CONTRIBUTING.md.",
            layout.geometry
        )
    })?;

    let mut out = String::new();
    out.push_str("# layout.toml — local-mode visual layout\n");
    out.push_str("#\n");
    out.push_str("# Generated by `oryx-bench detach`. Hand-edit freely from here.\n\n");
    out.push_str("[meta]\n");
    let _ = writeln!(out, "title    = \"{}\"", layout.title);
    let _ = writeln!(out, "geometry = \"{}\"", layout.geometry);
    out.push('\n');

    let mut sorted_layers: Vec<&super::canonical::CanonicalLayer> = layout.layers.iter().collect();
    sorted_layers.sort_by_key(|l| l.position);
    for layer in sorted_layers {
        out.push_str("[[layers]]\n");
        let _ = writeln!(out, "name     = \"{}\"", layer.name);
        let _ = writeln!(out, "position = {}", layer.position);
        out.push_str("[layers.keys]\n");
        for (idx, key) in layer.keys.iter().enumerate() {
            let Some(name) = geom.index_to_position(idx) else {
                continue;
            };
            if key_is_empty(key) {
                continue;
            }
            let _ = writeln!(out, "{name} = {}", render_key_entry(key));
        }
        out.push('\n');
    }
    Ok(out)
}

fn key_is_empty(key: &super::canonical::CanonicalKey) -> bool {
    use super::canonical::CanonicalAction::{None as NoneAct, Transparent};
    let blank = |a: &Option<super::canonical::CanonicalAction>| {
        a.is_none() || matches!(a, Some(NoneAct) | Some(Transparent))
    };
    blank(&key.tap)
        && blank(&key.hold)
        && blank(&key.double_tap)
        && blank(&key.tap_hold)
        && key.tapping_term.is_none()
        && key.custom_label.is_none()
}

fn render_key_entry(key: &super::canonical::CanonicalKey) -> String {
    let only_tap = key.hold.is_none()
        && key.double_tap.is_none()
        && key.tap_hold.is_none()
        && key.tapping_term.is_none();
    if only_tap {
        if let Some(action) = &key.tap {
            return format!("\"{}\"", render_action(action));
        }
    }
    let mut parts: Vec<String> = Vec::new();
    if let Some(a) = &key.tap {
        parts.push(format!("tap = \"{}\"", render_action(a)));
    }
    if let Some(a) = &key.hold {
        parts.push(format!("hold = \"{}\"", render_action(a)));
    }
    if let Some(a) = &key.double_tap {
        parts.push(format!("double_tap = \"{}\"", render_action(a)));
    }
    if let Some(a) = &key.tap_hold {
        parts.push(format!("tap_hold = \"{}\"", render_action(a)));
    }
    if let Some(t) = key.tapping_term {
        parts.push(format!("tapping_term = {t}"));
    }
    format!("{{ {} }}", parts.join(", "))
}

/// Inverse of `parse_action`. Renders a [`CanonicalAction`] into the
/// short string form layout.toml accepts.
pub fn render_action(action: &super::canonical::CanonicalAction) -> String {
    use super::canonical::CanonicalAction;
    match action {
        CanonicalAction::Keycode(kc) => kc.canonical_name().into_owned(),
        CanonicalAction::Modifier(m) => m.canonical_name().to_string(),
        CanonicalAction::Mo { layer } => format!("MO({})", render_layer_ref(layer)),
        CanonicalAction::Tg { layer } => format!("TG({})", render_layer_ref(layer)),
        CanonicalAction::To { layer } => format!("TO({})", render_layer_ref(layer)),
        CanonicalAction::Tt { layer } => format!("TT({})", render_layer_ref(layer)),
        CanonicalAction::Df { layer } => format!("DF({})", render_layer_ref(layer)),
        CanonicalAction::Lt { layer, tap } => {
            format!("LT({}, {})", render_layer_ref(layer), render_action(tap))
        }
        CanonicalAction::ModTap { mod_, tap } => {
            format!("{}_T({})", mod_.canonical_name(), render_action(tap))
        }
        CanonicalAction::Modified { mods, base } => {
            let mut out = render_action(base);
            for m in mods.iter().rev() {
                out = format!("{}({})", m.canonical_name(), out);
            }
            out
        }
        CanonicalAction::Custom(n) => format!("USER{:02}", n),
        CanonicalAction::Transparent => "KC_TRNS".to_string(),
        CanonicalAction::None => "KC_NO".to_string(),
    }
}

fn render_layer_ref(r: &super::canonical::LayerRef) -> String {
    match r {
        super::canonical::LayerRef::Name(n) => n.clone(),
        super::canonical::LayerRef::Index(i) => i.to_string(),
    }
}

fn split_once_top(s: &str, delim: char) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c == delim && depth == 0 => return Some((&s[..i], &s[i + 1..])),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_compact_keycode() {
        let k = parse_action("KC_A");
        assert!(matches!(k, CanonicalAction::Keycode(Keycode::KcA)));
    }

    #[test]
    fn parses_mo_layer() {
        let k = parse_action("MO(SymNum)");
        if let CanonicalAction::Mo { layer } = k {
            assert_eq!(layer.as_name(), Some("SymNum"));
        } else {
            panic!("expected MO action");
        }
    }

    #[test]
    fn parses_lt_with_inner_keycode() {
        let k = parse_action("LT(SymNum, BSPC)");
        if let CanonicalAction::Lt { layer, tap } = k {
            assert_eq!(layer.as_name(), Some("SymNum"));
            assert_eq!(tap.display(), "KC_BSPC");
        } else {
            panic!("expected LT action");
        }
    }

    #[test]
    fn parses_mod_tap() {
        let k = parse_action("LCTL_T(KC_A)");
        assert!(matches!(k, CanonicalAction::ModTap { .. }));
    }

    #[test]
    fn parses_user_custom_keycode() {
        let k = parse_action("USER05");
        match k {
            CanonicalAction::Custom(n) => assert_eq!(n, 5),
            other => panic!("expected Custom(5), got {other:?}"),
        }
    }

    #[test]
    fn parses_layout_file() {
        let raw = r#"
[meta]
title = "Test"
geometry = "voyager"

[[layers]]
name = "Main"
position = 0

[layers.keys]
L_pinky_home = "A"
R_thumb_outer = { tap = "BSPC", hold = "MO(SymNum)" }
"#;
        let file: LayoutFile = toml::from_str(raw).unwrap();
        assert_eq!(file.layers.len(), 1);
        assert_eq!(file.layers[0].keys.len(), 2);
    }
}
