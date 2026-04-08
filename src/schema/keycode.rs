//! QMK keycode catalog.
//!
//! A finite enum (~250 variants) with a [`Keycode::Other`] catch-all for
//! forward-compat with new QMK keycodes. Each variant carries metadata via
//! methods on the enum (alpha/vowel/modifier classification, canonical name,
//! etc.). The lint rules in `src/lint/` consume this metadata.
//!
//! ## Long vs short forms
//!
//! QMK exposes most keycodes under both a "long" form (`KC_BACKSPACE`) and a
//! "short" form (`KC_BSPC`). [`Keycode::from_str`] accepts either, and
//! [`Keycode::canonical_name`] always returns the short form. Round-tripping a
//! known variant therefore yields the short form even if the input was long.
//!
//! ## Forward-compat
//!
//! Unknown strings parse to [`Keycode::Other`] and serialize back as the same
//! literal string, so the codegen layer can pass through any QMK keycode we
//! haven't catalogued yet. New QMK releases get folded in by adding variants;
//! the lint rules then start being able to reason about them.

use std::borrow::Cow;
use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

/// A single QMK keycode.
///
/// See the [module-level docs](self) for forward-compat behavior and the
/// long-vs-short form contract.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Keycode {
    // ---- Sentinels ----------------------------------------------------------
    /// `KC_NO` — does nothing.
    KcNo,
    /// `KC_TRNS` / `KC_TRANSPARENT` — falls through to the layer below.
    KcTransparent,

    // ---- Letters ------------------------------------------------------------
    KcA,
    KcB,
    KcC,
    KcD,
    KcE,
    KcF,
    KcG,
    KcH,
    KcI,
    KcJ,
    KcK,
    KcL,
    KcM,
    KcN,
    KcO,
    KcP,
    KcQ,
    KcR,
    KcS,
    KcT,
    KcU,
    KcV,
    KcW,
    KcX,
    KcY,
    KcZ,

    // ---- Digits (top row, in QMK order: 1..0) -------------------------------
    Kc1,
    Kc2,
    Kc3,
    Kc4,
    Kc5,
    Kc6,
    Kc7,
    Kc8,
    Kc9,
    Kc0,

    // ---- Function keys ------------------------------------------------------
    KcF1,
    KcF2,
    KcF3,
    KcF4,
    KcF5,
    KcF6,
    KcF7,
    KcF8,
    KcF9,
    KcF10,
    KcF11,
    KcF12,
    KcF13,
    KcF14,
    KcF15,
    KcF16,
    KcF17,
    KcF18,
    KcF19,
    KcF20,
    KcF21,
    KcF22,
    KcF23,
    KcF24,

    // ---- Punctuation --------------------------------------------------------
    KcGrave,
    KcMinus,
    KcEqual,
    KcLbracket,
    KcRbracket,
    KcBslash,
    KcSemicolon,
    KcQuote,
    KcComma,
    KcDot,
    KcSlash,

    // ---- Shifted-symbol shortcuts (Oryx emits these directly) ---------------
    KcExclaim,    // KC_EXLM = S(1)
    KcAt,         // KC_AT   = S(2)
    KcHash,       // KC_HASH = S(3)
    KcDollar,     // KC_DLR  = S(4)
    KcPercent,    // KC_PERC = S(5)
    KcCircumflex, // KC_CIRC = S(6)
    KcAmpersand,  // KC_AMPR = S(7)
    KcAsterisk,   // KC_ASTR = S(8)
    KcLparen,     // KC_LPRN = S(9)
    KcRparen,     // KC_RPRN = S(0)
    KcColon,      // KC_COLN = S(;)
    KcLcurly,     // KC_LCBR = S([)
    KcRcurly,     // KC_RCBR = S(])
    KcPlus,       // KC_PLUS = S(=)

    // ---- Navigation ---------------------------------------------------------
    KcLeft,
    KcRight,
    KcUp,
    KcDown,
    KcHome,
    KcEnd,
    KcPgup,
    KcPgdn,

    // ---- Editing ------------------------------------------------------------
    KcEnter,
    KcEscape,
    KcBspc,
    KcTab,
    KcSpace,
    KcDelete,
    KcInsert,
    KcCapsLock,
    KcPrintScreen,
    KcScrollLock,
    KcPause,

    // ---- Modifiers ----------------------------------------------------------
    KcLctl,
    KcLsft,
    KcLalt,
    KcLgui,
    KcRctl,
    KcRsft,
    KcRalt,
    KcRgui,

    // ---- Keypad -------------------------------------------------------------
    KcKp0,
    KcKp1,
    KcKp2,
    KcKp3,
    KcKp4,
    KcKp5,
    KcKp6,
    KcKp7,
    KcKp8,
    KcKp9,
    KcKpDot,
    KcKpPlus,
    KcKpMinus,
    KcKpAsterisk,
    KcKpSlash,
    KcKpEnter,
    KcKpEqual,
    KcNumLock,

    // ---- Media --------------------------------------------------------------
    KcAudioMute,
    KcAudioVolUp,
    KcAudioVolDown,
    KcMediaPlayPause,
    KcMediaNext,
    KcMediaPrev,
    KcMediaStop,

    // ---- System -------------------------------------------------------------
    KcSystemPower,
    KcSystemSleep,
    KcSystemWake,

    // ---- Mouse --------------------------------------------------------------
    KcMsUp,
    KcMsDown,
    KcMsLeft,
    KcMsRight,
    KcMsBtn1,
    KcMsBtn2,
    KcMsBtn3,
    KcMsWhUp,
    KcMsWhDown,
    KcMsWhLeft,
    KcMsWhRight,

    // ---- RGB ----------------------------------------------------------------
    KcRgbToggle,
    KcRgbModeForward,
    KcRgbModeReverse,
    KcRgbHueUp,
    KcRgbHueDown,
    KcRgbSatUp,
    KcRgbSatDown,
    KcRgbValUp,
    KcRgbValDown,

    // ---- QMK-specific -------------------------------------------------------
    /// `QK_BOOT` — jump to bootloader.
    KcBootloader,
    /// `RESET` / `KC_RESET` — soft reset.
    KcReset,

    // ---- Forward-compat catch-all ------------------------------------------
    /// Any keycode string we don't (yet) have a variant for. The codegen layer
    /// emits the literal string verbatim into `keymap.c`, so unknown keycodes
    /// still pass through faithfully.
    Other(String),
}

