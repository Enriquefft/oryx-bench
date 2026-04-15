//! Headless (CLI-only) modes of `oryx-bench watch`. These paths never
//! open a window — they're for scripting, CI, and quick checks.
//!
//! Both modes drive the raw-HID client directly. `--once` is a
//! one-shot snapshot (firmware + product name + initial layer if the
//! firmware pushes one). `--layer-only` is a push-based stream: the
//! firmware emits `LAYER` events on change, we dedupe consecutive
//! identical indices and print one line per transition.

use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use tracing::info;

use crate::schema::canonical::CanonicalLayout;

use super::hid::{self, Command, WatchEvent};
use super::{blocking_runtime, ConnectOptions, Snapshot};

/// `oryx-bench watch --once`: one poll, print, exit.
///
/// Exit code 0 on success, 2 if the keyboard is unreachable, 1 otherwise.
pub fn run_once(layout: Option<&CanonicalLayout>, opts: &ConnectOptions) -> Result<ExitCode> {
    let snap = match hid::snapshot_once(opts) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e:#}");
            return Ok(ExitCode::from(2));
        }
    };
    print_snapshot_line(&snap, layout);
    Ok(ExitCode::from(0))
}

/// `oryx-bench watch --layer-only`: stream layer changes to stdout,
/// one line per change. Never draws anything. Exits cleanly on SIGINT
/// (Ctrl-C) with exit code 130. Reconnects on transport errors with
/// capped exponential backoff.
pub fn run_layer_stream(
    layout: Option<CanonicalLayout>,
    opts: &ConnectOptions,
) -> Result<ExitCode> {
    // HID I/O is synchronous; tokio is here only for `ctrl_c()` and
    // sleeping between reconnect attempts. The read thread pushes
    // events over an mpsc channel so the async side can select on
    // "next event" vs. "Ctrl-C" vs. "backoff sleep".
    let rt = blocking_runtime()?;
    rt.block_on(async move {
        let ctrlc = tokio::signal::ctrl_c();
        tokio::pin!(ctrlc);

        let mut last_layer: Option<i32> = None;
        let mut backoff = Duration::from_millis(100);

        loop {
            // Synchronous open on a worker thread so a hung device (or
            // an outright crashed udev path) doesn't block the async
            // select loop. We only keep the result channel end around.
            let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<ClientHandle>>(1);
            let opts_clone = opts.clone();
            let _open_thread = thread::spawn(move || {
                let result = open_and_start(opts_clone);
                let _ = ready_tx.send(result);
            });

            let open_fut = tokio::task::spawn_blocking(move || ready_rx.recv().ok());
            tokio::pin!(open_fut);

            let handle = tokio::select! {
                biased;
                _ = &mut ctrlc => return Ok::<_, anyhow::Error>(ExitCode::from(130)),
                joined = &mut open_fut => match joined.ok().flatten() {
                    Some(Ok(h)) => {
                        backoff = Duration::from_millis(100);
                        info!("connected to ZSA keyboard");
                        if let Some(ref name) = h.keyboard_name {
                            info!(product = %name, fw = h.firmware_version.as_deref().unwrap_or("<unknown>"), "handshake complete");
                        }
                        h
                    }
                    Some(Err(e)) => {
                        info!(error = %e, "handshake failed; retrying");
                        let sleep = tokio::time::sleep(backoff);
                        tokio::pin!(sleep);
                        tokio::select! {
                            biased;
                            _ = &mut ctrlc => return Ok(ExitCode::from(130)),
                            _ = &mut sleep => {}
                        }
                        backoff = (backoff * 2).min(Duration::from_secs(5));
                        continue;
                    }
                    None => {
                        // Worker thread dropped without sending — defensive.
                        info!("open worker disappeared; retrying");
                        continue;
                    }
                },
            };

            let ClientHandle { event_rx, keyboard_name, firmware_version: _ } = handle;

            // Drain events. `event_rx.recv()` is synchronous; we poll it
            // from a spawn_blocking so the cancel-on-ctrlc contract is
            // preserved. On channel close the worker thread has exited
            // (device gone, I/O error, …) — reconnect.
            loop {
                let rx = event_rx.clone();
                let recv_fut =
                    tokio::task::spawn_blocking(move || -> Option<WatchEvent> { rx.recv() });
                tokio::pin!(recv_fut);
                let event = tokio::select! {
                    biased;
                    _ = &mut ctrlc => return Ok(ExitCode::from(130)),
                    joined = &mut recv_fut => joined.ok().flatten(),
                };
                match event {
                    Some(WatchEvent::LayerChanged(l)) => {
                        let idx = i32::from(l);
                        if Some(idx) != last_layer {
                            last_layer = Some(idx);
                            let snap = Snapshot {
                                firmware_version: None,
                                keyboard_name: keyboard_name.clone(),
                                layer_idx: Some(idx),
                                protocol_version: None,
                            };
                            print_snapshot_line(&snap, layout.as_ref());
                        }
                    }
                    Some(WatchEvent::Error(msg)) => tracing::warn!(%msg, "device error"),
                    Some(WatchEvent::Idle) => {
                        // Reader thread never forwards Idle; defensive arm.
                    }
                    Some(WatchEvent::KeyDown { .. } | WatchEvent::KeyUp { .. }) => {
                        // `--layer-only` is explicitly scoped to layer
                        // transitions; a live keystroke stream would
                        // spam stdout and break grep-based scripts.
                    }
                    Some(WatchEvent::Disconnected) | None => {
                        info!("device disconnected; reconnecting");
                        last_layer = None;
                        break;
                    }
                }
            }
        }
    })
}

