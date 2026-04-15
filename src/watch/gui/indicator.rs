//! Live indicator mode.
//!
//! Renders the canonical layout at the layer the firmware currently
//! reports, plus a compact status strip (keyboard, firmware,
//! version, connection state, freshness).

use std::time::Instant;

use egui::{Align, Color32, Context, Layout, RichText};

use crate::schema::canonical::CanonicalLayout;
use crate::schema::geometry::Geometry;

use super::layout_view::{self, RenderOpts};
use super::theme;
use super::ConnState;
use crate::watch::hid::{Command, CommandSender};
use crate::watch::Snapshot;

#[allow(clippy::too_many_arguments)]
pub fn draw(
    ctx: &Context,
    layout: &CanonicalLayout,
    geometry: &dyn Geometry,
    snapshot: Option<&Snapshot>,
    conn: &ConnState,
    last_update: Option<Instant>,
    command: Option<&CommandSender>,
    pressed: &[(u8, u8)],
) {
    egui::TopBottomPanel::top("watch-header")
        .exact_height(56.0)
        .frame(egui::Frame::none().fill(theme::PANEL).inner_margin(12.0))
        .show(ctx, |ui| {
            header(ui, layout, snapshot, conn, last_update, command)
        });

    // Footer first so the central panel yields space to it. Using
    // `min_height` instead of `exact_height` avoids the panel collapsing
    // to zero on fractional DPI scales.
    egui::TopBottomPanel::bottom("watch-footer")
        .min_height(28.0)
        .frame(
            egui::Frame::none()
                .fill(theme::PANEL)
                .inner_margin(egui::Margin::symmetric(12.0, 6.0)),
        )
        .show(ctx, |ui| footer(ui, layout, snapshot));

    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(theme::BG).inner_margin(16.0))
        .show(ctx, |ui| {
            // Three distinct states:
            //   1. Firmware reported a layer — render exactly that.
            //   2. No firmware-reported layer yet (pre-pairing, or
            //      device never pushed one) — render the base layer
            //      if the layout has one, otherwise nothing. The
            //      caller's footer explains the state in words.
            //   3. Firmware reported an out-of-range layer — pass the
            //      bad index through; layout_view renders blanks and
            //      the footer shows "index out of range".
            let active = match snapshot.and_then(|s| s.layer_idx) {
                Some(i) => usize::try_from(i).ok(),
                None => (!layout.layers.is_empty()).then_some(0),
            };
            // Resolve firmware matrix coords into canonical indices via
            // the geometry. Unknown coords (matrix holes, foreign board)
            // are dropped — never guess.
            let pressed_indices: Vec<usize> = pressed
                .iter()
                .filter_map(|&(row, col)| geometry.matrix_to_index(row, col))
                .collect();
            let _rect = layout_view::draw(
                ui,
                &RenderOpts {
                    layout,
                    geometry,
                    active_layer: active,
                    highlight: &[],
                    pressed: &pressed_indices,
                },
            );
        });
}

fn header(
    ui: &mut egui::Ui,
    layout: &CanonicalLayout,
    snapshot: Option<&Snapshot>,
    conn: &ConnState,
    last_update: Option<Instant>,
    command: Option<&CommandSender>,
) {
    ui.horizontal(|ui| {
        // "live" means the raw-HID handshake completed and the keyboard
        // is paired. "connecting" covers enumeration + handshake.
        // "disconnected" carries the transport error verbatim so the
        // user doesn't need to open a terminal.
        let (dot, label, color) = match conn {
            ConnState::Connecting => ("●", "connecting…".to_string(), theme::WARN),
            ConnState::Live => ("●", "live".to_string(), theme::OK),
            ConnState::Error(e) => ("●", format!("disconnected: {e}"), theme::ERR),
        };
        ui.label(RichText::new(dot).color(color).size(16.0));
        ui.label(RichText::new(label).color(theme::MUTED));

        ui.separator();

        // Keyboard name comes from the HID product string, captured at
        // handshake. `None` here means we haven't paired yet — render
        // an em dash rather than inventing a fallback.
        let keyboard = match snapshot {
            None => "—",
            Some(s) => s.keyboard_name.as_deref().unwrap_or("—"),
        };
        ui.label(RichText::new(keyboard).strong().color(theme::TEXT));

        if let Some(fw) = snapshot.and_then(|s| s.firmware_version.as_deref()) {
            ui.label(RichText::new(format!("fw {fw}")).color(theme::MUTED));
        }

        ui.separator();

        // Layer pills. Click to lock the matching layer on; the UI
        // deliberately does *not* update its own "active" state —
        // the firmware's LAYER event is the source of truth, and we
        // reflect whatever index comes back. Pills are disabled when
        // no command sender is attached (pre-handshake / reconnect).
        let active = snapshot.and_then(|s| s.layer_idx);
        layer_pills(ui, layout, active, command);

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(&layout.title).color(theme::MUTED));
            ui.separator();
            if let Some(t) = last_update {
                let age = Instant::now().saturating_duration_since(t);
                let text = if age.as_millis() < 1500 {
                    "updated just now".to_string()
                } else {
                    format!("updated {}ms ago", age.as_millis())
                };
                ui.label(RichText::new(text).color(theme::MUTED).small());
            }
        });
    });
}

/// One clickable pill per layer in the layout. Click sends `SetLayer(n)`.
/// The pill is visually "active" only when the firmware reports that
/// index as active (snapshot.layer_idx) — never speculatively.
fn layer_pills(
    ui: &mut egui::Ui,
    layout: &CanonicalLayout,
    active_layer_idx: Option<i32>,
    command: Option<&CommandSender>,
) {
    for (idx, layer) in layout.layers.iter().enumerate() {
        let Ok(n) = u8::try_from(idx) else {
            // More than 255 layers is a layout error we don't plumb
            // down to the firmware; skip silently rather than corrupt
            // the protocol byte.
            continue;
        };
        let is_active = active_layer_idx.and_then(|i| usize::try_from(i).ok()) == Some(idx);
        let (bg, fg) = if is_active {
            (theme::OK, Color32::BLACK)
        } else {
            (theme::PANEL, theme::TEXT)
        };
        let label = RichText::new(format!("{idx} {}", layer.name))
            .color(fg)
            .monospace()
            .small();
        let button = egui::Button::new(label).fill(bg);
        let response = ui.add_enabled(command.is_some(), button);
        if response.clicked() {
            if let Some(sender) = command {
                if let Err(e) = sender.send(Command::SetLayer(n)) {
                    tracing::warn!(?e, layer = n, "SetLayer click failed");
                }
            }
        }
    }
}

fn footer(ui: &mut egui::Ui, layout: &CanonicalLayout, snapshot: Option<&Snapshot>) {
    ui.horizontal(|ui| {
        let layer_text = match snapshot.and_then(|s| s.layer_idx) {
            Some(idx) => match usize::try_from(idx).ok().and_then(|i| layout.layers.get(i)) {
                Some(l) => format!("layer {idx} · {}", l.name),
                None => format!("layer {idx} · (index out of range)"),
            },
            None => "–".into(),
        };
        ui.label(RichText::new(layer_text).color(theme::TEXT).monospace());

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new("digits lock layer · esc releases lock · close window to exit")
                    .color(Color32::from_gray(110))
                    .small(),
            );
        });
    });
}
