//! Hand-rolled split-grid renderer with box-drawing characters.
//!
//! Renders each keyboard half as a separate labeled grid with dynamic
//! per-column widths and human-friendly key labels.

use std::fmt::Write;

use crate::schema::canonical::{CanonicalAction, CanonicalKey, CanonicalLayer, LayerRef};
use crate::schema::geometry::{Geometry, GridLayout, Hand, ThumbKey, ThumbKeyWidth};
use crate::schema::keycode::{Keycode, Modifier};

use super::RenderOptions;

// ── Public entry point ─────────────────────────────────────────────

/// Render a single layer as two labeled box-drawing grids (one per hand)
/// with dynamic column widths, human-friendly labels, and a legend.
///
/// `all_layers` is used to resolve layer names in LT/MO labels.
pub fn render_layer(
    geom: &dyn Geometry,
    layer: &CanonicalLayer,
    all_layers: &[CanonicalLayer],
    opts: &RenderOptions,
) -> String {
    let grid = geom.ascii_layout();

    let label = |idx: usize| -> String {
        if opts.show_position_names {
            geom.index_to_position(idx).unwrap_or("?").to_string()
        } else {
            layer
                .keys
                .get(idx)
                .map(|k| friendly_key(k, all_layers))
                .unwrap_or_default()
        }
    };

    // Collect labels for each half.
    let left_rows: Vec<Vec<String>> = grid
        .rows
        .iter()
        .map(|row| {
            row.left
                .iter()
                .map(|mi| mi.map(&label).unwrap_or_default())
                .collect()
        })
        .collect();

    let right_rows: Vec<Vec<String>> = grid
        .rows
        .iter()
        .map(|row| {
            row.right
                .iter()
                .map(|mi| mi.map(&label).unwrap_or_default())
                .collect()
        })
        .collect();

    let left_widths = compute_col_widths(&left_rows);
    let right_widths = compute_col_widths(&right_rows);

    let (left_thumbs, right_thumbs) = split_thumbs_by_hand(grid);

    let mut out = String::new();

    // Left half
    out.push_str("Left:\n");
    render_grid(&mut out, &left_rows, &left_widths);
    if !left_thumbs.is_empty() {
        let matrix_w = grid_total_width(&left_widths);
        render_thumbs(&mut out, &left_thumbs, Hand::Left, matrix_w, &label);
    }

    out.push('\n');

    // Right half
    out.push_str("Right:\n");
    render_grid(&mut out, &right_rows, &right_widths);
    if !right_thumbs.is_empty() {
        let matrix_w = grid_total_width(&right_widths);
        render_thumbs(&mut out, &right_thumbs, Hand::Right, matrix_w, &label);
    }

    // Legend
    out.push_str("\nX/Mod = tap X, hold Mod    X/Layer = tap X, hold layer\n");

    out
}

// ── Human-friendly label rendering ─────────────────────────────────
//
// Produces readable labels: `ENT/Alt` instead of `LA:ENT`,
// `BSPC/Sym+Num` instead of `1:BSPC`, `,` instead of `COMM`.

fn friendly_key(key: &CanonicalKey, layers: &[CanonicalLayer]) -> String {
    match (&key.tap, &key.hold) {
        (Some(CanonicalAction::Lt { layer, tap }), _) => {
            let t = friendly_action(tap, layers);
            let l = layer_display_name(layer, layers);
            format!("{t}/{l}")
        }
        (Some(CanonicalAction::ModTap { mod_, tap }), _) => {
            let t = friendly_action(tap, layers);
            let m = friendly_modifier(mod_);
            format!("{t}/{m}")
        }
        (Some(t), Some(h)) => {
            format!(
                "{}/{}",
                friendly_action(t, layers),
                friendly_hold(h, layers)
            )
        }
        (Some(t), None) => friendly_action(t, layers),
        (None, Some(h)) => format!("/{}", friendly_hold(h, layers)),
        (None, None) => String::new(),
    }
}

