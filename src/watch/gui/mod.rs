//! egui/eframe window hosting the live indicator.
//!
//! State sharing between the egui draw thread and the HID reader thread
//! is `arc_swap::ArcSwap` — lock-free on the draw path, never poisons,
//! no `.unwrap()` sprinkled through the hot loop.

pub mod indicator;
pub mod layout_view;
pub mod theme;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use arc_swap::ArcSwap;
use tokio::sync::Notify;
use tracing::info;

use crate::schema::canonical::CanonicalLayout;
use crate::schema::geometry::Geometry;

use super::hid::{self, Command, CommandSender, WatchEvent};
use super::{runtime, ConnectOptions, Snapshot};

/// Connection status the UI surfaces. Errors keep their text so the
/// user doesn't need to open a terminal to debug.
#[derive(Debug, Clone, Default)]
pub enum ConnState {
    #[default]
    Connecting,
    Live,
    Error(String),
}

/// Lock-free state shared between the egui main thread and the HID
/// reader thread. Every field is an `ArcSwap` so the draw path never
/// blocks, never poisons, and never panics on a crashed reader.
struct Shared {
    snapshot: ArcSwap<Option<Snapshot>>,
    conn: ArcSwap<ConnState>,
    last_update: ArcSwap<Option<Instant>>,
    /// Currently-pressed electrical matrix coordinates, updated from
    /// KEYDOWN/KEYUP. Stored as raw `(row, col)` because the firmware
    /// speaks matrix coords; the renderer resolves to canonical index
    /// via `Geometry::matrix_to_index`. Small enough (≤ 52 entries on
    /// the Voyager, strictly NKRO-bounded) that an `ArcSwap<Vec>` with
    /// load-clone-mutate-store beats a mutex on the draw path.
    pressed: ArcSwap<Vec<(u8, u8)>>,
    /// Current command sender, if the pump is live. `None` during
    /// (re)connect. Swapped in atomically when a handshake completes;
    /// swapped back to `None` when the pump exits. The draw thread
    /// dereferences through this; clicking a pill that arrives
    /// *during* a reconnect is a no-op by design — no optimistic
    /// updates, no queued-but-lost commands.
    command: ArcSwap<Option<CommandSender>>,
    /// Wakes async sleepers / selects when the window closes.
    shutdown: Notify,
    /// Synchronous shutdown signal for the blocking HID pump — the pump
    /// checks this between its ~100ms read windows. Cheap (relaxed
    /// atomic load) so the hot path stays allocation-free.
    shutdown_flag: AtomicBool,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            snapshot: ArcSwap::from_pointee(None),
            conn: ArcSwap::from_pointee(ConnState::default()),
            last_update: ArcSwap::from_pointee(None),
            pressed: ArcSwap::from_pointee(Vec::new()),
            command: ArcSwap::from_pointee(None),
            shutdown: Notify::new(),
            shutdown_flag: AtomicBool::new(false),
        }
    }
}

pub struct App {
    layout: CanonicalLayout,
    geometry: &'static dyn Geometry,
    shared: Arc<Shared>,
    /// Keeps the tokio runtime alive for the app's lifetime. Dropped
    /// last so in-flight task state outlives the shutdown notify.
    _runtime: tokio::runtime::Runtime,
}

impl App {
    /// Open the window and block until the user closes it.
    pub fn run(
        layout: CanonicalLayout,
        geometry: &'static dyn Geometry,
        opts: ConnectOptions,
    ) -> Result<()> {
        let shared = Arc::new(Shared::default());
        let rt = runtime()?;
        spawn_hid_task(&rt, Arc::clone(&shared), opts);

        let options = eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([960.0, 420.0])
                .with_min_inner_size([520.0, 280.0])
                .with_title("oryx-bench watch"),
            ..Default::default()
        };

        let app = Self {
            layout,
            geometry,
            shared: Arc::clone(&shared),
            _runtime: rt,
        };

        let result = eframe::run_native(
            "oryx-bench watch",
            options,
            Box::new(move |_cc| Ok(Box::new(app))),
        );
        shared.shutdown_flag.store(true, Ordering::Release);
        shared.shutdown.notify_waiters();
        result
            .map_err(|e| anyhow::anyhow!("eframe: {e}"))
            .context("running watch window")
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::apply(ctx);

        // The HID task pushes fresh state into `shared`; ask egui to
        // repaint on a timer so we pick up events without needing the
        // reader to poke the context directly. 33ms ≈ 30fps — key
        // press/release flashes feel instant at this cadence; any
        // longer and short taps alias past a single frame.
        ctx.request_repaint_after(Duration::from_millis(33));