impl Keycode {
    /// Returns the canonical (short) QMK name for this keycode.
    ///
    /// For [`Keycode::Other`] this is the original string the variant was
    /// constructed with.
    pub fn canonical_name(&self) -> Cow<'static, str> {
        use Keycode::*;
        let s: &'static str = match self {
            KcNo => "KC_NO",
            KcTransparent => "KC_TRNS",

            KcA => "KC_A",
            KcB => "KC_B",
            KcC => "KC_C",
            KcD => "KC_D",
            KcE => "KC_E",
            KcF => "KC_F",
            KcG => "KC_G",
            KcH => "KC_H",
            KcI => "KC_I",
            KcJ => "KC_J",
            KcK => "KC_K",
            KcL => "KC_L",
            KcM => "KC_M",
            KcN => "KC_N",
            KcO => "KC_O",
            KcP => "KC_P",
            KcQ => "KC_Q",
            KcR => "KC_R",
            KcS => "KC_S",
            KcT => "KC_T",
            KcU => "KC_U",
            KcV => "KC_V",
            KcW => "KC_W",
            KcX => "KC_X",
            KcY => "KC_Y",
            KcZ => "KC_Z",

            Kc1 => "KC_1",
            Kc2 => "KC_2",
            Kc3 => "KC_3",
            Kc4 => "KC_4",
            Kc5 => "KC_5",
            Kc6 => "KC_6",
            Kc7 => "KC_7",
            Kc8 => "KC_8",
            Kc9 => "KC_9",
            Kc0 => "KC_0",

            KcF1 => "KC_F1",
            KcF2 => "KC_F2",
            KcF3 => "KC_F3",
            KcF4 => "KC_F4",
            KcF5 => "KC_F5",
            KcF6 => "KC_F6",
            KcF7 => "KC_F7",
            KcF8 => "KC_F8",
            KcF9 => "KC_F9",
            KcF10 => "KC_F10",
            KcF11 => "KC_F11",
            KcF12 => "KC_F12",
            KcF13 => "KC_F13",
            KcF14 => "KC_F14",
            KcF15 => "KC_F15",
            KcF16 => "KC_F16",
            KcF17 => "KC_F17",
            KcF18 => "KC_F18",
            KcF19 => "KC_F19",
            KcF20 => "KC_F20",
            KcF21 => "KC_F21",
            KcF22 => "KC_F22",
            KcF23 => "KC_F23",
            KcF24 => "KC_F24",

            KcGrave => "KC_GRV",
            KcMinus => "KC_MINUS",
            KcEqual => "KC_EQUAL",
            KcLbracket => "KC_LBRC",
            KcRbracket => "KC_RBRC",
            KcBslash => "KC_BSLS",
            KcSemicolon => "KC_SCLN",
            KcQuote => "KC_QUOTE",
            KcComma => "KC_COMMA",
            KcDot => "KC_DOT",
            KcSlash => "KC_SLASH",

            KcExclaim => "KC_EXLM",
            KcAt => "KC_AT",
            KcHash => "KC_HASH",
            KcDollar => "KC_DLR",
            KcPercent => "KC_PERC",
            KcCircumflex => "KC_CIRC",
            KcAmpersand => "KC_AMPR",
            KcAsterisk => "KC_ASTR",
            KcLparen => "KC_LPRN",
            KcRparen => "KC_RPRN",
            KcColon => "KC_COLN",
            KcLcurly => "KC_LCBR",
            KcRcurly => "KC_RCBR",
            KcPlus => "KC_PLUS",

            KcLeft => "KC_LEFT",
            KcRight => "KC_RIGHT",
            KcUp => "KC_UP",
            KcDown => "KC_DOWN",
            KcHome => "KC_HOME",
            KcEnd => "KC_END",
            KcPgup => "KC_PGUP",
            KcPgdn => "KC_PGDN",

            KcEnter => "KC_ENT",
            KcEscape => "KC_ESC",
            KcBspc => "KC_BSPC",
            KcTab => "KC_TAB",
            KcSpace => "KC_SPC",
            KcDelete => "KC_DEL",
            KcInsert => "KC_INS",
            KcCapsLock => "KC_CAPS",
            KcPrintScreen => "KC_PSCR",
            KcScrollLock => "KC_SCRL",
            KcPause => "KC_PAUS",

