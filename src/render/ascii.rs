//! Hand-rolled split-grid renderer.
//!
//! Not built on `tabled` — the Voyager's shape doesn't fit `tabled`'s
//! rectangular model. ~100 lines of straightforward formatting code.

use crate::schema::canonical::{CanonicalAction, CanonicalKey, CanonicalLayer, LayerRef};
use crate::schema::geometry::{Geometry, GridLayout, Hand, ThumbCluster};
use crate::schema::keycode::{Keycode, Modifier};

use super::RenderOptions;

/// Render a single layer as an ASCII split-grid keyboard picture.
///
/// `all_layers` is used to resolve layer names to position numbers in
/// compact labels (e.g. `LT(Sym+Num, KC_BSPC)` → `1:BSPC`). Pass
/// the full `CanonicalLayout.layers` slice.
pub fn render_layer(
    geom: &dyn Geometry,
    layer: &CanonicalLayer,
    all_layers: &[CanonicalLayer],
    opts: &RenderOptions,
) -> String {
    let grid = geom.ascii_layout();
    // Fits most compact labels (e.g. "1:BSPC" = 6, "LA:ENT" = 6).
    // Longer names are truncated with `…`.
    const CELL_WIDTH: usize = 7;

    // Pre-format each cell.
    let cell = |idx: usize| -> String {
        let s = if opts.show_position_names {
            geom.index_to_position(idx).unwrap_or("?").to_string()
        } else {
            layer
                .keys
                .get(idx)
                .map(|k| compact_key(k, all_layers))
                .unwrap_or_default()
        };
        let formatted = truncate_with_ellipsis(&s, CELL_WIDTH);
        format!("{formatted:<CELL_WIDTH$}")
    };

    let mut out = String::new();

    // Main matrix rows
    for row in grid.rows {
        let mut line = String::new();
        for maybe_idx in row.left {
            line.push('|');
            line.push_str(&match *maybe_idx {
                Some(i) => cell(i),
                None => " ".repeat(CELL_WIDTH),
            });
        }
        line.push('|');
        // Gap between halves
        line.push_str("    ");
        for maybe_idx in row.right {
            line.push('|');
            line.push_str(&match *maybe_idx {
                Some(i) => cell(i),
                None => " ".repeat(CELL_WIDTH),
            });
        }
        line.push('|');
        out.push_str(&line);
        out.push('\n');
    }

    // Thumb clusters — render below the main matrix, aligned under the
    // two halves they belong to.
    if !grid.thumb_clusters.is_empty() {
        out.push('\n');
        let (left_thumbs, right_thumbs) = split_thumbs_by_hand(grid);
        let max_len = left_thumbs.len().max(right_thumbs.len());
        for i in 0..max_len {
            let left_cell = left_thumbs
                .get(i)
                .map(|&idx| cell(idx))
                .unwrap_or_else(|| " ".repeat(CELL_WIDTH));
            let right_cell = right_thumbs
                .get(i)
                .map(|&idx| cell(idx))
                .unwrap_or_else(|| " ".repeat(CELL_WIDTH));
            out.push_str(&format!(
                "{:>pad$}|{left_cell}|    |{right_cell}|\n",
                "",
                pad = CELL_WIDTH * 4 + 5,
            ));
        }
    }
    out
}

// ── Compact label rendering ─────────────────────────────────────────
//
// Produces short, information-dense labels for the fixed-width ASCII
// grid. The full verbose `CanonicalKey::display()` is for code
// generation and explain output; the compact form here is for visual
// scanning at a glance.
//
// Key transformations:
//   - KC_ prefix stripped from all keycodes
//   - Long keycode names use QMK short aliases (ENTER→ENT, SLASH→SLSH)
//   - LT: `<layer_position>:<tap>` (e.g. `1:BSPC`)
//   - ModTap: `<2-char modifier>:<tap>` (e.g. `LA:ENT`)
//   - MO/TG/TO: `MO(<pos>)`, `TG(<pos>)`, etc.
//   - Modified: `<mod_chain>(<tap>)` with abbreviated mods

fn compact_key(key: &CanonicalKey, layers: &[CanonicalLayer]) -> String {
    match (&key.tap, &key.hold) {
        (Some(CanonicalAction::Lt { layer, tap }), _) => {
            let lr = compact_layer_ref(layer, layers);
            let t = compact_action(tap, layers);
            format!("{lr}:{t}")
        }
        (Some(CanonicalAction::ModTap { mod_, tap }), _) => {
            let m = short_modifier(mod_);
            let t = compact_action(tap, layers);
            format!("{m}:{t}")
        }
        (Some(t), Some(h)) => {
            format!(
                "{}/{}",
                compact_action(t, layers),
                compact_action(h, layers)
            )
        }
        (Some(t), None) => compact_action(t, layers),
        (None, Some(h)) => format!("/{}", compact_action(h, layers)),
        (None, None) => String::new(),
    }
}