/// How long to wait for the firmware's `LAYER(n)` echo after a
/// `SET_LAYER` write before we declare the session broken. Generous
/// enough to absorb USB contention on loaded hosts; still tight enough
/// that scripts fail fast on a misbehaving device.
const SET_LAYER_CONFIRMATION: Duration = Duration::from_millis(500);

/// Upper bound for `--reset-layers` when no project layout is loaded.
/// QMK's default `MAX_LAYER` is 16 (indices 0..=15); sending one extra
/// UnsetLayer per layer is cheap and matches Keymapp's "unlock all"
/// behavior.
const RESET_LAYER_FALLBACK_COUNT: u8 = 16;

/// Outcome of the `--set-layer` flow, separate from the CLI exit code
/// so the inner loop can be unit-tested without driving a real TTY.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SetLayerOutcome {
    /// Firmware echoed the requested layer.
    Confirmed,
    /// Handshake succeeded, but no matching LAYER event arrived
    /// before the confirmation deadline.
    TimedOut,
    /// Firmware reported an ERROR frame mid-flight.
    DeviceError(String),
    /// The read pipe went dead before confirmation.
    Disconnected,
}

/// Core of [`run_set_layer`]: given an open client, issue the lock
/// command and watch for confirmation. Factored out of `run_set_layer`
/// so it can be exercised in tests against a scripted `MockTransport`.
pub(crate) fn set_layer_on_client(
    client: &mut hid::Client,
    layer: u8,
    confirmation_window: Duration,
) -> Result<SetLayerOutcome> {
    let sender = client.command_sender();
    sender.send(Command::SetLayer(layer))?;

    let deadline = std::time::Instant::now() + confirmation_window;
    loop {
        match client.next_event()? {
            WatchEvent::LayerChanged(got) if got == layer => {
                return Ok(SetLayerOutcome::Confirmed);
            }
            WatchEvent::LayerChanged(other) => {
                // The firmware may emit a stale LAYER from a prior
                // state before our lock lands; keep waiting.
                tracing::debug!(got = other, want = layer, "ignoring stale LAYER event");
            }
            WatchEvent::Idle => {}
            WatchEvent::KeyDown { .. } | WatchEvent::KeyUp { .. } => {
                // The user typing during a set-layer confirmation is
                // orthogonal to whether the lock landed — keep waiting
                // for the LAYER echo.
            }
            WatchEvent::Error(msg) => return Ok(SetLayerOutcome::DeviceError(msg)),
            WatchEvent::Disconnected => return Ok(SetLayerOutcome::Disconnected),
        }
        if std::time::Instant::now() >= deadline {
            return Ok(SetLayerOutcome::TimedOut);
        }
    }
}

/// `oryx-bench watch --set-layer N`: one-shot.
///
/// Opens the device, sends `SET_LAYER(N)`, waits for the firmware to
/// echo a `LAYER(N)` event (within [`SET_LAYER_CONFIRMATION`]), prints
/// the confirmation, exits. Exit codes:
/// * 0 — firmware confirmed the lock.
/// * 2 — could not reach the keyboard (open/handshake failure).
/// * 1 — handshake succeeded but the firmware never echoed the layer
///   within the confirmation window.
pub fn run_set_layer(layer: u8, opts: &ConnectOptions) -> Result<ExitCode> {
    let mut client = match hid::Client::open(opts) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e:#}");
            return Ok(ExitCode::from(2));
        }
    };
    // Short read timeout so the confirmation loop stays tight.
    client.set_read_timeout(Duration::from_millis(50));
    match set_layer_on_client(&mut client, layer, SET_LAYER_CONFIRMATION)? {
        SetLayerOutcome::Confirmed => {
            println!("{layer}\tlocked");
            client.disconnect();
            Ok(ExitCode::from(0))
        }
        SetLayerOutcome::TimedOut => {
            eprintln!("timed out waiting for LAYER({layer}) confirmation");
            Ok(ExitCode::from(1))
        }
        SetLayerOutcome::DeviceError(msg) => {
            eprintln!("device error: {msg}");
            Ok(ExitCode::from(1))
        }
        SetLayerOutcome::Disconnected => {
            eprintln!("keyboard disconnected before confirming layer");
            Ok(ExitCode::from(1))
        }
    }
}

