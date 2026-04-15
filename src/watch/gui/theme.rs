//! Visual theme for the watch window. Centralized so the indicator and
//! the drill trainer share one palette.

use egui::{Color32, Context, FontFamily, FontId, TextStyle, Visuals};

pub const BG: Color32 = Color32::from_rgb(0x12, 0x14, 0x18);
pub const PANEL: Color32 = Color32::from_rgb(0x1a, 0x1d, 0x23);
pub const KEY: Color32 = Color32::from_rgb(0x24, 0x29, 0x33);
pub const KEY_ACCENT: Color32 = Color32::from_rgb(0x3a, 0xa0, 0xff);
/// Fill for a key the firmware reports as currently held. Warm amber
/// so it reads as "this physical key is down right now" — distinct
/// from the cool KEY_ACCENT used for compositional highlights
/// (combos, next-key hints) the layout might surface later.
pub const KEY_PRESSED: Color32 = Color32::from_rgb(0xff, 0xc8, 0x57);
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe8, 0xec);
pub const MUTED: Color32 = Color32::from_rgb(0x8a, 0x93, 0xa0);
pub const OK: Color32 = Color32::from_rgb(0x4c, 0xd4, 0x82);
pub const WARN: Color32 = Color32::from_rgb(0xff, 0xc8, 0x57);
pub const ERR: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);

pub fn apply(ctx: &Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = BG;
    visuals.extreme_bg_color = PANEL;
    visuals.override_text_color = Some(TEXT);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(22.0, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(13.0, FontFamily::Monospace),
    );
    ctx.set_style(style);
}