            KcLctl => "KC_LCTL",
            KcLsft => "KC_LSFT",
            KcLalt => "KC_LALT",
            KcLgui => "KC_LGUI",
            KcRctl => "KC_RCTL",
            KcRsft => "KC_RSFT",
            KcRalt => "KC_RALT",
            KcRgui => "KC_RGUI",

            KcKp0 => "KC_KP_0",
            KcKp1 => "KC_KP_1",
            KcKp2 => "KC_KP_2",
            KcKp3 => "KC_KP_3",
            KcKp4 => "KC_KP_4",
            KcKp5 => "KC_KP_5",
            KcKp6 => "KC_KP_6",
            KcKp7 => "KC_KP_7",
            KcKp8 => "KC_KP_8",
            KcKp9 => "KC_KP_9",
            KcKpDot => "KC_KP_DOT",
            KcKpPlus => "KC_KP_PLUS",
            KcKpMinus => "KC_KP_MINUS",
            KcKpAsterisk => "KC_KP_ASTERISK",
            KcKpSlash => "KC_KP_SLASH",
            KcKpEnter => "KC_KP_ENTER",
            KcKpEqual => "KC_KP_EQUAL",
            KcNumLock => "KC_NUM",

            KcAudioMute => "KC_MUTE",
            KcAudioVolUp => "KC_VOLU",
            KcAudioVolDown => "KC_VOLD",
            KcMediaPlayPause => "KC_MPLY",
            KcMediaNext => "KC_MNXT",
            KcMediaPrev => "KC_MPRV",
            KcMediaStop => "KC_MSTP",

            KcSystemPower => "KC_PWR",
            KcSystemSleep => "KC_SLEP",
            KcSystemWake => "KC_WAKE",

            KcMsUp => "KC_MS_U",
            KcMsDown => "KC_MS_D",
            KcMsLeft => "KC_MS_L",
            KcMsRight => "KC_MS_R",
            KcMsBtn1 => "KC_BTN1",
            KcMsBtn2 => "KC_BTN2",
            KcMsBtn3 => "KC_BTN3",
            KcMsWhUp => "KC_WH_U",
            KcMsWhDown => "KC_WH_D",
            KcMsWhLeft => "KC_WH_L",
            KcMsWhRight => "KC_WH_R",

            KcRgbToggle => "RGB_TOG",
            KcRgbModeForward => "RGB_MOD",
            KcRgbModeReverse => "RGB_RMOD",
            KcRgbHueUp => "RGB_HUI",
            KcRgbHueDown => "RGB_HUD",
            KcRgbSatUp => "RGB_SAI",
            KcRgbSatDown => "RGB_SAD",
            KcRgbValUp => "RGB_VAI",
            KcRgbValDown => "RGB_VAD",

            KcBootloader => "QK_BOOT",
            KcReset => "RESET",