        let snapshot_guard = self.shared.snapshot.load();
        let snapshot: Option<&Snapshot> = snapshot_guard.as_ref().as_ref();
        let conn = (**self.shared.conn.load()).clone();
        let last = **self.shared.last_update.load();
        let command_guard = self.shared.command.load();
        let command: Option<&CommandSender> = command_guard.as_ref().as_ref();
        let pressed_guard = self.shared.pressed.load();
        let pressed: &[(u8, u8)] = pressed_guard.as_slice();

        // Keyboard shortcuts: digits 0..=9 lock that layer; Esc releases
        // the currently-displayed layer's lock. Shortcut dispatch runs
        // before the panel draw so clicks and keys behave identically.
        if let Some(sender) = command {
            dispatch_shortcuts(ctx, &self.layout, snapshot, sender);
        }

        indicator::draw(
            ctx,
            &self.layout,
            self.geometry,
            snapshot,
            &conn,
            last,
            command,
            pressed,
        );
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.shared.shutdown_flag.store(true, Ordering::Release);
        self.shared.shutdown.notify_waiters();
    }
}

/// Spawn a tokio task that owns the HID reconnect loop. The actual
/// hidapi I/O happens on a `spawn_blocking` thread — tokio just
/// coordinates shutdown, backoff, and state publication.
fn spawn_hid_task(rt: &tokio::runtime::Runtime, shared: Arc<Shared>, opts: ConnectOptions) {
    let rt_handle = rt.handle().clone();
    rt.spawn(async move {
        let mut backoff = Duration::from_millis(150);
        loop {
            shared.conn.store(Arc::new(ConnState::Connecting));
            shared.command.store(Arc::new(None));
            // Drop stale pressed keys so a disconnect mid-press doesn't
            // leave a ghost highlight after reconnect.
            shared.pressed.store(Arc::new(Vec::new()));

            let open_shared = Arc::clone(&shared);
            let open_opts = opts.clone();
            let open_rt = rt_handle.clone();
            let open_fut =
                tokio::task::spawn_blocking(move || open_and_pump(open_opts, open_shared, open_rt));
            let shutdown = shared.shutdown.notified();
            tokio::pin!(open_fut, shutdown);

            let result = tokio::select! {
                biased;
                _ = &mut shutdown => return,
                joined = &mut open_fut => joined.unwrap_or_else(|_| Ok(PumpExit::Dropped)),
            };

            shared.command.store(Arc::new(None));

            match result {
                Ok(PumpExit::Dropped) => return,
                Ok(PumpExit::Disconnected) => {
                    shared
                        .conn
                        .store(Arc::new(ConnState::Error("disconnected".into())));
                    backoff = Duration::from_millis(150);
                }
                Err(e) => {
                    shared
                        .conn
                        .store(Arc::new(ConnState::Error(format!("{e:#}"))));
                }
            }

            // Backoff before retry, cancel-safe.
            let sleep = tokio::time::sleep(backoff);
            let again = shared.shutdown.notified();
            tokio::pin!(sleep, again);
            tokio::select! {
                biased;
                _ = &mut again => return,
                _ = &mut sleep => {}
            }
            backoff = (backoff * 2).min(Duration::from_secs(5));
        }
    });
}

/// Translate digit / Esc key presses into `Command` sends. Silent when
/// the window doesn't have keyboard focus or when no command sender is
/// attached. Digit keys map 1:1 to layer index (`0` → layer 0, etc.);
/// indices outside the current layout are dropped rather than sent to
/// the firmware, where they'd be interpreted as a valid lock and
/// potentially leave the keyboard in a surprising state.
fn dispatch_shortcuts(
    ctx: &egui::Context,
    layout: &CanonicalLayout,
    snapshot: Option<&Snapshot>,
    sender: &CommandSender,
) {
    let layer_count = layout.layers.len();
    ctx.input(|i| {
        const DIGIT_KEYS: [(egui::Key, u8); 10] = [
            (egui::Key::Num0, 0),
            (egui::Key::Num1, 1),
            (egui::Key::Num2, 2),
            (egui::Key::Num3, 3),
            (egui::Key::Num4, 4),
            (egui::Key::Num5, 5),
            (egui::Key::Num6, 6),
            (egui::Key::Num7, 7),
            (egui::Key::Num8, 8),
            (egui::Key::Num9, 9),
        ];
        for (key, n) in DIGIT_KEYS {
            if i.key_pressed(key) && usize::from(n) < layer_count {
                if let Err(e) = sender.send(Command::SetLayer(n)) {
                    tracing::warn!(?e, layer = n, "SetLayer dispatch failed");
                }
            }
        }
        if i.key_pressed(egui::Key::Escape) {
            // Releasing the currently-displayed layer is the right
            // default: it clears the host-driven lock without needing
            // the user to remember which index they pressed.
            if let Some(idx) = snapshot
                .and_then(|s| s.layer_idx)
                .and_then(|i| u8::try_from(i).ok())
            {
                if let Err(e) = sender.send(Command::UnsetLayer(idx)) {
                    tracing::warn!(?e, layer = idx, "UnsetLayer dispatch failed");
                }
            }
        }
    });
}