/// `oryx-bench watch --reset-layers`: release every host-driven layer
/// lock. Iterates `0..layer_count` (from project layout if available,
/// otherwise [`RESET_LAYER_FALLBACK_COUNT`]) and sends `UnsetLayer(n)`
/// for each. Exits 0 on clean completion, 2 on open/handshake failure.
pub fn run_reset_layers(
    layout: Option<&CanonicalLayout>,
    opts: &ConnectOptions,
) -> Result<ExitCode> {
    let client = match hid::Client::open(opts) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e:#}");
            return Ok(ExitCode::from(2));
        }
    };
    let mut client = client;
    let count = layout
        .map(|l| l.layers.len())
        .and_then(|n| u8::try_from(n).ok())
        .unwrap_or(RESET_LAYER_FALLBACK_COUNT);
    let sender = client.command_sender();
    for n in 0..count {
        sender.send(Command::UnsetLayer(n))?;
    }
    // Drain the command queue synchronously — next_event writes all
    // pending commands before it blocks on a read.
    client.drain_commands()?;
    println!("unlocked layers 0..{count}");
    client.disconnect();
    Ok(ExitCode::from(0))
}

fn print_snapshot_line(snap: &Snapshot, layout: Option<&CanonicalLayout>) {
    match snap.layer_idx {
        Some(idx) => {
            let name = layout
                .and_then(|l| snap.layer_name(&l.layers))
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("layer {idx}"));
            let kb = snap.keyboard_name.as_deref().unwrap_or("<unknown>");
            println!("{idx}\t{name}\t{kb}");
        }
        None => {
            // Firmware hasn't reported a layer yet. Still useful to
            // surface the keyboard identity so scripts can gate on it.
            let kb = snap.keyboard_name.as_deref().unwrap_or("<unknown>");
            let fw = snap.firmware_version.as_deref().unwrap_or("-");
            println!("-\t{kb}\t{fw}");
        }
    }
}

/// What the worker thread hands back once it's paired. Events flow over
/// the channel; the handle carries metadata captured at handshake so
/// the caller can annotate printed lines without re-querying.
struct ClientHandle {
    event_rx: SharedReceiver,
    keyboard_name: Option<String>,
    firmware_version: Option<String>,
}

/// `mpsc::Receiver` isn't `Clone` and we need to move it into each
/// `spawn_blocking`. The standard workaround wraps it in `Arc<Mutex>`
/// so every task takes the lock before calling `recv`.
///
/// Poisoning note: if the lock is poisoned, some other `recv` caller
/// panicked holding it — a real failure mode of this module. We treat
/// that as the channel being dead (same effect as the sender side
/// dropping): the caller will see `None` and tear down the session.
/// Silently swallowing the poison would hide a bug; surfacing it via
/// a "disconnected" signal matches the user-visible outcome.
#[derive(Clone)]
struct SharedReceiver(std::sync::Arc<std::sync::Mutex<mpsc::Receiver<WatchEvent>>>);

impl SharedReceiver {
    fn recv(&self) -> Option<WatchEvent> {
        match self.0.lock() {
            Ok(guard) => guard.recv().ok(),
            Err(poisoned) => {
                tracing::error!("HID event channel mutex poisoned — treating session as dead");
                // Still drain if we can — poisoned mutex is recoverable.
                poisoned.into_inner().recv().ok()
            }
        }
    }
}

/// Open the device, spawn a blocking reader thread, and return a
/// handle. The reader thread owns the `hid::Client` and drops it on
/// loop exit (disconnect or I/O error).
fn open_and_start(opts: ConnectOptions) -> Result<ClientHandle> {
    let mut client = hid::Client::open(&opts)?;
    // Shorten per-read timeout so Ctrl-C propagates without waiting
    // out the handshake-sized default.
    client.set_read_timeout(Duration::from_millis(150));
    let keyboard_name = client.product_string.clone();
    let firmware_version = client.firmware_version.clone();
    let (tx, rx) = mpsc::channel::<WatchEvent>();
    let shared = SharedReceiver(std::sync::Arc::new(std::sync::Mutex::new(rx)));

    thread::spawn(move || {
        // The read loop swallows Idle ticks internally — no point pushing
        // "nothing happened" across a channel. Real events + disconnect
        // are the only things the consumer needs to see.
        loop {
            match client.next_event() {
                Ok(WatchEvent::Idle) => continue,
                Ok(ev) => {
                    if tx.send(ev).is_err() {
                        return;
                    }
                }
                Err(_) => {
                    let _ = tx.send(WatchEvent::Disconnected);
                    return;
                }
            }
        }
    });

    Ok(ClientHandle {
        event_rx: shared,
        keyboard_name,
        firmware_version,
    })
}