            Other(s) => return Cow::Owned(s.clone()),
        };
        Cow::Borrowed(s)
    }

    /// Parses a string into a [`Keycode`], accepting both long and short forms
    /// as well as the bare form (without the `KC_` prefix).
    ///
    /// Unknown strings produce [`Keycode::Other`] preserving the original
    /// spelling, so they round-trip through serialization.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        // Strip optional KC_ prefix for the bare-form lookup, but keep the
        // original around so Other(_) preserves the user's spelling.
        let upper = s.to_ascii_uppercase();
        let bare = upper.strip_prefix("KC_").unwrap_or(&upper);

        use Keycode::*;
        let parsed: Option<Keycode> = match bare {
            "NO" => Some(KcNo),
            "TRNS" | "TRANSPARENT" => Some(KcTransparent),

            "A" => Some(KcA),
            "B" => Some(KcB),
            "C" => Some(KcC),
            "D" => Some(KcD),
            "E" => Some(KcE),
            "F" => Some(KcF),
            "G" => Some(KcG),
            "H" => Some(KcH),
            "I" => Some(KcI),
            "J" => Some(KcJ),
            "K" => Some(KcK),
            "L" => Some(KcL),
            "M" => Some(KcM),
            "N" => Some(KcN),
            "O" => Some(KcO),
            "P" => Some(KcP),
            "Q" => Some(KcQ),
            "R" => Some(KcR),
            "S" => Some(KcS),
            "T" => Some(KcT),
            "U" => Some(KcU),
            "V" => Some(KcV),
            "W" => Some(KcW),
            "X" => Some(KcX),
            "Y" => Some(KcY),
            "Z" => Some(KcZ),

            "1" => Some(Kc1),
            "2" => Some(Kc2),
            "3" => Some(Kc3),
            "4" => Some(Kc4),
            "5" => Some(Kc5),
            "6" => Some(Kc6),
            "7" => Some(Kc7),
            "8" => Some(Kc8),
            "9" => Some(Kc9),
            "0" => Some(Kc0),

            "F1" => Some(KcF1),
            "F2" => Some(KcF2),
            "F3" => Some(KcF3),
            "F4" => Some(KcF4),
            "F5" => Some(KcF5),
            "F6" => Some(KcF6),
            "F7" => Some(KcF7),
            "F8" => Some(KcF8),
            "F9" => Some(KcF9),
            "F10" => Some(KcF10),
            "F11" => Some(KcF11),
            "F12" => Some(KcF12),
            "F13" => Some(KcF13),
            "F14" => Some(KcF14),
            "F15" => Some(KcF15),
            "F16" => Some(KcF16),
            "F17" => Some(KcF17),
            "F18" => Some(KcF18),
            "F19" => Some(KcF19),
            "F20" => Some(KcF20),
            "F21" => Some(KcF21),
            "F22" => Some(KcF22),
            "F23" => Some(KcF23),
            "F24" => Some(KcF24),

            "GRV" | "GRAVE" => Some(KcGrave),
            "MINS" | "MINUS" => Some(KcMinus),
            "EQL" | "EQUAL" => Some(KcEqual),
            "LBRC" | "LBRACKET" | "LEFT_BRACKET" => Some(KcLbracket),
            "RBRC" | "RBRACKET" | "RIGHT_BRACKET" => Some(KcRbracket),
            "BSLS" | "BSLASH" | "BACKSLASH" => Some(KcBslash),
            "SCLN" | "SEMICOLON" => Some(KcSemicolon),
            "QUOT" | "QUOTE" => Some(KcQuote),
            "COMM" | "COMMA" => Some(KcComma),
            "DOT" => Some(KcDot),
            "SLSH" | "SLASH" => Some(KcSlash),

            "EXLM" | "EXCLAIM" => Some(KcExclaim),
            "AT" => Some(KcAt),
            "HASH" => Some(KcHash),
            "DLR" | "DOLLAR" => Some(KcDollar),
            "PERC" | "PERCENT" => Some(KcPercent),
            "CIRC" | "CIRCUMFLEX" => Some(KcCircumflex),
            "AMPR" | "AMPERSAND" => Some(KcAmpersand),
            "ASTR" | "ASTERISK" => Some(KcAsterisk),
            "LPRN" | "LEFT_PAREN" => Some(KcLparen),
            "RPRN" | "RIGHT_PAREN" => Some(KcRparen),
            "COLN" | "COLON" => Some(KcColon),
            "LCBR" | "LEFT_CURLY_BRACE" => Some(KcLcurly),
            "RCBR" | "RIGHT_CURLY_BRACE" => Some(KcRcurly),
            "PLUS" => Some(KcPlus),

            "LEFT" => Some(KcLeft),
            "RIGHT" => Some(KcRight),
            "UP" => Some(KcUp),
            "DOWN" => Some(KcDown),
            "HOME" => Some(KcHome),
            "END" => Some(KcEnd),
            "PGUP" | "PAGE_UP" => Some(KcPgup),
            "PGDN" | "PAGE_DOWN" => Some(KcPgdn),

            "ENT" | "ENTER" => Some(KcEnter),
            "ESC" | "ESCAPE" => Some(KcEscape),
            "BSPC" | "BACKSPACE" => Some(KcBspc),
            "TAB" => Some(KcTab),
            "SPC" | "SPACE" => Some(KcSpace),
            "DEL" | "DELETE" => Some(KcDelete),
            "INS" | "INSERT" => Some(KcInsert),
            "CAPS" | "CAPS_LOCK" | "CAPSLOCK" => Some(KcCapsLock),
            "PSCR" | "PRINT_SCREEN" | "PRINTSCREEN" => Some(KcPrintScreen),
            "SCRL" | "SCROLL_LOCK" | "SCROLLLOCK" => Some(KcScrollLock),
            "PAUS" | "PAUSE" => Some(KcPause),

            "LCTL" | "LCTRL" | "LEFT_CTRL" | "LEFT_CONTROL" => Some(KcLctl),
            "LSFT" | "LSHIFT" | "LEFT_SHIFT" => Some(KcLsft),
            "LALT" | "LEFT_ALT" => Some(KcLalt),
            "LGUI" | "LEFT_GUI" => Some(KcLgui),
            "RCTL" | "RCTRL" | "RIGHT_CTRL" | "RIGHT_CONTROL" => Some(KcRctl),
            "RSFT" | "RSHIFT" | "RIGHT_SHIFT" => Some(KcRsft),
            "RALT" | "RIGHT_ALT" => Some(KcRalt),
            "RGUI" | "RIGHT_GUI" => Some(KcRgui),

            "KP_0" | "P0" => Some(KcKp0),
            "KP_1" | "P1" => Some(KcKp1),
            "KP_2" | "P2" => Some(KcKp2),
            "KP_3" | "P3" => Some(KcKp3),
            "KP_4" | "P4" => Some(KcKp4),
            "KP_5" | "P5" => Some(KcKp5),
            "KP_6" | "P6" => Some(KcKp6),
            "KP_7" | "P7" => Some(KcKp7),
            "KP_8" | "P8" => Some(KcKp8),
            "KP_9" | "P9" => Some(KcKp9),
            "KP_DOT" | "PDOT" => Some(KcKpDot),
            "KP_PLUS" | "PPLS" => Some(KcKpPlus),
            "KP_MINUS" | "PMNS" => Some(KcKpMinus),
            "KP_ASTERISK" | "PAST" => Some(KcKpAsterisk),
            "KP_SLASH" | "PSLS" => Some(KcKpSlash),
            "KP_ENTER" | "PENT" => Some(KcKpEnter),
            "KP_EQUAL" | "PEQL" => Some(KcKpEqual),
            "NUM" | "NUM_LOCK" | "NUMLOCK" | "NLCK" => Some(KcNumLock),

            "MUTE" | "AUDIO_MUTE" => Some(KcAudioMute),
            "VOLU" | "AUDIO_VOL_UP" => Some(KcAudioVolUp),
            "VOLD" | "AUDIO_VOL_DOWN" => Some(KcAudioVolDown),
            "MPLY" | "MEDIA_PLAY_PAUSE" => Some(KcMediaPlayPause),
            "MNXT" | "MEDIA_NEXT_TRACK" | "MEDIA_NEXT" => Some(KcMediaNext),
            "MPRV" | "MEDIA_PREV_TRACK" | "MEDIA_PREV" => Some(KcMediaPrev),
            "MSTP" | "MEDIA_STOP" => Some(KcMediaStop),

            "PWR" | "SYSTEM_POWER" => Some(KcSystemPower),
            "SLEP" | "SYSTEM_SLEEP" => Some(KcSystemSleep),
            "WAKE" | "SYSTEM_WAKE" => Some(KcSystemWake),

            "MS_U" | "MS_UP" | "MOUSE_UP" => Some(KcMsUp),
            "MS_D" | "MS_DOWN" | "MOUSE_DOWN" => Some(KcMsDown),
            "MS_L" | "MS_LEFT" | "MOUSE_LEFT" => Some(KcMsLeft),
            "MS_R" | "MS_RIGHT" | "MOUSE_RIGHT" => Some(KcMsRight),
            "BTN1" | "MS_BTN1" | "MOUSE_BTN1" => Some(KcMsBtn1),
            "BTN2" | "MS_BTN2" | "MOUSE_BTN2" => Some(KcMsBtn2),
            "BTN3" | "MS_BTN3" | "MOUSE_BTN3" => Some(KcMsBtn3),
            "WH_U" | "MS_WH_UP" | "MOUSE_WH_UP" => Some(KcMsWhUp),
            "WH_D" | "MS_WH_DOWN" | "MOUSE_WH_DOWN" => Some(KcMsWhDown),
            "WH_L" | "MS_WH_LEFT" | "MOUSE_WH_LEFT" => Some(KcMsWhLeft),
            "WH_R" | "MS_WH_RIGHT" | "MOUSE_WH_RIGHT" => Some(KcMsWhRight),

            _ => None,
        };

        if let Some(kc) = parsed {
            return kc;
        }

        // RGB and QMK-specific names live outside the KC_ namespace, so try
        // them against the original (uppercased) string. Long-form names
        // (RGB_MODE_FORWARD, RGB_MODE_REVERSE) collapse to the short form.
        match upper.as_str() {
            "RGB_TOG" | "RGB_TOGGLE" => KcRgbToggle,
            "RGB_MOD" | "RGB_MODE_FORWARD" | "RGB_MODEFORWARD" => KcRgbModeForward,
            "RGB_RMOD" | "RGB_MODE_REVERSE" | "RGB_MODEREVERSE" => KcRgbModeReverse,
            "RGB_HUI" | "RGB_HUE_INCREASE" => KcRgbHueUp,
            "RGB_HUD" | "RGB_HUE_DECREASE" => KcRgbHueDown,
            "RGB_SAI" | "RGB_SAT_INCREASE" => KcRgbSatUp,
            "RGB_SAD" | "RGB_SAT_DECREASE" => KcRgbSatDown,
            "RGB_VAI" | "RGB_VAL_INCREASE" => KcRgbValUp,
            "RGB_VAD" | "RGB_VAL_DECREASE" => KcRgbValDown,
            "QK_BOOT" | "KC_BOOTLOADER" | "BOOTLOADER" => KcBootloader,
            "RESET" | "KC_RESET" => KcReset,
            _ => Keycode::Other(s.to_string()),
        }
    }

    /// True for the high-frequency editing keys that the `lt-on-high-freq`
    /// lint warns against using as layer-tap or mod-tap holds.
    pub fn is_high_frequency(&self) -> bool {
        matches!(
            self,
            Keycode::KcBspc
                | Keycode::KcDelete
                | Keycode::KcEnter
                | Keycode::KcSpace
                | Keycode::KcTab
                | Keycode::KcEscape
        )
    }

    /// True for the eight base modifier keycodes (LCTL/LSFT/LALT/LGUI and
    /// their right-hand counterparts).
    pub fn is_modifier(&self) -> bool {
        matches!(
            self,
            Keycode::KcLctl
                | Keycode::KcLsft
                | Keycode::KcLalt
                | Keycode::KcLgui
                | Keycode::KcRctl
                | Keycode::KcRsft
                | Keycode::KcRalt
                | Keycode::KcRgui
        )
    }

    /// True for the 26 alphabetic keycodes `KC_A`..`KC_Z`.
    pub fn is_alpha(&self) -> bool {
        matches!(
            self,
            Keycode::KcA
                | Keycode::KcB
                | Keycode::KcC
                | Keycode::KcD
                | Keycode::KcE
                | Keycode::KcF
                | Keycode::KcG
                | Keycode::KcH
                | Keycode::KcI
                | Keycode::KcJ
                | Keycode::KcK
                | Keycode::KcL
                | Keycode::KcM
                | Keycode::KcN
                | Keycode::KcO
                | Keycode::KcP
                | Keycode::KcQ
                | Keycode::KcR
                | Keycode::KcS
                | Keycode::KcT
                | Keycode::KcU
                | Keycode::KcV
                | Keycode::KcW
                | Keycode::KcX
                | Keycode::KcY
                | Keycode::KcZ
        )
    }

    /// True for vowels (A, E, I, O, U, Y) — used by the `mod-tap-on-vowel`
    /// lint, which warns against putting mod-taps on the home-row vowels of
    /// alphabet-heavy layouts.
    pub fn is_vowel(&self) -> bool {
        matches!(
            self,
            Keycode::KcA | Keycode::KcE | Keycode::KcI | Keycode::KcO | Keycode::KcU | Keycode::KcY
        )
    }

    /// True if this keycode has a known variant (i.e. is not [`Keycode::Other`]).
    pub fn is_known(&self) -> bool {
        !matches!(self, Keycode::Other(_))
    }
}

