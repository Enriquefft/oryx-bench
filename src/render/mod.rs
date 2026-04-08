//! Visualization — ASCII split-grid renderer.
//!
//! SVG rendering (via the `keymap-drawer` subprocess) is a planned
//! future addition; for now `oryx-bench show` is ASCII-only.

pub mod ascii;

#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// If true, render position names instead of keycode bindings.
    pub show_position_names: bool,
}