fn compact_action(action: &CanonicalAction, layers: &[CanonicalLayer]) -> String {
    match action {
        CanonicalAction::Keycode(kc) => short_keycode(kc),
        CanonicalAction::Modifier(m) => m.canonical_name().to_string(),
        CanonicalAction::Mo { layer } => {
            format!("MO({})", compact_layer_ref(layer, layers))
        }
        CanonicalAction::Tg { layer } => {
            format!("TG({})", compact_layer_ref(layer, layers))
        }
        CanonicalAction::To { layer } => {
            format!("TO({})", compact_layer_ref(layer, layers))
        }
        CanonicalAction::Tt { layer } => {
            format!("TT({})", compact_layer_ref(layer, layers))
        }
        CanonicalAction::Df { layer } => {
            format!("DF({})", compact_layer_ref(layer, layers))
        }
        CanonicalAction::Lt { layer, tap } => {
            format!(
                "{}:{}",
                compact_layer_ref(layer, layers),
                compact_action(tap, layers)
            )
        }
        CanonicalAction::ModTap { mod_, tap } => {
            format!("{}:{}", short_modifier(mod_), compact_action(tap, layers))
        }
        CanonicalAction::Modified { mods, base } => {
            let chain: String = mods
                .iter()
                .map(|m| short_modifier(m).to_string())
                .collect::<Vec<_>>()
                .join("+");
            format!("{}({})", chain, compact_action(base, layers))
        }
        CanonicalAction::Custom(n) => format!("USR{n:02}"),
        CanonicalAction::Transparent => "TRNS".into(),
        CanonicalAction::None => String::new(),
    }
}

/// Resolve a layer reference to its position number (compact, always
/// fits). Falls back to the first 2 characters of the name if the
/// layer isn't found in the table (shouldn't happen in practice).
fn compact_layer_ref(r: &LayerRef, layers: &[CanonicalLayer]) -> String {
    match r {
        LayerRef::Name(n) => layers
            .iter()
            .find(|l| l.name == *n)
            .map(|l| l.position.to_string())
            .unwrap_or_else(|| n.chars().take(2).collect()),
        LayerRef::Index(i) => i.to_string(),
    }
}