impl fmt::Display for Keycode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical_name())
    }
}

impl Serialize for Keycode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.canonical_name())
    }
}

impl<'de> Deserialize<'de> for Keycode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeycodeVisitor;

        impl<'de> Visitor<'de> for KeycodeVisitor {
            type Value = Keycode;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a QMK keycode string (e.g. \"KC_A\", \"KC_BSPC\")")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Keycode, E> {
                Ok(Keycode::from_str(v))
            }

            fn visit_string<E: de::Error>(self, v: String) -> Result<Keycode, E> {
                Ok(Keycode::from_str(&v))
            }
        }

        deserializer.deserialize_str(KeycodeVisitor)
    }
}

// =============================================================================
// Modifier
// =============================================================================

/// A modifier slot used by mod-tap and mod-combo keycodes (e.g. `LCTL_T(KC_A)`).
///
/// Includes the eight base modifiers plus two combo shortcuts:
/// - [`Modifier::Hypr`] = LCTL + LSFT + LALT + LGUI
/// - [`Modifier::Meh`]  = LCTL + LSFT + LALT
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modifier {
    Lctl,
    Lsft,
    Lalt,
    Lgui,
    Rctl,
    Rsft,
    Ralt,
    Rgui,
    /// All four left-hand modifiers (CTL+SFT+ALT+GUI).
    Hypr,
    /// CTL+SFT+ALT (the "Meh" combo).
    Meh,
}