enum PumpExit {
    /// The consumer went away — stop the whole task. (We don't expect
    /// this in normal operation; the GUI keeps `Shared` alive for the
    /// whole app lifetime.)
    Dropped,
    /// The device disconnected or errored. The outer loop will retry.
    Disconnected,
}

/// Blocking: open + pump events until the device errors. Publishes
/// snapshot updates to `shared` as events arrive.
fn open_and_pump(
    opts: ConnectOptions,
    shared: Arc<Shared>,
    rt: tokio::runtime::Handle,
) -> Result<PumpExit> {
    let mut client = hid::Client::open(&opts)?;
    // Shorten per-read timeout so shutdown propagates quickly. 100ms
    // is imperceptible for layer-change latency and well below the
    // ~150ms eframe redraw budget.
    client.set_read_timeout(Duration::from_millis(100));
    // Hook up sustain-timer scheduling before we publish the command
    // sender — otherwise a sustained command issued in the tiny window
    // after publication but before attachment would lose its release
    // timer.
    client.set_runtime(rt);
    let sender = client.command_sender();
    shared.command.store(Arc::new(Some(sender)));
    let product = client.product_string.clone();
    let fw = client.firmware_version.clone();
    let proto = client.protocol_version;

    // Seed the UI with the handshake-level info; the firmware will
    // push a LAYER event shortly that fills in `layer_idx`.
    let initial = Snapshot {
        firmware_version: fw.clone(),
        keyboard_name: product.clone(),
        layer_idx: None,
        protocol_version: Some(proto),
    };
    shared.snapshot.store(Arc::new(Some(initial)));
    shared.last_update.store(Arc::new(Some(Instant::now())));
    shared.conn.store(Arc::new(ConnState::Live));
    info!(
        product = product.as_deref().unwrap_or("<unknown>"),
        fw = fw.as_deref().unwrap_or("<unknown>"),
        proto,
        "watch GUI: keyboard paired"
    );

    // Synchronous event pump. Draw thread reads `shared` lock-free.
    // Between reads we poll the shutdown flag; the read timeout is
    // ~100ms (see `hid::run_indicator`), so close propagates in <100ms.
    loop {
        if shared.shutdown_flag.load(Ordering::Acquire) {
            return Ok(PumpExit::Dropped);
        }
        match client.next_event() {
            Ok(WatchEvent::Idle) => {
                // No state change — don't touch `last_update`; stale
                // reads would mask an actually-dead device.
            }
            Ok(WatchEvent::LayerChanged(l)) => {
                let current = shared.snapshot.load().as_ref().clone().unwrap_or_default();
                let next = Snapshot {
                    layer_idx: Some(i32::from(l)),
                    ..current
                };
                shared.snapshot.store(Arc::new(Some(next)));
                shared.last_update.store(Arc::new(Some(Instant::now())));
            }
            Ok(WatchEvent::KeyDown { row, col }) => {
                let mut next = (**shared.pressed.load()).clone();
                if !next.contains(&(row, col)) {
                    next.push((row, col));
                }
                shared.pressed.store(Arc::new(next));
                shared.last_update.store(Arc::new(Some(Instant::now())));
            }
            Ok(WatchEvent::KeyUp { row, col }) => {
                let mut next = (**shared.pressed.load()).clone();
                next.retain(|&k| k != (row, col));
                shared.pressed.store(Arc::new(next));
                shared.last_update.store(Arc::new(Some(Instant::now())));
            }
            Ok(WatchEvent::Error(msg)) => {
                shared.conn.store(Arc::new(ConnState::Error(msg)));
            }
            Ok(WatchEvent::Disconnected) | Err(_) => {
                return Ok(PumpExit::Disconnected);
            }
        }
    }
}