/// Short keycode name — QMK short alias without KC_ prefix.
fn short_keycode(kc: &Keycode) -> String {
    use Keycode::*;
    let s: &str = match kc {
        KcNo => return String::new(),
        KcTransparent => "TRNS",

        KcA => "A",
        KcB => "B",
        KcC => "C",
        KcD => "D",
        KcE => "E",
        KcF => "F",
        KcG => "G",
        KcH => "H",
        KcI => "I",
        KcJ => "J",
        KcK => "K",
        KcL => "L",
        KcM => "M",
        KcN => "N",
        KcO => "O",
        KcP => "P",
        KcQ => "Q",
        KcR => "R",
        KcS => "S",
        KcT => "T",
        KcU => "U",
        KcV => "V",
        KcW => "W",
        KcX => "X",
        KcY => "Y",
        KcZ => "Z",

        Kc1 => "1",
        Kc2 => "2",
        Kc3 => "3",
        Kc4 => "4",
        Kc5 => "5",
        Kc6 => "6",
        Kc7 => "7",
        Kc8 => "8",
        Kc9 => "9",
        Kc0 => "0",

        KcF1 => "F1",
        KcF2 => "F2",
        KcF3 => "F3",
        KcF4 => "F4",
        KcF5 => "F5",
        KcF6 => "F6",
        KcF7 => "F7",
        KcF8 => "F8",
        KcF9 => "F9",
        KcF10 => "F10",
        KcF11 => "F11",
        KcF12 => "F12",
        KcF13 => "F13",
        KcF14 => "F14",
        KcF15 => "F15",
        KcF16 => "F16",
        KcF17 => "F17",
        KcF18 => "F18",
        KcF19 => "F19",
        KcF20 => "F20",
        KcF21 => "F21",
        KcF22 => "F22",
        KcF23 => "F23",
        KcF24 => "F24",

        KcGrave => "GRV",
        KcMinus => "MINS",
        KcEqual => "EQL",
        KcLbracket => "LBRC",
        KcRbracket => "RBRC",
        KcBslash => "BSLS",
        KcSemicolon => "SCLN",
        KcQuote => "QUOT",
        KcComma => "COMM",
        KcDot => "DOT",
        KcSlash => "SLSH",

        KcExclaim => "EXLM",
        KcAt => "AT",
        KcHash => "HASH",
        KcDollar => "DLR",
        KcPercent => "PERC",
        KcCircumflex => "CIRC",
        KcAmpersand => "AMPR",
        KcAsterisk => "ASTR",
        KcLparen => "LPRN",
        KcRparen => "RPRN",
        KcColon => "COLN",
        KcLcurly => "LCBR",
        KcRcurly => "RCBR",
        KcPlus => "PLUS",
        KcUnderscore => "UNDS",
        KcTilde => "TILD",
        KcPipe => "PIPE",
        KcDblQuote => "DQUO",
        KcLessThan => "LABK",
        KcGreaterThan => "RABK",

        KcLeft => "LEFT",
        KcRight => "RGHT",
        KcUp => "UP",
        KcDown => "DOWN",
        KcHome => "HOME",
        KcEnd => "END",
        KcPgup => "PGUP",
        KcPgdn => "PGDN",

        KcEnter => "ENT",
        KcEscape => "ESC",
        KcBspc => "BSPC",
        KcTab => "TAB",
        KcSpace => "SPC",
        KcDelete => "DEL",
        KcInsert => "INS",
        KcCapsLock => "CAPS",
        KcPrintScreen => "PSCR",
        KcScrollLock => "SCRL",
        KcPause => "PAUS",

        KcLctl => "LCTL",
        KcLsft => "LSFT",
        KcLalt => "LALT",
        KcLgui => "LGUI",
        KcRctl => "RCTL",
        KcRsft => "RSFT",
        KcRalt => "RALT",
        KcRgui => "RGUI",

        KcKp0 => "KP_0",
        KcKp1 => "KP_1",
        KcKp2 => "KP_2",
        KcKp3 => "KP_3",
        KcKp4 => "KP_4",
        KcKp5 => "KP_5",
        KcKp6 => "KP_6",
        KcKp7 => "KP_7",
        KcKp8 => "KP_8",
        KcKp9 => "KP_9",
        KcKpDot => "KP_DOT",
        KcKpPlus => "KP_+",
        KcKpMinus => "KP_-",
        KcKpAsterisk => "KP_*",
        KcKpSlash => "KP_/",
        KcKpEnter => "KP_EN",
        KcKpEqual => "KP_EQ",
        KcNumLock => "NLCK",

        KcAudioMute => "MUTE",
        KcAudioVolUp => "VOLU",
        KcAudioVolDown => "VOLD",
        KcMediaPlayPause => "MPLY",
        KcMediaNext => "MNXT",
        KcMediaPrev => "MPRV",
        KcMediaStop => "MSTP",

        KcSystemPower => "PWR",
        KcSystemSleep => "SLEP",
        KcSystemWake => "WAKE",

        KcMsUp => "MS_U",
        KcMsDown => "MS_D",
        KcMsLeft => "MS_L",
        KcMsRight => "MS_R",
        KcMsBtn1 => "BTN1",
        KcMsBtn2 => "BTN2",
        KcMsBtn3 => "BTN3",
        KcMsWhUp => "WH_U",
        KcMsWhDown => "WH_D",
        KcMsWhLeft => "WH_L",
        KcMsWhRight => "WH_R",

        KcRgbToggle => "RTOG",
        KcRgbModeForward => "RMOD",
        KcRgbModeReverse => "RRMOD",
        KcRgbHueUp => "RHUI",
        KcRgbHueDown => "RHUD",
        KcRgbSatUp => "RSAI",
        KcRgbSatDown => "RSAD",
        KcRgbValUp => "RVAI",
        KcRgbValDown => "RVAD",

        KcBootloader => "BOOT",
        KcReset => "RESET",

        Other(s) => {
            // Strip common prefixes (KC_, US_, etc.) for consistency.
            return s
                .strip_prefix("KC_")
                .or_else(|| s.strip_prefix("US_"))
                .unwrap_or(s)
                .to_string();
        }
    };
    s.into()
}

/// 2-character modifier abbreviation for compact rendering.
fn short_modifier(m: &Modifier) -> &'static str {
    match m {
        Modifier::Lctl => "LC",
        Modifier::Lsft => "LS",
        Modifier::Lalt => "LA",
        Modifier::Lgui => "LG",
        Modifier::Rctl => "RC",
        Modifier::Rsft => "RS",
        Modifier::Ralt => "RA",
        Modifier::Rgui => "RG",
        Modifier::Hypr => "HY",
        Modifier::Meh => "ME",
    }
}

