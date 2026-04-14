//! Keyboard geometry trait and registry.
//!
//! Adding a new keyboard is "create one file in this directory and
//! register it in [`registry`]". See `CONTRIBUTING.md` and `voyager.rs`
//! for the reference implementation.

pub mod voyager;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stable enum identifier for the keyboards we support. The string form
/// matches Oryx's `geometry` slug exactly so the `From<&str>` and
/// `Display` impls are lossless.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometryName {
    Voyager,
    /// Forward-compat catch-all for any geometry slug we haven't catalogued.
    /// Preserves the original string so it round-trips through serde.
    #[serde(untagged)]
    Other(String),
}

impl GeometryName {
    pub fn as_str(&self) -> &str {
        match self {
            GeometryName::Voyager => "voyager",
            GeometryName::Other(s) => s.as_str(),
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "voyager" => GeometryName::Voyager,
            other => GeometryName::Other(other.to_string()),
        }
    }
}

impl std::fmt::Display for GeometryName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for GeometryName {
    fn from(s: &str) -> Self {
        Self::from_str(s)
    }
}

impl From<String> for GeometryName {
    fn from(s: String) -> Self {
        Self::from_str(&s)
    }
}

#[cfg(test)]
mod geometry_name_tests {
    use super::*;

    #[test]
    fn voyager_round_trips_through_serde() {
        let g = GeometryName::Voyager;
        let j = serde_json::to_string(&g).unwrap();
        let back: GeometryName = serde_json::from_str(&j).unwrap();
        assert_eq!(back, GeometryName::Voyager);
    }

    #[test]
    fn other_round_trips_through_serde() {
        let g = GeometryName::Other("ergodox".into());
        let j = serde_json::to_string(&g).unwrap();
        let back: GeometryName = serde_json::from_str(&j).unwrap();
        assert_eq!(back, GeometryName::Other("ergodox".into()));
    }

    #[test]
    fn from_str_voyager() {
        assert_eq!(GeometryName::from_str("voyager"), GeometryName::Voyager);
    }

    #[test]
    fn from_str_other() {
        assert_eq!(
            GeometryName::from_str("moonlander"),
            GeometryName::Other("moonlander".into())
        );
    }
}

/// DFU flash parameters for a board's bootloader. Boards whose
/// bootloader speaks the USB DFU protocol (Voyager, Moonlander)
/// expose these; boards with other protocols (e.g. halfkay) don't.
///
/// Every field maps directly to a `dfu-util` flag so the flash
/// backend can construct the command without any board-specific
/// knowledge beyond what this struct carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DfuParams {
    /// USB vendor ID the bootloader enumerates as. May differ from the
    /// normal-mode vendor ID (e.g. Moonlander boots as STM32 0x0483,
    /// runs as ZSA 0x3297).
    pub vendor_id: u16,
    /// USB product ID the bootloader enumerates as.
    pub product_id: u16,
    /// DFU alternate setting (`-a` flag). Almost always 0.
    pub alt_setting: u8,
    /// Flash start address (`-s` flag). Board-specific; for Voyager
    /// this is `0x0800_2000` (first 8 KB reserved for the bootloader).
    pub start_address: u32,
}

impl DfuParams {
    /// Format as `"VVVV:PPPP"` for dfu-util's `-d` flag.
    pub fn device_id(&self) -> String {
        format!("{:04x}:{:04x}", self.vendor_id, self.product_id)
    }

    /// Format the start address for dfu-util's `-s` flag.
    pub fn address_spec(&self) -> String {
        format!("{:#010X}:leave", self.start_address)
    }
}

/// STM32 DFU vendor ID. `wally-cli` is only compatible with boards
/// whose bootloader enumerates under this vendor.
pub const STM32_DFU_VENDOR: u16 = 0x0483;

/// A single keyboard's matrix and rendering metadata.
pub trait Geometry: Send + Sync {
    /// Stable identifier matching Oryx's `geometry` field.
    fn id(&self) -> &'static str;

    /// Human display name.
    fn display_name(&self) -> &'static str;

    /// Number of matrix keys (excludes encoders).
    fn matrix_key_count(&self) -> usize;

    /// Number of encoders. Voyager: 0. Moonlander: 2. Ergodox EZ: 1.
    fn encoder_count(&self) -> usize;

    /// Position name → index in the flat matrix array.
    fn position_to_index(&self, name: &str) -> Option<usize>;

    /// Reverse map.
    fn index_to_position(&self, index: usize) -> Option<&'static str>;

    /// Layout for the ASCII split-grid renderer.
    fn ascii_layout(&self) -> &'static GridLayout;

    /// QMK keyboard target name (e.g., "zsa/voyager").
    fn qmk_keyboard(&self) -> &'static str;

