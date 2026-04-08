//! Serde types and the canonical internal layout representation.
//!
//! Each submodule owns a single concern:
//!
//! - [`oryx`] — Oryx GraphQL JSON shape (camelCase, lossless via `extra: HashMap`)
//! - [`layout`] — local-mode `layout.toml` schema
//! - [`features`] — `overlay/features.toml` schema (Tier 1)
//! - [`kb_toml`] — project meta-config
//! - [`canonical`] — the internal representation both Oryx and local modes deserialize into
//! - [`keycode`] — finite QMK keycode catalog with an `Other(String)` catch-all
//! - [`geometry`] — Keyboard geometry trait + Voyager impl (extension point)

pub mod canonical;
pub mod features;
pub mod geometry;
pub mod kb_toml;
pub mod keycode;
pub mod layout;
pub mod naming;
pub mod oryx;