/// Render a hold-layer action with friendly modifier names.
/// Standalone `Modifier` uses the short form (Sft, Alt) since the
/// physical position already implies L/R.
fn friendly_hold(action: &CanonicalAction, layers: &[CanonicalLayer]) -> String {
    match action {
        CanonicalAction::Modifier(m) => friendly_modifier(m).to_string(),
        other => friendly_action(other, layers),
    }
}

fn friendly_action(action: &CanonicalAction, layers: &[CanonicalLayer]) -> String {
    match action {
        CanonicalAction::Keycode(kc) => friendly_keycode(kc),
        // Standalone modifier as a tap action — keep L/R distinction.
        CanonicalAction::Modifier(m) => m.canonical_name().to_string(),
        CanonicalAction::Mo { layer } => {
            format!("MO({})", layer_display_name(layer, layers))
        }
        CanonicalAction::Tg { layer } => {
            format!("TG({})", layer_display_name(layer, layers))
        }
        CanonicalAction::To { layer } => {
            format!("TO({})", layer_display_name(layer, layers))
        }
        CanonicalAction::Tt { layer } => {
            format!("TT({})", layer_display_name(layer, layers))
        }
        CanonicalAction::Df { layer } => {
            format!("DF({})", layer_display_name(layer, layers))
        }
        CanonicalAction::Lt { layer, tap } => {
            format!(
                "{}/{}",
                friendly_action(tap, layers),
                layer_display_name(layer, layers)
            )
        }
        CanonicalAction::ModTap { mod_, tap } => {
            format!(
                "{}/{}",
                friendly_action(tap, layers),
                friendly_modifier(mod_)
            )
        }
        CanonicalAction::Modified { mods, base } => {
            let chain: String = mods
                .iter()
                .map(|m| friendly_modifier(m))
                .collect::<Vec<_>>()
                .join("+");
            format!("{}({})", chain, friendly_action(base, layers))
        }
        CanonicalAction::Custom(n) => format!("USR{n:02}"),
        CanonicalAction::Transparent => "TRNS".into(),
        CanonicalAction::None => String::new(),
    }
}

/// Resolve a layer reference to its human-readable name.
fn layer_display_name(r: &LayerRef, layers: &[CanonicalLayer]) -> String {
    match r {
        LayerRef::Name(n) => layers
            .iter()
            .find(|l| l.name == *n)
            .map(|l| l.name.clone())
            .unwrap_or_else(|| n.clone()),
        LayerRef::Index(i) => layers
            .iter()
            .find(|l| l.position == *i)
            .map(|l| l.name.clone())
            .unwrap_or_else(|| format!("L{i}")),
    }
}

/// Human-friendly modifier name. No L/R prefix — the physical
/// position on the board already tells you which hand.
fn friendly_modifier(m: &Modifier) -> &'static str {
    match m {
        Modifier::Lctl | Modifier::Rctl => "Ctl",
        Modifier::Lsft | Modifier::Rsft => "Sft",
        Modifier::Lalt | Modifier::Ralt => "Alt",
        Modifier::Lgui | Modifier::Rgui => "Gui",
        Modifier::Hypr => "Hypr",
        Modifier::Meh => "Meh",
    }
}

