//! `oryx-bench watch` — live runtime indicator + typing trainer.
//!
//! Two scriptable headless paths (`--once`, `--layer-only`) plus an
//! interactive GUI (eframe/egui) for the live indicator.
//!
//! Transport is raw HID: we enumerate ZSA keyboards via the QMK raw HID
//! usage page (`0xFF60` / usage `0x61`) and speak the "Oryx WebHID"
//! protocol directly — no daemon, no "enable API in Settings" step.
//! See `src/watch/hid.rs` for the wire format.

pub mod gui;
pub mod headless;
pub mod hid;

use std::time::Duration;

/// Shared connection options. Every mode (headless + GUI) accepts the
/// same knobs so scripting and interactive UX stay aligned.
#[derive(Debug, Clone)]
pub struct ConnectOptions {
    /// Optional device selector — either a HID device node path (e.g.
    /// `/dev/hidraw3`) or a keyboard serial number. `None` → open the
    /// first matching ZSA raw HID device found.
    pub device_override: Option<String>,
    /// Per-operation timeout (connect, read-during-handshake).
    pub timeout: Duration,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            device_override: None,
            timeout: Duration::from_secs(2),
        }
    }
}

/// A single poll snapshot. All GUI state flows through this type so the
/// headless and interactive paths share one schema.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// Firmware build string, as reported by `GET_FW_VERSION`.
    pub firmware_version: Option<String>,
    /// `product_string()` from the HID descriptor — this is the name
    /// the USB interface advertises (e.g. "Voyager", "Moonlander Mk I").
    pub keyboard_name: Option<String>,
    /// Latest firmware-reported active layer index, if any.
    pub layer_idx: Option<i32>,
    /// Oryx HID protocol version the keyboard reported at handshake.
    /// Exposed for diagnostics; UI only surfaces it as a footer hint.
    pub protocol_version: Option<u8>,
}

impl Snapshot {
    /// Map the int layer index to the canonical layer name when
    /// possible. `None` when no firmware layer has been reported yet
    /// or the firmware reports an index outside the layout.
    pub fn layer_name<'a>(
        &self,
        layers: &'a [crate::schema::canonical::CanonicalLayer],
    ) -> Option<&'a str> {
        let idx = usize::try_from(self.layer_idx?).ok()?;
        layers.get(idx).map(|l| l.name.as_str())
    }
}

/// Build a Tokio runtime for the GUI path. egui owns the main thread
/// and never `block_on`s, so a current-thread runtime would leave
/// spawned tasks undriven — a single-worker multi-thread runtime
/// self-drives with minimal overhead.
pub fn runtime() -> anyhow::Result<tokio::runtime::Runtime> {
    use anyhow::Context;
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_io()
        .enable_time()
        .build()
        .context("building tokio runtime for `oryx-bench watch`")
}

/// Headless paths drive the runtime themselves via `block_on`, so a
/// current-thread runtime is correct and slightly cheaper.
pub fn blocking_runtime() -> anyhow::Result<tokio::runtime::Runtime> {
    use anyhow::Context;
    tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .context("building tokio runtime for `oryx-bench watch` (headless)")
}