impl Modifier {
    /// Returns the canonical mnemonic without a `KC_` prefix — these are the
    /// fragments that appear inside `LCTL_T(...)`, `MEH(...)`, etc.
    pub fn canonical_name(&self) -> &'static str {
        match self {
            Modifier::Lctl => "LCTL",
            Modifier::Lsft => "LSFT",
            Modifier::Lalt => "LALT",
            Modifier::Lgui => "LGUI",
            Modifier::Rctl => "RCTL",
            Modifier::Rsft => "RSFT",
            Modifier::Ralt => "RALT",
            Modifier::Rgui => "RGUI",
            Modifier::Hypr => "HYPR",
            Modifier::Meh => "MEH",
        }
    }

    /// Parses a modifier mnemonic, accepting both QMK shorthand (`LCTL`) and
    /// the longer English forms (`LCTRL`, `LSHIFT`, `LALT`, `LGUI`).
    ///
    /// Returns `None` for unknown inputs.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        let upper = s.to_ascii_uppercase();
        let bare = upper.strip_prefix("KC_").unwrap_or(&upper);
        Some(match bare {
            "LCTL" | "LCTRL" | "LEFT_CTRL" | "LEFT_CONTROL" => Modifier::Lctl,
            "LSFT" | "LSHIFT" | "LEFT_SHIFT" => Modifier::Lsft,
            "LALT" | "LEFT_ALT" => Modifier::Lalt,
            "LGUI" | "LEFT_GUI" => Modifier::Lgui,
            "RCTL" | "RCTRL" | "RIGHT_CTRL" | "RIGHT_CONTROL" => Modifier::Rctl,
            "RSFT" | "RSHIFT" | "RIGHT_SHIFT" => Modifier::Rsft,
            "RALT" | "RIGHT_ALT" => Modifier::Ralt,
            "RGUI" | "RIGHT_GUI" => Modifier::Rgui,
            "HYPR" | "HYPER" => Modifier::Hypr,
            "MEH" => Modifier::Meh,
            _ => return None,
        })
    }
}