/// Human-friendly keycode: actual symbols for punctuation,
/// short names for special keys.
fn friendly_keycode(kc: &Keycode) -> String {
    use Keycode::*;
    let s: &str = match kc {
        KcNo => return String::new(),
        KcTransparent => "TRNS",

        // Letters
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

        // Digits
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

        // Function keys
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

        // Punctuation → actual symbols
        KcGrave => "`",
        KcMinus => "-",
        KcEqual => "=",
        KcLbracket => "[",
        KcRbracket => "]",
        KcBslash => "\\",
        KcSemicolon => ";",
        KcQuote => "'",
        KcComma => ",",
        KcDot => ".",
        KcSlash => "/",

        // Shifted symbols → actual symbols
        KcExclaim => "!",
        KcAt => "@",
        KcHash => "#",
        KcDollar => "$",
        KcPercent => "%",
        KcCircumflex => "^",
        KcAmpersand => "&",
        KcAsterisk => "*",
        KcLparen => "(",
        KcRparen => ")",
        KcColon => ":",
        KcLcurly => "{",
        KcRcurly => "}",
        KcPlus => "+",
        KcUnderscore => "_",
        KcTilde => "~",
        KcPipe => "|",
        KcDblQuote => "\"",
        KcLessThan => "<",
        KcGreaterThan => ">",

        // Navigation
        KcLeft => "LEFT",
        KcRight => "RGHT",
        KcUp => "UP",
        KcDown => "DOWN",
        KcHome => "HOME",
        KcEnd => "END",
        KcPgup => "PGUP",
        KcPgdn => "PGDN",

        // Editing / special
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

        // Modifier keycodes (standalone, not mod-tap)
        KcLctl => "LCTL",
        KcLsft => "LSFT",
        KcLalt => "LALT",
        KcLgui => "LGUI",
        KcRctl => "RCTL",
        KcRsft => "RSFT",
        KcRalt => "RALT",
        KcRgui => "RGUI",

        // Keypad
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

        // Media
        KcAudioMute => "MUTE",
        KcAudioVolUp => "VOLU",
        KcAudioVolDown => "VOLD",
        KcMediaPlayPause => "MPLY",
        KcMediaNext => "MNXT",
        KcMediaPrev => "MPRV",
        KcMediaStop => "MSTP",

        // System
        KcSystemPower => "PWR",
        KcSystemSleep => "SLEP",
        KcSystemWake => "WAKE",

        // Mouse
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

        // RGB
        KcRgbToggle => "RTOG",
        KcRgbModeForward => "RMOD",
        KcRgbModeReverse => "RRMOD",
        KcRgbHueUp => "RHUI",
        KcRgbHueDown => "RHUD",
        KcRgbSatUp => "RSAI",
        KcRgbSatDown => "RSAD",
        KcRgbValUp => "RVAI",
        KcRgbValDown => "RVAD",

        // QMK
        KcBootloader => "BOOT",
        KcReset => "RESET",

        Other(s) => {
            return s
                .strip_prefix("KC_")
                .or_else(|| s.strip_prefix("US_"))
                .unwrap_or(s)
                .to_string();
        }
    };
    s.into()
}

// ── Column width computation ────────────────────────────────────────

/// Compute per-column content widths (minimum 1 character).
fn compute_col_widths(rows: &[Vec<String>]) -> Vec<usize> {
    let ncols = rows.first().map(|r| r.len()).unwrap_or(0);
    let mut widths = vec![1usize; ncols];
    for row in rows {
        for (col, lbl) in row.iter().enumerate() {
            widths[col] = widths[col].max(lbl.chars().count());
        }
    }
    widths
}

/// Total display width of a box-drawing grid with given column content
/// widths. Each column occupies `w + 2` (1 space padding each side),
/// plus `n + 1` border characters.
fn grid_total_width(widths: &[usize]) -> usize {
    widths.iter().sum::<usize>() + 3 * widths.len() + 1
}

// ── Box-drawing grid ────────────────────────────────────────────────

enum BorderKind {
    Top,
    Mid,
    Bot,
}

fn push_h_border(out: &mut String, widths: &[usize], kind: BorderKind) {
    let (left, cross, right) = match kind {
        BorderKind::Top => ('┌', '┬', '┐'),
        BorderKind::Mid => ('├', '┼', '┤'),
        BorderKind::Bot => ('└', '┴', '┘'),
    };
    out.push(left);
    for (i, &w) in widths.iter().enumerate() {
        for _ in 0..w + 2 {
            out.push('─');
        }
        if i < widths.len() - 1 {
            out.push(cross);
        }
    }
    out.push(right);
}