    /// Default LAYOUT() macro name for the QMK keymap.c.
    fn layout_macro(&self) -> &'static str;

    /// Mapping from QMK `LAYOUT()` macro positional argument index to
    /// the corresponding canonical (Oryx serialization) index.
    ///
    /// QMK's LAYOUT macro takes positional args in a specific physical
    /// order (typically `[L row 0][R row 0][L row 1][R row 1]...`),
    /// while Oryx serializes its `keys[]` array in
    /// `[L all rows][L thumb][R all rows][R thumb]` order. The codegen
    /// layer must permute the canonical layout into QMK arg order so
    /// the generated `keymap.c` places keys at the correct physical
    /// positions.
    ///
    /// **If this returns the identity mapping, the firmware will be
    /// physically scrambled.** Verify against the keyboard's
    /// `keyboard.json` `layouts.LAYOUT.layout[]` array.
    fn qmk_arg_order(&self) -> &'static [usize];

    /// Which hand a matrix index belongs to. Used by the `opposite_hands`
    /// chord strategy lint / renderer. `None` for thumb-cluster positions
    /// that don't belong to either half.
    fn hand(&self, index: usize) -> Option<Hand> {
        let _ = index;
        None
    }

    /// USB vendor ID this keyboard enumerates as. The flash plan
    /// surfaces this in `--dry-run` so the user can sanity-check that
    /// the device they're about to write to is actually the one we
    /// expect — pre-flight against bricking the wrong board.
    ///
    /// Hex string form (e.g. `"0x3297"`) so it matches what `lsusb`
    /// and the QMK keyboard.json files use verbatim.
    fn usb_vendor_id(&self) -> &'static str;

    /// Total flash budget for this board, in bytes. Read by the
    /// `large-firmware` lint rule and (eventually) the build cache
    /// to fail-fast on link-time overflow with a more actionable
    /// message than `arm-none-eabi-ld: section overflow`.
    fn flash_budget_bytes(&self) -> u64;

    /// DFU bootloader parameters. Boards with a DFU-capable bootloader
    /// return `Some(...)` so the flash pipeline can invoke `dfu-util`
    /// with the correct device ID and start address. Boards that use a
    /// different protocol (e.g. halfkay / Teensy) return `None`.
    ///
    /// The default is `None` so adding a new geometry that doesn't
    /// support DFU doesn't require stubbing this out.
    fn dfu_params(&self) -> Option<DfuParams> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Left,
    Right,
}

/// Physical width of a thumb key, relative to a standard 1u matrix key.
/// Used by the renderer to visually distinguish key sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbKeyWidth {
    /// Standard 1u key.
    Standard,
    /// Wide key (~1.5u).
    Wide,
}

/// A single thumb key with its matrix index and physical width.
pub struct ThumbKey {
    pub index: usize,
    pub width: ThumbKeyWidth,
}

/// Grid layout description used by the ASCII renderer.
///
/// The renderer walks `rows` in order, then the thumb clusters. Each row
/// is a slice of matrix indices — `None` entries render as empty gaps.
pub struct GridLayout {
    pub halves: u8,
    /// Rows of (left-half indices, right-half indices).
    pub rows: &'static [GridRow],
    /// Thumb clusters, rendered below the main matrix.
    pub thumb_clusters: &'static [ThumbCluster],
}

pub struct GridRow {
    pub left: &'static [Option<usize>],
    pub right: &'static [Option<usize>],
}

pub struct ThumbCluster {
    pub hand: Hand,
    pub keys: &'static [ThumbKey],
}

static REGISTRY: Lazy<HashMap<&'static str, &'static dyn Geometry>> = Lazy::new(|| {
    let mut m = HashMap::new();
    let v: &'static dyn Geometry = &voyager::Voyager;
    m.insert(v.id(), v);
    m
});

/// Look up a geometry by its Oryx `geometry` slug. Accepts both `&str`
/// and `&GeometryName` via `Into<&str>`.
pub fn get(id: &str) -> Option<&'static dyn Geometry> {
    REGISTRY.get(id).copied()
}

/// Look up via the typed enum.
pub fn get_typed(name: &GeometryName) -> Option<&'static dyn Geometry> {
    get(name.as_str())
}

/// True if the id matches a geometry we support.
pub fn is_known(id: &str) -> bool {
    REGISTRY.contains_key(id)
}

/// Comma-separated, sorted list of every supported geometry slug.
/// Used by error messages so the list of supported boards never has
/// to be retyped as a string literal at every error site — the
/// `REGISTRY` is the single source of truth and this helper
/// projects it into a user-readable form.
pub fn supported_slugs() -> String {
    let mut ids: Vec<&'static str> = REGISTRY.keys().copied().collect();
    ids.sort_unstable();
    ids.join(", ")
}