// ── Truncation ──────────────────────────────────────────────────────

/// Truncate `s` to at most `width` characters. When truncation happens
/// the last kept character is replaced with `…` so the user can tell
/// at a glance that the cell is eliding content rather than showing
/// the full binding. (The previous renderer silently cut characters
/// off the right edge, which made `LT(Sym+Num, KC_BSPC)` and
/// `LT(Sym+Num,` look identical in the grid.)
fn truncate_with_ellipsis(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn split_thumbs_by_hand(grid: &'static GridLayout) -> (Vec<usize>, Vec<usize>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for cluster in grid.thumb_clusters {
        let ThumbCluster { hand, keys } = cluster;
        match hand {
            Hand::Left => left.extend_from_slice(keys),
            Hand::Right => right.extend_from_slice(keys),
        }
    }
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::geometry::voyager::Voyager;

    #[test]
    fn renders_empty_layer() {
        let layer = CanonicalLayer {
            name: "Test".into(),
            position: 0,
            keys: vec![CanonicalKey::default(); 52],
        };
        let out = render_layer(
            &Voyager,
            &layer,
            std::slice::from_ref(&layer),
            &RenderOptions::default(),
        );
        // Empty keys render as blank (KC_NO compact form).
        assert!(out.lines().count() > 4);
    }

    #[test]
    fn truncate_with_ellipsis_no_change_when_fits() {
        assert_eq!(truncate_with_ellipsis("KC_A", 7), "KC_A");
        assert_eq!(truncate_with_ellipsis("KC_BSPC", 7), "KC_BSPC");
    }

    #[test]
    fn truncate_with_ellipsis_appends_marker() {
        // 7 chars kept — 6 first chars + ellipsis.
        assert_eq!(truncate_with_ellipsis("LT(Sym+Num, KC_BSPC)", 7), "LT(Sym…");
    }

    #[test]
    fn truncate_with_ellipsis_handles_zero_width() {
        assert_eq!(truncate_with_ellipsis("anything", 0), "");
    }

    #[test]
    fn truncate_with_ellipsis_width_one() {
        // Width 1 means the ellipsis alone.
        assert_eq!(truncate_with_ellipsis("anything", 1), "…");
    }

    #[test]
    fn renders_position_names_with_flag() {
        let layer = CanonicalLayer {
            name: "Test".into(),
            position: 0,
            keys: vec![CanonicalKey::default(); 52],
        };
        let out = render_layer(
            &Voyager,
            &layer,
            std::slice::from_ref(&layer),
            &RenderOptions {
                show_position_names: true,
            },
        );
        // Position names like "L_pinky_num" are wider than our 7-char
        // cell, so they render with the ellipsis truncation marker.
        assert!(
            out.contains("L_pink…"),
            "expected truncated position name with ellipsis in output:\n{out}"
        );
    }

    #[test]
    fn compact_lt_uses_position_colon_tap() {
        use crate::schema::canonical::CanonicalAction;
        let layers = vec![
            CanonicalLayer {
                name: "Main".into(),
                position: 0,
                keys: vec![],
            },
            CanonicalLayer {
                name: "Sym+Num".into(),
                position: 1,
                keys: vec![],
            },
        ];
        let key = CanonicalKey {
            tap: Some(CanonicalAction::Lt {
                layer: LayerRef::Name("Sym+Num".into()),
                tap: Box::new(CanonicalAction::Keycode(Keycode::KcBspc)),
            }),
            ..Default::default()
        };
        let label = compact_key(&key, &layers);
        assert_eq!(label, "1:BSPC");
    }

    #[test]
    fn compact_mod_tap_uses_short_mod_colon_tap() {
        let key = CanonicalKey {
            tap: Some(CanonicalAction::ModTap {
                mod_: Modifier::Lalt,
                tap: Box::new(CanonicalAction::Keycode(Keycode::KcEnter)),
            }),
            ..Default::default()
        };
        let label = compact_key(&key, &[]);
        assert_eq!(label, "LA:ENT");
    }

    #[test]
    fn compact_keycode_strips_kc_prefix() {
        assert_eq!(short_keycode(&Keycode::KcSlash), "SLSH");
        assert_eq!(short_keycode(&Keycode::KcComma), "COMM");
        assert_eq!(short_keycode(&Keycode::KcQuote), "QUOT");
        assert_eq!(short_keycode(&Keycode::KcA), "A");
        assert_eq!(short_keycode(&Keycode::KcBspc), "BSPC");
    }
}