fn render_grid(out: &mut String, rows: &[Vec<String>], widths: &[usize]) {
    push_h_border(out, widths, BorderKind::Top);
    out.push('\n');

    for (i, row) in rows.iter().enumerate() {
        out.push('│');
        for (col, label) in row.iter().enumerate() {
            let w = widths[col];
            let _ = write!(out, " {:^w$} │", label);
        }
        out.push('\n');

        if i < rows.len() - 1 {
            push_h_border(out, widths, BorderKind::Mid);
            out.push('\n');
        }
    }

    push_h_border(out, widths, BorderKind::Bot);
    out.push('\n');
}

// ── Thumb cluster rendering ─────────────────────────────────────────

fn split_thumbs_by_hand(
    grid: &'static GridLayout,
) -> (Vec<&'static ThumbKey>, Vec<&'static ThumbKey>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for cluster in grid.thumb_clusters {
        match cluster.hand {
            Hand::Left => left.extend(cluster.keys.iter()),
            Hand::Right => right.extend(cluster.keys.iter()),
        }
    }
    (left, right)
}

/// Render thumb keys as a single-row box-drawing grid with size distinction.
///
/// Display order:
/// - Left hand: \[outer, inner\] (outer at left edge, inner near split gap)
/// - Right hand: \[inner, outer\] (inner near gap, outer at right edge)
///
/// The outer (Wide) key cell is at least 1.5× the inner (Standard) cell
/// width, reflecting the physical 1u vs 1.5u key sizes.
fn render_thumbs(
    out: &mut String,
    thumbs: &[&'static ThumbKey],
    hand: Hand,
    matrix_width: usize,
    label: &dyn Fn(usize) -> String,
) {
    if thumbs.is_empty() {
        return;
    }

    // Physical display order: left hand reverses (outer first).
    let ordered: Vec<&ThumbKey> = if hand == Hand::Left {
        thumbs.iter().rev().copied().collect()
    } else {
        thumbs.to_vec()
    };

    let labels: Vec<String> = ordered.iter().map(|tk| label(tk.index)).collect();

    // Determine content widths with physical size constraints.
    let standard_content: usize = ordered
        .iter()
        .zip(labels.iter())
        .filter(|(tk, _)| tk.width == ThumbKeyWidth::Standard)
        .map(|(_, l)| l.chars().count().max(3))
        .max()
        .unwrap_or(3);

    // Wide key minimum: ceil(standard × 1.5)
    let wide_min = (standard_content * 3 + 1) / 2;

    let cell_widths: Vec<usize> = ordered
        .iter()
        .zip(labels.iter())
        .map(|(tk, lbl)| {
            let content_len = lbl.chars().count();
            match tk.width {
                ThumbKeyWidth::Standard => content_len.max(standard_content),
                ThumbKeyWidth::Wide => content_len.max(wide_min),
            }
        })
        .collect();

    let thumb_total = grid_total_width(&cell_widths);

    // Alignment: left-hand thumbs right-aligned under matrix,
    // right-hand thumbs left-aligned.
    let indent = match hand {
        Hand::Left => matrix_width.saturating_sub(thumb_total),
        Hand::Right => 0,
    };
    let pad = " ".repeat(indent);

    out.push_str(&pad);
    push_h_border(out, &cell_widths, BorderKind::Top);
    out.push('\n');

    out.push_str(&pad);
    out.push('│');
    for (i, lbl) in labels.iter().enumerate() {
        let w = cell_widths[i];
        let _ = write!(out, " {:^w$} │", lbl);
    }
    out.push('\n');

    out.push_str(&pad);
    push_h_border(out, &cell_widths, BorderKind::Bot);
    out.push('\n');
}

// ── Tests ───────────────────────────────────────────────────────────

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
        // Should produce two labeled halves with box-drawing.
        assert!(
            out.contains("Left:"),
            "expected Left: label in output:\n{out}"
        );
        assert!(
            out.contains("Right:"),
            "expected Right: label in output:\n{out}"
        );
        assert!(out.contains('┌'), "expected box-drawing in output:\n{out}");
        assert!(out.contains('│'), "expected box-drawing in output:\n{out}");
    }

    #[test]
    fn renders_position_names_without_truncation() {
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
        // Position names should appear in full (no ellipsis truncation).
        assert!(
            out.contains("L_pinky_num"),
            "expected full position name in output:\n{out}"
        );
        assert!(
            !out.contains('…'),
            "no truncation ellipsis expected:\n{out}"
        );
    }

    #[test]
    fn friendly_mod_tap_format() {
        let key = CanonicalKey {
            tap: Some(CanonicalAction::ModTap {
                mod_: Modifier::Lalt,
                tap: Box::new(CanonicalAction::Keycode(Keycode::KcEnter)),
            }),
            ..Default::default()
        };
        assert_eq!(friendly_key(&key, &[]), "ENT/Alt");
    }

    #[test]
    fn friendly_layer_tap_format() {
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
        assert_eq!(friendly_key(&key, &layers), "BSPC/Sym+Num");
    }

    #[test]
    fn friendly_hold_only_modifier() {
        let key = CanonicalKey {
            tap: None,
            hold: Some(CanonicalAction::Modifier(Modifier::Lsft)),
            ..Default::default()
        };
        assert_eq!(friendly_key(&key, &[]), "/Sft");
    }

    #[test]
    fn friendly_keycode_symbols() {
        assert_eq!(friendly_keycode(&Keycode::KcComma), ",");
        assert_eq!(friendly_keycode(&Keycode::KcSlash), "/");
        assert_eq!(friendly_keycode(&Keycode::KcQuote), "'");
        assert_eq!(friendly_keycode(&Keycode::KcSemicolon), ";");
        assert_eq!(friendly_keycode(&Keycode::KcMinus), "-");
        assert_eq!(friendly_keycode(&Keycode::KcEqual), "=");
        assert_eq!(friendly_keycode(&Keycode::KcBslash), "\\");
        assert_eq!(friendly_keycode(&Keycode::KcGrave), "`");
    }

    #[test]
    fn friendly_keycode_special_keys() {
        assert_eq!(friendly_keycode(&Keycode::KcEnter), "ENT");
        assert_eq!(friendly_keycode(&Keycode::KcBspc), "BSPC");
        assert_eq!(friendly_keycode(&Keycode::KcSpace), "SPC");
        assert_eq!(friendly_keycode(&Keycode::KcA), "A");
        assert_eq!(friendly_keycode(&Keycode::Kc1), "1");
    }

    #[test]
    fn friendly_modifier_names() {
        assert_eq!(friendly_modifier(&Modifier::Lalt), "Alt");
        assert_eq!(friendly_modifier(&Modifier::Ralt), "Alt");
        assert_eq!(friendly_modifier(&Modifier::Lctl), "Ctl");
        assert_eq!(friendly_modifier(&Modifier::Lsft), "Sft");
        assert_eq!(friendly_modifier(&Modifier::Lgui), "Gui");
    }

    #[test]
    fn box_drawing_grid_structure() {
        let rows = vec![
            vec!["A".to_string(), "BB".to_string()],
            vec!["CCC".to_string(), "D".to_string()],
        ];
        let widths = compute_col_widths(&rows);
        assert_eq!(widths, vec![3, 2]);

        let mut out = String::new();
        render_grid(&mut out, &rows, &widths);
        // Verify box-drawing structure.
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "┌─────┬────┐");
        assert_eq!(lines[1], "│  A  │ BB │");
        assert_eq!(lines[2], "├─────┼────┤");
        assert_eq!(lines[3], "│ CCC │ D  │");
        assert_eq!(lines[4], "└─────┴────┘");
    }

    #[test]
    fn legend_present() {
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
        assert!(
            out.contains("X/Mod = tap X, hold Mod"),
            "expected legend in output:\n{out}"
        );
    }
}