impl fmt::Display for Modifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.canonical_name())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant we want to round-trip through `canonical_name -> from_str`.
    /// Excludes `Other(_)` because its round-trip semantics are tested
    /// separately.
    fn all_known_variants() -> Vec<Keycode> {
        use Keycode::*;
        vec![
            KcNo,
            KcTransparent,
            KcA,
            KcB,
            KcC,
            KcD,
            KcE,
            KcF,
            KcG,
            KcH,
            KcI,
            KcJ,
            KcK,
            KcL,
            KcM,
            KcN,
            KcO,
            KcP,
            KcQ,
            KcR,
            KcS,
            KcT,
            KcU,
            KcV,
            KcW,
            KcX,
            KcY,
            KcZ,
            Kc1,
            Kc2,
            Kc3,
            Kc4,
            Kc5,
            Kc6,
            Kc7,
            Kc8,
            Kc9,
            Kc0,
            KcF1,
            KcF2,
            KcF3,
            KcF4,
            KcF5,
            KcF6,
            KcF7,
            KcF8,
            KcF9,
            KcF10,
            KcF11,
            KcF12,
            KcF13,
            KcF14,
            KcF15,
            KcF16,
            KcF17,
            KcF18,
            KcF19,
            KcF20,
            KcF21,
            KcF22,
            KcF23,
            KcF24,
            KcGrave,
            KcMinus,
            KcEqual,
            KcLbracket,
            KcRbracket,
            KcBslash,
            KcSemicolon,
            KcQuote,
            KcComma,
            KcDot,
            KcSlash,
            KcExclaim,
            KcAt,
            KcHash,
            KcDollar,
            KcPercent,
            KcCircumflex,
            KcAmpersand,
            KcAsterisk,
            KcLparen,
            KcRparen,
            KcColon,
            KcLcurly,
            KcRcurly,
            KcPlus,
            KcLeft,
            KcRight,
            KcUp,
            KcDown,
            KcHome,
            KcEnd,
            KcPgup,
            KcPgdn,
            KcEnter,
            KcEscape,
            KcBspc,
            KcTab,
            KcSpace,
            KcDelete,
            KcInsert,
            KcCapsLock,
            KcPrintScreen,
            KcScrollLock,
            KcPause,
            KcLctl,
            KcLsft,
            KcLalt,
            KcLgui,
            KcRctl,
            KcRsft,
            KcRalt,
            KcRgui,
            KcKp0,
            KcKp1,
            KcKp2,
            KcKp3,
            KcKp4,
            KcKp5,
            KcKp6,
            KcKp7,
            KcKp8,
            KcKp9,
            KcKpDot,
            KcKpPlus,
            KcKpMinus,
            KcKpAsterisk,
            KcKpSlash,
            KcKpEnter,
            KcKpEqual,
            KcNumLock,
            KcAudioMute,
            KcAudioVolUp,
            KcAudioVolDown,
            KcMediaPlayPause,
            KcMediaNext,
            KcMediaPrev,
            KcMediaStop,
            KcSystemPower,
            KcSystemSleep,
            KcSystemWake,
            KcMsUp,
            KcMsDown,
            KcMsLeft,
            KcMsRight,
            KcMsBtn1,
            KcMsBtn2,
            KcMsBtn3,
            KcMsWhUp,
            KcMsWhDown,
            KcMsWhLeft,
            KcMsWhRight,
            KcRgbToggle,
            KcRgbModeForward,
            KcRgbModeReverse,
            KcRgbHueUp,
            KcRgbHueDown,
            KcRgbSatUp,
            KcRgbSatDown,
            KcRgbValUp,
            KcRgbValDown,
            KcBootloader,
            KcReset,
        ]
    }

    #[test]
    fn round_trip_all_variants() {
        for kc in all_known_variants() {
            let name = kc.canonical_name().into_owned();
            let parsed = Keycode::from_str(&name);
            assert_eq!(
                parsed, kc,
                "round-trip failed: variant {:?} -> {:?} -> {:?}",
                kc, name, parsed
            );
        }
    }

    #[test]
    fn high_frequency_keys() {
        assert!(Keycode::KcBspc.is_high_frequency());
        assert!(Keycode::KcSpace.is_high_frequency());
        assert!(Keycode::KcEnter.is_high_frequency());
        assert!(Keycode::KcDelete.is_high_frequency());
        assert!(Keycode::KcTab.is_high_frequency());
        assert!(Keycode::KcEscape.is_high_frequency());

        // Non-high-frequency examples.
        assert!(!Keycode::KcA.is_high_frequency());
        assert!(!Keycode::KcLctl.is_high_frequency());
    }

    #[test]
    fn vowels() {
        for v in [
            Keycode::KcA,
            Keycode::KcE,
            Keycode::KcI,
            Keycode::KcO,
            Keycode::KcU,
            Keycode::KcY,
        ] {
            assert!(v.is_vowel(), "{:?} should be a vowel", v);
        }
        for c in [Keycode::KcQ, Keycode::KcB, Keycode::KcC] {
            assert!(!c.is_vowel(), "{:?} should not be a vowel", c);
        }
    }

    #[test]
    fn long_and_short_forms_collapse() {
        assert_eq!(Keycode::from_str("KC_BSPC"), Keycode::KcBspc);
        assert_eq!(Keycode::from_str("KC_BACKSPACE"), Keycode::KcBspc);

        assert_eq!(Keycode::from_str("KC_ENT"), Keycode::KcEnter);
        assert_eq!(Keycode::from_str("KC_ENTER"), Keycode::KcEnter);

        assert_eq!(Keycode::from_str("KC_LCTL"), Keycode::KcLctl);
        assert_eq!(Keycode::from_str("KC_LCTRL"), Keycode::KcLctl);
        assert_eq!(Keycode::from_str("KC_LEFT_CTRL"), Keycode::KcLctl);

        assert_eq!(Keycode::from_str("KC_LSFT"), Keycode::KcLsft);
        assert_eq!(Keycode::from_str("KC_LEFT_SHIFT"), Keycode::KcLsft);
    }

    #[test]
    fn bare_form_without_kc_prefix() {
        assert_eq!(Keycode::from_str("A"), Keycode::KcA);
        assert_eq!(Keycode::from_str("BSPC"), Keycode::KcBspc);
        assert_eq!(Keycode::from_str("BACKSPACE"), Keycode::KcBspc);
    }

    #[test]
    fn unknown_keycode_falls_through_to_other() {
        let kc = Keycode::from_str("KC_FROBNICATE");
        assert_eq!(kc, Keycode::Other("KC_FROBNICATE".to_string()));
        assert!(!kc.is_known());

        // Other should preserve original spelling on round-trip.
        let name = kc.canonical_name().into_owned();
        assert_eq!(name, "KC_FROBNICATE");
        assert_eq!(Keycode::from_str(&name), kc);
    }

    #[test]
    fn known_variants_report_known() {
        assert!(Keycode::KcA.is_known());
        assert!(Keycode::KcBspc.is_known());
        assert!(!Keycode::Other("KC_WAT".into()).is_known());
    }

    #[test]
    fn modifier_classification() {
        for m in [
            Keycode::KcLctl,
            Keycode::KcLsft,
            Keycode::KcLalt,
            Keycode::KcLgui,
            Keycode::KcRctl,
            Keycode::KcRsft,
            Keycode::KcRalt,
            Keycode::KcRgui,
        ] {
            assert!(m.is_modifier(), "{:?} should be a modifier", m);
        }
        assert!(!Keycode::KcA.is_modifier());
        assert!(!Keycode::KcBspc.is_modifier());
    }

    #[test]
    fn alpha_classification() {
        for kc in all_known_variants() {
            let name = kc.canonical_name().into_owned();
            // Alpha iff name is exactly KC_<single uppercase letter>.
            let is_single_letter = name.len() == 4
                && name.starts_with("KC_")
                && name.as_bytes()[3].is_ascii_uppercase();
            assert_eq!(
                kc.is_alpha(),
                is_single_letter,
                "is_alpha mismatch for {:?}",
                kc
            );
        }
    }

    #[test]
    fn display_matches_canonical_name() {
        assert_eq!(Keycode::KcBspc.to_string(), "KC_BSPC");
        assert_eq!(Keycode::KcLctl.to_string(), "KC_LCTL");
        assert_eq!(Keycode::Other("KC_FOO".into()).to_string(), "KC_FOO");
    }

    // ---- Modifier tests ----------------------------------------------------

    #[test]
    fn modifier_parse_short_and_long() {
        assert_eq!(Modifier::from_str("LSHIFT"), Some(Modifier::Lsft));
        assert_eq!(Modifier::from_str("LCTL"), Some(Modifier::Lctl));
        assert_eq!(Modifier::from_str("LCTRL"), Some(Modifier::Lctl));
        assert_eq!(Modifier::from_str("LEFT_CTRL"), Some(Modifier::Lctl));
        assert_eq!(Modifier::from_str("LALT"), Some(Modifier::Lalt));
        assert_eq!(Modifier::from_str("LGUI"), Some(Modifier::Lgui));
        assert_eq!(Modifier::from_str("RSHIFT"), Some(Modifier::Rsft));
        assert_eq!(Modifier::from_str("HYPR"), Some(Modifier::Hypr));
        assert_eq!(Modifier::from_str("MEH"), Some(Modifier::Meh));
    }

    #[test]
    fn modifier_parse_unknown_returns_none() {
        assert_eq!(Modifier::from_str("FOO"), None);
        assert_eq!(Modifier::from_str(""), None);
    }

    #[test]
    fn modifier_canonical_names() {
        assert_eq!(Modifier::Lctl.canonical_name(), "LCTL");
        assert_eq!(Modifier::Lsft.canonical_name(), "LSFT");
        assert_eq!(Modifier::Lalt.canonical_name(), "LALT");
        assert_eq!(Modifier::Lgui.canonical_name(), "LGUI");
        assert_eq!(Modifier::Hypr.canonical_name(), "HYPR");
        assert_eq!(Modifier::Meh.canonical_name(), "MEH");
    }

    #[test]
    fn modifier_round_trip() {
        for m in [
            Modifier::Lctl,
            Modifier::Lsft,
            Modifier::Lalt,
            Modifier::Lgui,
            Modifier::Rctl,
            Modifier::Rsft,
            Modifier::Ralt,
            Modifier::Rgui,
            Modifier::Hypr,
            Modifier::Meh,
        ] {
            assert_eq!(Modifier::from_str(m.canonical_name()), Some(m));
        }
    }

    // ---- Serde tests --------------------------------------------------------

    #[test]
    fn serde_round_trip_known() {
        let kc = Keycode::KcBspc;
        let json = serde_json::to_string(&kc).unwrap();
        assert_eq!(json, "\"KC_BSPC\"");
        let back: Keycode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, kc);
    }

    #[test]
    fn serde_long_form_normalizes_to_short() {
        let back: Keycode = serde_json::from_str("\"KC_BACKSPACE\"").unwrap();
        assert_eq!(back, Keycode::KcBspc);
        // Re-serializing should yield the short form.
        let json = serde_json::to_string(&back).unwrap();
        assert_eq!(json, "\"KC_BSPC\"");
    }

    #[test]
    fn serde_unknown_round_trips_verbatim() {
        let json = "\"KC_FROBNICATE\"";
        let kc: Keycode = serde_json::from_str(json).unwrap();
        assert_eq!(kc, Keycode::Other("KC_FROBNICATE".into()));
        assert_eq!(serde_json::to_string(&kc).unwrap(), json);
    }
}
