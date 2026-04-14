//! Direct raw-HID transport to ZSA keyboards.
//!
//! Speaks the "Oryx WebHID" protocol shipped in `zsa/qmk_modules` — same
//! wire format the ZSA keyboard firmware speaks — over QMK's raw HID channel (usage page
//! `0xFF60`, usage `0x61`). No daemon, no "enable API in Settings":
//! plug the keyboard in and open `/dev/hidraw*` directly via the
//! kernel-claimed HID interface.
//!
//! Phase 2 scope: read path from Phase 1 plus a bidirectional command
//! queue — host→device writes for layer locking, RGB, status LEDs, and
//! brightness. Commands are enqueued via [`CommandSender`]; the blocking
//! pump drains pending commands between reads, so writes never race the
//! `hidapi` read handle (which is not `Sync`). Device state mutation
//! remains authoritative — the UI observes firmware LAYER events rather
//! than updating optimistically.
//!
//! ## Wire format
//!
//! Every packet is 32 bytes. `bytes[0]` is the command / event id;
//! payload follows; padded with `0xFE` to 32 bytes. The firmware treats
//! `0xFE` as both the stop byte and the neutral padding byte. Because
//! of that, a fully-padded all-`0xFE` packet means "no command, please
//! return the protocol version" — and the `GET_PROTOCOL_VERSION`
//! command id is therefore defined as `0xFE` itself.
//!
//! ## Session flow
//!
//! 1. Enumerate HID devices with VID `0x3297` + usage page `0xFF60` /
//!    usage `0x61`. First match wins (multi-device selection: Phase 2+).
//! 2. Open the device.
//! 3. `GET_PROTOCOL_VERSION` → expect `0x04`. Warn on drift below 0x04;
//!    hard-fail on anything above (unknown newer dialect).
//! 4. `PAIRING_INIT` → read until `PAIRING_SUCCESS`. In firmware25 the
//!    success event is immediate.
//! 5. `GET_FW_VERSION` → stash the version string.
//! 6. Event loop: drain any queued [`Command`]s, then `read_timeout(32)`
//!    and dispatch by `bytes[0]`.
//!
//! ## Host-side sustain
//!
//! The firmware holds an RGB / status-LED override indefinitely until
//! the host explicitly hands control back via `RGB_CONTROL(0)` /
//! `STATUS_LED_CONTROL(0)`. To match Keymapp's "timed effect" UX we
//! implement sustain on the host side: a non-zero `sustain` on an RGB
//! or status command spawns a timer task that enqueues the matching
//! release command when it fires. A newer sustained command cancels
//! the prior timer (newer call wins).

use std::ffi::CStr;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use hidapi::{HidApi, HidDevice};
use tracing::{debug, info, warn};

use super::{ConnectOptions, Snapshot};

/// ZSA's USB vendor ID. Every ZSA keyboard runtime descriptor reports this.
pub const ZSA_VID: u16 = 0x3297;

/// QMK raw HID usage page. Matching on this (not interface number) is
/// correct because composite descriptors put it at a different index
/// per-board and per-firmware-build.
pub const RAW_USAGE_PAGE: u16 = 0xFF60;

/// QMK raw HID usage within the page.
pub const RAW_USAGE: u16 = 0x61;

/// Packet size in both directions. Fixed by the QMK report descriptor.
pub const REPORT_SIZE: usize = 32;

/// Stop / pad byte. Every field that doesn't span the full packet is
/// followed by this; the firmware treats it as end-of-payload.
pub const STOP: u8 = 0xFE;

/// Highest "Oryx WebHID" protocol version this client speaks. Drift
/// below this is best-effort; drift above hard-fails — a newer firmware
/// dialect could redefine event IDs and we'd misroute writes silently.
pub const PROTOCOL_VERSION: u8 = 0x04;

/// Command / event identifiers. The `Cmd` constants are host→device
/// request bytes; the `Evt` constants are device→host response bytes.
/// Some IDs serve both roles (GET_FW_VERSION, GET_PROTOCOL_VERSION).
pub mod wire {
    /// Request (and response) — `bytes[0]` = 0x00, followed by the
    /// firmware version string.
    pub const GET_FW_VERSION: u8 = 0x00;
    /// Request only — tell the keyboard we're an Oryx-compatible host.
    pub const PAIRING_INIT: u8 = 0x01;
    /// Request only — close the session cleanly. Device sends nothing back.
    pub const DISCONNECT: u8 = 0x03;
    /// Response only — payload-less ack that pairing is complete.
    pub const PAIRING_SUCCESS: u8 = 0x04;
    /// Async event — `[0x05, layer_index, 0xFE, ...]`.
    pub const LAYER: u8 = 0x05;
    /// Async event — `[0x06, col, row, 0xFE, ...]`.
    pub const KEYDOWN: u8 = 0x06;
    /// Async event — `[0x07, col, row, 0xFE, ...]`.
    pub const KEYUP: u8 = 0x07;
    /// Request (and response) — `bytes[0]` = 0xFE.
    pub const GET_PROTOCOL_VERSION: u8 = 0xFE;
    /// Async error — `[0xFF, code, ...]`.
    pub const ERROR: u8 = 0xFF;

    // ---- Phase 2: host→device write commands. All dual-purpose with
    // the LAYER event above (0x04 is PAIRING_SUCCESS when received, a
    // layer lock op when sent — the firmware disambiguates by direction
    // and by the subcommand byte in `bytes[1]`).

    /// Request only — `[0x04, op, layer_index, 0xFE..]`. `op == 0x01`
    /// locks the layer on; `op == 0x00` releases the lock. Matches
    /// `PAIRING_SUCCESS` on its id, but PAIRING_SUCCESS is
    /// device→host and payload-less, so there's no ambiguity in
    /// practice.
    pub const SET_LAYER: u8 = 0x04;
    /// Request only — `[0x05, on(0|1), 0xFE..]`. Hands RGB ownership
    /// to the host (1) or back to the firmware default (0).
    pub const RGB_CONTROL: u8 = 0x05;
    /// Request only — `[0x06, led, r, g, b, 0xFE..]`.
    pub const SET_RGB_LED: u8 = 0x06;
    /// Request only — `[0x07, led(0..5), on(0|1), 0xFE..]`.
    pub const SET_STATUS_LED: u8 = 0x07;
    /// Request only — `[0x08, dir(0|1), 0xFE..]`. `0` = decrease, `1`
    /// = increase.
    pub const UPDATE_BRIGHTNESS: u8 = 0x08;
    /// Request only — `[0x09, r, g, b, 0xFE..]`.
    pub const SET_RGB_LED_ALL: u8 = 0x09;
    /// Request only — `[0x0A, on(0|1), 0xFE..]`.
    pub const STATUS_LED_CONTROL: u8 = 0x0A;

    /// Layer-lock subcommand — lock the named layer on.
    pub const LAYER_OP_LOCK: u8 = 0x01;
    /// Layer-lock subcommand — release a prior lock.
    pub const LAYER_OP_UNLOCK: u8 = 0x00;

    /// Brightness direction — increase one step.
    pub const BRIGHTNESS_INCREASE: u8 = 0x01;
    /// Brightness direction — decrease one step.
    pub const BRIGHTNESS_DECREASE: u8 = 0x00;
}

/// Host→device command. Mirrors the gRPC surface Keymapp exposes so
/// future IPC layers translate 1:1. `sustain` on RGB/status commands
/// is host-side: the pump spawns a timer that enqueues the matching
/// release command when it fires (see module-level "Host-side sustain"
/// section).
///
/// Only the commands this crate actually emits are listed; every
/// variant is plumbed end-to-end (byte-level tested) even when no UI
/// surface issues it yet — so the next caller drops in cleanly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Lock a layer on. The firmware emits a `LAYER(n)` event once it
    /// takes effect; callers observe that rather than updating UI state
    /// optimistically.
    SetLayer(u8),
    /// Release a prior `SetLayer` lock on `n`.
    UnsetLayer(u8),
    /// Hand RGB ownership between host and firmware.
    /// `true` = oryx-bench drives LEDs; `false` = hand back.
    RgbControl(bool),
    /// Single LED, explicit RGB. `sustain` of zero means "hold until
    /// the next SetRgb* or RgbControl(false)"; non-zero means the pump
    /// schedules a host-side `RgbControl(false)` after that duration.
    SetRgbLed {
        led: u8,
        r: u8,
        g: u8,
        b: u8,
        sustain: Duration,
    },
    /// All LEDs, explicit RGB. Sustain semantics as `SetRgbLed`.
    SetRgbAll {
        r: u8,
        g: u8,
        b: u8,
        sustain: Duration,
    },
    /// Toggle a single status LED (0..=5). Sustain semantics as
    /// `SetRgbLed`, routed through `StatusLedControl(false)`.
    SetStatusLed {
        led: u8,
        on: bool,
        sustain: Duration,
    },
    /// Increase global RGB brightness by one firmware step.
    IncreaseBrightness,
    /// Decrease global RGB brightness by one firmware step.
    DecreaseBrightness,
    /// Hand status-LED ownership between host and firmware.
    StatusLedControl(bool),
}

/// Write-side failure surface. Disjoint from [`HidOpenError`] — those
/// are enumeration / handshake problems; these fire after the handshake
/// succeeded and we're pumping events.
#[derive(Debug, thiserror::Error)]
pub enum HidWriteError {
    /// Transport returned an error while writing a command frame.
    #[error("HID write failed for command 0x{cmd:02X}: {source}")]
    Io {
        cmd: u8,
        #[source]
        source: hidapi::HidError,
    },
    /// Command channel has no live pump on the receiving end. Either
    /// the pump died (transport error) or it has shut down. Surfaces
    /// as an error rather than a panic so UI paths can reconnect.
    #[error("HID command channel closed — pump thread is gone")]
    PumpGone,
}

/// Handle the UI and headless paths use to enqueue commands. Cheap to
/// clone. Dropping all senders tears down the pump's command side —
/// the pump keeps running and continues reading events.
#[derive(Debug, Clone)]
pub struct CommandSender {
    tx: std::sync::mpsc::Sender<Command>,
    /// Shared sustain state — tracked here so the UI layer can spawn
    /// tokio timers on its runtime when issuing sustained commands.
    /// The timer, when it fires, posts a release command back to
    /// `tx`.
    sustain: Arc<SustainState>,
}

impl CommandSender {
    /// Enqueue a command for the pump. Returns [`HidWriteError::PumpGone`]
    /// if the pump dropped its receiver (transport died, client torn
    /// down). Non-blocking.
    pub fn send(&self, cmd: Command) -> Result<(), HidWriteError> {
        self.tx.send(cmd).map_err(|_| HidWriteError::PumpGone)
    }

    /// Schedule a sustain release on the supplied tokio runtime. Called
    /// by the pump, not by UI code, after it successfully writes a
    /// sustain-bearing command. Cancels any prior in-flight timer for
    /// the same sustain *channel* (rgb / status) so a newer call wins.
    ///
    /// **Invariant: pump-thread-only.** The `fetch_add` → per-channel
    /// `store` sequence is only safe if these two steps happen
    /// atomically with respect to *other* schedulers. We enforce that
    /// by serializing: the only caller is `execute_command`, which
    /// runs on the blocking pump thread. If a future caller exposes
    /// `schedule_sustain` to other threads, wrap the two stores in a
    /// critical section or pre-order the ops with a CAS on the
    /// per-channel gen.
    fn schedule_sustain(
        &self,
        channel: SustainChannel,
        dur: Duration,
        rt: &tokio::runtime::Handle,
    ) {
        if dur.is_zero() {
            return;
        }
        let sustain = Arc::clone(&self.sustain);
        let gen = sustain.next_gen.fetch_add(1, Ordering::AcqRel) + 1;
        // Install our generation *before* spawning so a timer that
        // fires immediately still sees the right generation.
        match channel {
            SustainChannel::Rgb => sustain.rgb.store(gen, Ordering::Release),
            SustainChannel::Status => sustain.status.store(gen, Ordering::Release),
        }
        let tx = self.tx.clone();
        rt.spawn(async move {
            tokio::time::sleep(dur).await;
            let current = match channel {
                SustainChannel::Rgb => sustain.rgb.load(Ordering::Acquire),
                SustainChannel::Status => sustain.status.load(Ordering::Acquire),
            };
            // Cancelled? A newer schedule stored a higher gen.
            if current != gen {
                return;
            }
            let release = match channel {
                SustainChannel::Rgb => Command::RgbControl(false),
                SustainChannel::Status => Command::StatusLedControl(false),
            };
            if tx.send(release).is_err() {
                // Pump is gone; sustain release is moot.
                debug!("sustain timer fired but pump channel closed");
            }
        });
    }
}

/// The two sustain "channels" — RGB effects and status-LED effects.
/// A newer command on the same channel preempts a pending timer; the
/// two channels are independent (an RGB sustain doesn't cancel a
/// status sustain or vice versa).
#[derive(Debug, Clone, Copy)]
enum SustainChannel {
    Rgb,
    Status,
}

/// Shared generation counters used to cancel superseded sustain
/// timers without touching the pump lock. A monotonically increasing
/// counter (one per channel) wins over the "keep a JoinHandle, abort
/// the old one" approach: it's allocation-free in the hot path and
/// composes cleanly across cloned `CommandSender`s.
#[derive(Debug, Default)]
struct SustainState {
    next_gen: AtomicU64,
    rgb: AtomicU64,
    status: AtomicU64,
}

/// Minimum non-zero sustain. Below this, the release command would
/// race the initial write and look like "did nothing". Named rather
/// than magic-number-inlined so future tuning has a single edit site.
pub const MIN_SUSTAIN: Duration = Duration::from_millis(10);

/// A parsed firmware→host event. Everything the read-loop might pull
/// off the wire is enumerated here; the dispatcher maps raw bytes to
/// this type so callers never see the wire format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    ProtocolVersion(u8),
    FirmwareVersion(String),
    PairingSuccess,
    LayerChanged(u8),
    KeyDown {
        col: u8,
        row: u8,
    },
    KeyUp {
        col: u8,
        row: u8,
    },
    Error {
        code: u8,
    },
    /// Unknown event byte. Surfaced so the caller can log & continue —
    /// a future firmware might push event types we don't parse yet.
    Unknown {
        bytes: [u8; REPORT_SIZE],
    },
}

/// High-level event surfaced to GUI / headless consumers. Intentionally
/// narrower than `Event`: transport-level plumbing (PairingSuccess,
/// ProtocolVersion) is handled inside this module during handshake and
/// never propagates up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    /// Firmware reported a new active layer index.
    LayerChanged(u8),
    /// Device sent an error frame. `String` is a human-readable
    /// description (we keep the raw code too for logs).
    Error(String),
    /// `read_timeout` returned no data during the configured window —
    /// not an error, but useful to the UI for freshness indicators.
    Idle,
    /// Read loop exited (EOF, broken pipe, user requested shutdown).
    Disconnected,
}

/// Typed errors from the HID path. The CLI wraps these with `anyhow`;
/// the enum exists so unit tests can assert on specific failure modes
/// (unknown protocol version, permission denied, …) without string
/// matching.
#[derive(Debug, thiserror::Error)]
pub enum HidOpenError {
    #[error("no ZSA keyboard found (VID=0x{ZSA_VID:04X}, usage page=0x{RAW_USAGE_PAGE:04X}). is the keyboard plugged in and running your custom firmware?")]
    NotFound,
    #[error(
        "permission denied opening {path}. install the udev rules from packaging/linux/50-zsa.rules into /etc/udev/rules.d/ (on NixOS: set `hardware.keyboard.zsa.enable = true;`)"
    )]
    PermissionDenied { path: String },
    #[error(
        "keyboard responded on USB but does not speak the Oryx HID protocol. your firmware is missing the handler — rebuild with COMMUNITY_MODULES += oryx, RAW_ENABLE = yes, and RGB_MATRIX_ENABLE = yes (or use the stock ZSA firmware)"
    )]
    FirmwareHandlerMissing,
    #[error("failed to open {path}: {source}")]
    Open {
        path: String,
        #[source]
        source: hidapi::HidError,
    },
    #[error("hidapi init failed: {0}")]
    Init(#[source] hidapi::HidError),
    #[error(
        "keyboard reports Oryx HID protocol 0x{got:02X}, but we only know up to 0x{ours:02X}. upgrade oryx-bench or downgrade the firmware."
    )]
    UnknownProtocolVersion { got: u8, ours: u8 },
    #[error("pairing handshake failed: {0}")]
    Pairing(String),
    #[error("HID I/O error: {0}")]
    Io(#[source] hidapi::HidError),
}

/// Parse the 32-byte packet at `frame` into a typed `Event`.
fn decode_event(frame: &[u8; REPORT_SIZE]) -> Event {
    match frame[0] {
        wire::GET_FW_VERSION => {
            // Firmware responds with a null/stop-terminated ASCII string
            // starting at frame[1]. We strip both 0x00 and 0xFE.
            let end = frame
                .iter()
                .skip(1)
                .position(|&b| b == STOP || b == 0x00)
                .map(|p| p + 1)
                .unwrap_or(REPORT_SIZE);
            let raw = &frame[1..end];
            let s = String::from_utf8_lossy(raw).to_string();
            Event::FirmwareVersion(s)
        }
        wire::PAIRING_SUCCESS => Event::PairingSuccess,
        wire::LAYER => Event::LayerChanged(frame[1]),
        wire::KEYDOWN => Event::KeyDown {
            col: frame[1],
            row: frame[2],
        },
        wire::KEYUP => Event::KeyUp {
            col: frame[1],
            row: frame[2],
        },
        wire::ERROR => Event::Error { code: frame[1] },
        wire::GET_PROTOCOL_VERSION => Event::ProtocolVersion(frame[1]),
        _ => Event::Unknown { bytes: *frame },
    }
}

/// Abstracts the HID transport so the protocol state machine can be
/// unit-tested against a scripted mock without touching hidapi. Real
/// builds use `HidDeviceTransport`, tests use `MockTransport`.
pub trait Transport: Send {
    /// Write a single `REPORT_SIZE`-byte frame. The implementation is
    /// responsible for any Report ID byte the underlying API requires.
    fn write_frame(&mut self, frame: &[u8; REPORT_SIZE]) -> Result<(), hidapi::HidError>;
    /// Read one frame, blocking up to `timeout_ms` milliseconds. `-1`
    /// for an unbounded wait. Returns `Ok(None)` on timeout.
    fn read_frame(
        &mut self,
        timeout_ms: i32,
    ) -> Result<Option<[u8; REPORT_SIZE]>, hidapi::HidError>;

    /// Test-only introspection: every frame the transport has been
    /// asked to write, in order. Real transports return an empty
    /// slice; mocks return their scratch buffer. Lets tests assert
    /// on the exact byte sequence without downcasting the boxed
    /// trait object. `#[cfg(test)]` keeps this out of release builds.
    #[cfg(test)]
    fn recorded_writes(&self) -> &[[u8; REPORT_SIZE]] {
        &[]
    }

    /// Test-only: enqueue a scripted read response. Real transports
    /// no-op. Used by tests that drive the client after handshake.
    #[cfg(test)]
    fn queue_scripted_read(&mut self, _frame: [u8; REPORT_SIZE]) {}
}

struct HidDeviceTransport {
    device: HidDevice,
}

impl Transport for HidDeviceTransport {
    fn write_frame(&mut self, frame: &[u8; REPORT_SIZE]) -> Result<(), hidapi::HidError> {
        let mut buf = [STOP; REPORT_SIZE + 1];
        buf[0] = 0x00;
        buf[1..].copy_from_slice(frame);
        self.device.write(&buf)?;
        Ok(())
    }

    fn read_frame(
        &mut self,
        timeout_ms: i32,
    ) -> Result<Option<[u8; REPORT_SIZE]>, hidapi::HidError> {
        let mut buf = [0u8; REPORT_SIZE];
        let n = self.device.read_timeout(&mut buf, timeout_ms)?;
        if n == 0 {
            Ok(None)
        } else {
            // Pad short reads with the stop byte so the decoder always
            // sees a full frame. In practice QMK always delivers 32.
            for b in &mut buf[n..] {
                *b = STOP;
            }
            Ok(Some(buf))
        }
    }
}

/// Open, paired, and ready-to-read handle. The caller drives it through
/// `next_event` (blocking) or `snapshot_once` (one-shot).
///
/// Phase 2: the client also owns a sync `mpsc::Receiver<Command>` and a
/// shared [`SustainState`]. UI / headless callers obtain a
/// [`CommandSender`] via [`Client::command_sender`] and enqueue writes;
/// the pump drains pending commands at the top of each `next_event`
/// tick before reading from the transport. This preserves strict
/// single-threaded ownership of the `HidDevice` handle (hidapi's
/// `HidDevice: !Sync`) without a mutex on the blocking read path.
pub struct Client {
    transport: Box<dyn Transport>,
    pub firmware_version: Option<String>,
    pub product_string: Option<String>,
    pub protocol_version: u8,
    /// Per-read timeout. `ConnectOptions::timeout` controls this. Kept
    /// short enough that shutdown feels instant but long enough that
    /// we're not busy-looping.
    read_timeout: Duration,
    /// Receive end of the command queue. Owned by the pump.
    command_rx: std::sync::mpsc::Receiver<Command>,
    /// Send end retained so `command_sender()` can hand out additional
    /// senders without reopening the channel. Held in an `Option` so
    /// tests that want to close the channel early (for coverage) can
    /// drop it.
    command_tx: std::sync::mpsc::Sender<Command>,
    /// Shared sustain state co-owned by every `CommandSender` clone.
    sustain: Arc<SustainState>,
    /// Handle of the tokio runtime used for scheduling sustain timers.
    /// Optional because tests (and one-shot `snapshot_once`) may open
    /// a client without a runtime — in that mode any non-zero sustain
    /// is treated as zero with a warning, which is the *only* correct
    /// behavior: silently accepting sustain without a runtime would
    /// leave the device stuck on a host-driven effect.
    runtime: Option<tokio::runtime::Handle>,
}

impl Client {
    /// Enumerate, pick, open, and handshake. Phase 1 picks the first
    /// matching device; Phase 2 will support selection by serial.
    pub fn open(opts: &ConnectOptions) -> Result<Self> {
        let api = HidApi::new().map_err(HidOpenError::Init)?;
        let info = find_zsa_raw_hid(&api, opts.device_override.as_deref())?;
        let path = info.path().to_owned();
        let product = info.product_string().map(str::to_owned);

        let device = api
            .open_path(info.path())
            .map_err(|source| map_open_error(&path, source))?;

        info!(
            vid = format_args!("{:#06x}", info.vendor_id()),
            pid = format_args!("{:#06x}", info.product_id()),
            product = product.as_deref().unwrap_or("<unknown>"),
            "opened ZSA raw HID device"
        );

        let transport: Box<dyn Transport> = Box::new(HidDeviceTransport { device });
        Self::handshake(transport, product, opts.timeout)
    }

    /// Split out so the unit tests can feed a `MockTransport` through
    /// the exact same state machine the real client uses.
    pub(crate) fn handshake(
        mut transport: Box<dyn Transport>,
        product_string: Option<String>,
        read_timeout: Duration,
    ) -> Result<Self> {
        let timeout_ms = timeout_to_ms(read_timeout);

        // 1. Protocol version probe — best-effort, warn on drift.
        // A timeout here (device responds on USB but never answers the
        // Oryx probe) means the firmware doesn't carry the Oryx HID
        // handler. Translate that specifically; a generic "read
        // timeout" sends users chasing ghosts.
        write_command(&mut *transport, wire::GET_PROTOCOL_VERSION, &[])?;
        let protocol_version = read_until(&mut *transport, timeout_ms, |e| {
            matches!(e, Event::ProtocolVersion(_))
        })
        .map_err(|e| translate_handshake_timeout(e, HandshakeStage::Probe))?;
        let proto_v = match protocol_version {
            Event::ProtocolVersion(v) => v,
            _ => unreachable!("filter above guarantees this"),
        };
        if proto_v > PROTOCOL_VERSION {
            return Err(HidOpenError::UnknownProtocolVersion {
                got: proto_v,
                ours: PROTOCOL_VERSION,
            }
            .into());
        }
        if proto_v < PROTOCOL_VERSION {
            warn!(
                got = proto_v,
                ours = PROTOCOL_VERSION,
                "older Oryx HID protocol; proceeding best-effort"
            );
        }

        // 2. Pairing.
        write_command(&mut *transport, wire::PAIRING_INIT, &[])?;
        let _ = read_until(&mut *transport, timeout_ms, |e| {
            matches!(e, Event::PairingSuccess)
        })
        .map_err(|e| translate_handshake_timeout(e, HandshakeStage::Pairing))?;

        // 3. Firmware version.
        write_command(&mut *transport, wire::GET_FW_VERSION, &[])?;
        let fw = match read_until(&mut *transport, timeout_ms, |e| {
            matches!(e, Event::FirmwareVersion(_))
        })
        .map_err(|e| translate_handshake_timeout(e, HandshakeStage::FirmwareVersion))?
        {
            Event::FirmwareVersion(s) => Some(s),
            _ => None,
        };

        let (command_tx, command_rx) = std::sync::mpsc::channel::<Command>();
        Ok(Self {
            transport,
            firmware_version: fw,
            product_string,
            protocol_version: proto_v,
            read_timeout,
            command_rx,
            command_tx,
            sustain: Arc::new(SustainState::default()),
            runtime: None,
        })
    }

    /// Override the per-read timeout. Short values (≤100ms) make
    /// shutdown feel instant in the GUI pump; long values are fine for
    /// one-shot reads. Has no effect on writes.
    pub fn set_read_timeout(&mut self, d: Duration) {
        self.read_timeout = d;
    }

    /// Attach a tokio runtime handle for scheduling host-side sustain
    /// timers. Must be called before issuing RGB/status commands with a
    /// non-zero `sustain`, otherwise those commands will log a warning
    /// and behave as if `sustain` were zero. The GUI path attaches the
    /// watch runtime; the headless `--set-layer` / `--reset-layers`
    /// paths don't issue sustained commands and deliberately skip this.
    pub fn set_runtime(&mut self, rt: tokio::runtime::Handle) {
        self.runtime = Some(rt);
    }

    /// Hand out a fresh [`CommandSender`]. Safe to call any number of
    /// times; all senders feed the same queue.
    pub fn command_sender(&self) -> CommandSender {
        CommandSender {
            tx: self.command_tx.clone(),
            sustain: Arc::clone(&self.sustain),
        }
    }

    /// Drain any pending commands, writing them to the transport, then
    /// block for up to `read_timeout` for an event. The command drain
    /// is strict FIFO; the read happens *after* every pending command
    /// is flushed so a burst of UI clicks doesn't stack up waiting on
    /// read timeouts.
    ///
    /// A command write failure surfaces as an error (same lifetime
    /// semantics as a read error) — the pump treats the session as
    /// broken and the GUI reconnect loop takes over.
    pub fn next_event(&mut self) -> Result<WatchEvent> {
        self.drain_commands()?;
        let timeout_ms = timeout_to_ms(self.read_timeout);
        match self.transport.read_frame(timeout_ms) {
            Ok(Some(frame)) => Ok(classify(decode_event(&frame))),
            Ok(None) => Ok(WatchEvent::Idle),
            Err(e) => {
                debug!(?e, "HID read error");
                Err(HidOpenError::Io(e).into())
            }
        }
    }

    /// Drain every pending `Command` off the queue. Exposed for tests;
    /// the pump calls it at the top of every `next_event`.
    pub(crate) fn drain_commands(&mut self) -> Result<()> {
        loop {
            match self.command_rx.try_recv() {
                Ok(cmd) => self.execute_command(cmd)?,
                Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(()),
            }
        }
    }

    fn execute_command(&mut self, cmd: Command) -> Result<()> {
        let (cmd_id, payload, sustain) = encode_command(&cmd);
        write_command_typed(&mut *self.transport, cmd_id, &payload)?;
        if let Some((channel, dur)) = sustain {
            match self.runtime.as_ref() {
                Some(rt) => {
                    let sender = self.command_sender();
                    sender.schedule_sustain(channel, dur, rt);
                }
                None => {
                    warn!(
                        ?cmd,
                        "sustain requested but no runtime attached; \
                         release command will not be auto-scheduled"
                    );
                }
            }
        }
        Ok(())
    }

    /// Best-effort graceful close. We intentionally ignore the write
    /// result — the device may already be gone, and the next `drop`
    /// will tear the handle down regardless.
    pub fn disconnect(mut self) {
        let _ = write_command(&mut *self.transport, wire::DISCONNECT, &[]);
    }

    pub fn snapshot(&self, current_layer: Option<u8>) -> Snapshot {
        Snapshot {
            firmware_version: self.firmware_version.clone(),
            keyboard_name: self.product_string.clone(),
            layer_idx: current_layer.map(i32::from),
            protocol_version: Some(self.protocol_version),
        }
    }
}

/// Map a raw `Event` to its user-facing projection (or drop it).
fn classify(event: Event) -> WatchEvent {
    match event {
        Event::LayerChanged(l) => WatchEvent::LayerChanged(l),
        Event::Error { code } => WatchEvent::Error(format!("firmware error 0x{code:02X}")),
        // Keydown/up are Phase 2+ consumers; accept and drop cleanly.
        Event::KeyDown { .. }
        | Event::KeyUp { .. }
        | Event::Unknown { .. }
        | Event::PairingSuccess
        | Event::ProtocolVersion(_)
        | Event::FirmwareVersion(_) => WatchEvent::Idle,
    }
}

/// One-shot snapshot for `watch --once`. Does the full handshake, reads
/// the firmware version + initial layer (if the firmware pushes one
/// within the timeout window), and returns. The device is disconnected
/// cleanly before the function returns.
pub fn snapshot_once(opts: &ConnectOptions) -> Result<Snapshot> {
    let mut client = Client::open(opts)?;
    // Opportunistically catch the first LAYER event — firmware25 pushes
    // one on pairing success. If nothing arrives we still return a
    // useful snapshot (firmware version + product name).
    let timeout_ms = timeout_to_ms(opts.timeout.min(INITIAL_LAYER_WAIT));
    let mut layer: Option<u8> = None;
    if let Ok(Some(frame)) = client.transport.read_frame(timeout_ms) {
        if let Event::LayerChanged(l) = decode_event(&frame) {
            layer = Some(l);
        }
    }
    let snap = client.snapshot(layer);
    client.disconnect();
    Ok(snap)
}

// ---------- private helpers ----------

fn find_zsa_raw_hid<'a>(
    api: &'a HidApi,
    device_override: Option<&str>,
) -> Result<&'a hidapi::DeviceInfo> {
    // `device_override` matches either the serial number or the hidraw
    // path. Serial is the stable identifier; path is useful for testing
    // against a specific node when udev renames are in play.
    let mut match_iter = api.device_list().filter(|d| {
        d.vendor_id() == ZSA_VID && d.usage_page() == RAW_USAGE_PAGE && d.usage() == RAW_USAGE
    });

    if let Some(needle) = device_override {
        return match_iter
            .find(|d| {
                d.serial_number() == Some(needle)
                    || d.path().to_str().map(|s| s == needle).unwrap_or(false)
            })
            .ok_or_else(|| anyhow!("no ZSA HID device matches '{needle}'"));
    }
    match_iter
        .next()
        .ok_or_else(|| HidOpenError::NotFound.into())
}

fn map_open_error(path: &CStr, source: hidapi::HidError) -> anyhow::Error {
    let path_str = path.to_string_lossy().into_owned();
    let msg = source.to_string();
    if msg.contains("Permission denied") || msg.contains("EACCES") || msg.contains("permission") {
        HidOpenError::PermissionDenied { path: path_str }.into()
    } else {
        HidOpenError::Open {
            path: path_str,
            source,
        }
        .into()
    }
}

/// Encode a high-level [`Command`] into the wire-level tuple
/// `(command_byte, payload, sustain)`. Pure function; no I/O. All
/// byte-level tests drive the wire layout through this, so changing
/// a command's framing changes exactly one file.
fn encode_command(cmd: &Command) -> (u8, Vec<u8>, Option<(SustainChannel, Duration)>) {
    match *cmd {
        Command::SetLayer(n) => (wire::SET_LAYER, vec![wire::LAYER_OP_LOCK, n], None),
        Command::UnsetLayer(n) => (wire::SET_LAYER, vec![wire::LAYER_OP_UNLOCK, n], None),
        Command::RgbControl(on) => (wire::RGB_CONTROL, vec![u8::from(on)], None),
        Command::SetRgbLed {
            led,
            r,
            g,
            b,
            sustain,
        } => {
            let channel = schedule_channel(SustainChannel::Rgb, sustain);
            (wire::SET_RGB_LED, vec![led, r, g, b], channel)
        }
        Command::SetRgbAll { r, g, b, sustain } => {
            let channel = schedule_channel(SustainChannel::Rgb, sustain);
            (wire::SET_RGB_LED_ALL, vec![r, g, b], channel)
        }
        Command::SetStatusLed { led, on, sustain } => {
            let channel = schedule_channel(SustainChannel::Status, sustain);
            (wire::SET_STATUS_LED, vec![led, u8::from(on)], channel)
        }
        Command::IncreaseBrightness => (
            wire::UPDATE_BRIGHTNESS,
            vec![wire::BRIGHTNESS_INCREASE],
            None,
        ),
        Command::DecreaseBrightness => (
            wire::UPDATE_BRIGHTNESS,
            vec![wire::BRIGHTNESS_DECREASE],
            None,
        ),
        Command::StatusLedControl(on) => (wire::STATUS_LED_CONTROL, vec![u8::from(on)], None),
    }
}

/// Decide whether a sustain duration is significant enough to spawn a
/// timer. Anything under [`MIN_SUSTAIN`] is treated as "no sustain"
/// (the release would race the write and look like a glitch); anything
/// else is handed back so the pump can schedule it.
fn schedule_channel(
    channel: SustainChannel,
    sustain: Duration,
) -> Option<(SustainChannel, Duration)> {
    if sustain < MIN_SUSTAIN {
        None
    } else {
        Some((channel, sustain))
    }
}

/// Write a command and translate transport errors into
/// [`HidWriteError::Io`]. This is the write-path analogue of
/// `read_until`'s error translation — callers can downcast.
fn write_command_typed(transport: &mut dyn Transport, cmd: u8, payload: &[u8]) -> Result<()> {
    debug_assert!(
        payload.len() < REPORT_SIZE,
        "HID payload {} bytes exceeds report budget {}",
        payload.len(),
        REPORT_SIZE - 1
    );
    let mut frame = [STOP; REPORT_SIZE];
    frame[0] = cmd;
    if !payload.is_empty() {
        let end = 1 + payload.len().min(REPORT_SIZE - 1);
        frame[1..end].copy_from_slice(&payload[..end - 1]);
    }
    transport
        .write_frame(&frame)
        .map_err(|source| HidWriteError::Io { cmd, source }.into())
}

fn write_command(transport: &mut dyn Transport, cmd: u8, payload: &[u8]) -> Result<()> {
    // One byte reserved for the command; the rest is payload. Silent
    // truncation would have been a latent data-corruption bug for
    // Phase 2 writes (SetLayer takes 2 payload bytes, SetRGB* take
    // up to 5) — assert in debug so any future caller that exceeds
    // the budget fails loud instead of losing bytes.
    debug_assert!(
        payload.len() < REPORT_SIZE,
        "HID payload {} bytes exceeds report budget {}",
        payload.len(),
        REPORT_SIZE - 1
    );
    let mut frame = [STOP; REPORT_SIZE];
    frame[0] = cmd;
    if !payload.is_empty() {
        let end = 1 + payload.len().min(REPORT_SIZE - 1);
        frame[1..end].copy_from_slice(&payload[..end - 1]);
    }
    transport
        .write_frame(&frame)
        .with_context(|| format!("writing HID command 0x{cmd:02X}"))
}

/// Read frames until `pred` accepts one or the transport errors.
/// Frames that don't match are discarded (they're usually unsolicited
/// KEYDOWN/KEYUP events arriving mid-handshake — expected and harmless).
fn read_until<P>(transport: &mut dyn Transport, timeout_ms: i32, mut pred: P) -> Result<Event>
where
    P: FnMut(&Event) -> bool,
{
    // Budget the whole wait to roughly `timeout_ms`; each individual
    // `read_timeout` call uses the same budget. Under normal handshake
    // conditions the first read returns the expected event.
    loop {
        match transport.read_frame(timeout_ms) {
            Ok(Some(frame)) => {
                let ev = decode_event(&frame);
                if pred(&ev) {
                    return Ok(ev);
                }
                // Ignore unrelated events; keep reading.
                debug!(?ev, "ignoring out-of-band event during handshake");
            }
            Ok(None) => bail!("HID read timed out waiting for response"),
            Err(e) => return Err(HidOpenError::Io(e).into()),
        }
    }
}

fn timeout_to_ms(d: Duration) -> i32 {
    i32::try_from(d.as_millis()).unwrap_or(i32::MAX)
}

/// One-shot `snapshot_once` opportunistic read window for the first
/// LAYER event after pairing. Capped so that a firmware that never
/// pushes a layer (e.g. missing RGB_MATRIX on older oryx modules)
/// doesn't block the CLI past this bound.
const INITIAL_LAYER_WAIT: Duration = Duration::from_millis(500);

/// Which handshake step produced a timeout, so the error translation
/// can give a useful hint instead of a generic "read timed out".
enum HandshakeStage {
    /// The protocol-version probe — first thing we send.
    Probe,
    /// The pairing init — firmware responds immediately on firmware25.
    Pairing,
    /// The firmware-version query — post-pairing.
    FirmwareVersion,
}

/// Map a handshake-read error to a specific user-facing variant.
/// Timeouts at the Probe stage mean the firmware isn't speaking Oryx
/// at all; later-stage timeouts imply protocol drift between our
/// version and the firmware's.
fn translate_handshake_timeout(e: anyhow::Error, stage: HandshakeStage) -> anyhow::Error {
    let msg = e.to_string();
    let is_timeout = msg.contains("timed out");
    if !is_timeout {
        return e;
    }
    match stage {
        HandshakeStage::Probe => HidOpenError::FirmwareHandlerMissing.into(),
        HandshakeStage::Pairing => HidOpenError::Pairing(
            "firmware did not ack pairing — likely an Oryx HID protocol drift; \
             upgrade oryx-bench or try the stock ZSA firmware"
                .to_string(),
        )
        .into(),
        HandshakeStage::FirmwareVersion => HidOpenError::Pairing(
            "firmware paired but did not return a version — likely an Oryx HID protocol drift"
                .to_string(),
        )
        .into(),
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Scripted in-memory transport. Records every frame the client
    /// writes (so tests can assert on handshake order) and replies with
    /// a pre-queued sequence of frames.
    struct MockTransport {
        writes: Vec<[u8; REPORT_SIZE]>,
        reads: VecDeque<Result<Option<[u8; REPORT_SIZE]>, hidapi::HidError>>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                writes: Vec::new(),
                reads: VecDeque::new(),
            }
        }

        fn queue_read(&mut self, frame: [u8; REPORT_SIZE]) {
            self.reads.push_back(Ok(Some(frame)));
        }

        fn queue_timeout(&mut self) {
            self.reads.push_back(Ok(None));
        }
    }

    impl Transport for MockTransport {
        fn write_frame(&mut self, frame: &[u8; REPORT_SIZE]) -> Result<(), hidapi::HidError> {
            self.writes.push(*frame);
            Ok(())
        }

        fn read_frame(
            &mut self,
            _timeout_ms: i32,
        ) -> Result<Option<[u8; REPORT_SIZE]>, hidapi::HidError> {
            self.reads.pop_front().unwrap_or_else(|| Ok(None))
        }

        fn recorded_writes(&self) -> &[[u8; REPORT_SIZE]] {
            &self.writes
        }

        fn queue_scripted_read(&mut self, frame: [u8; REPORT_SIZE]) {
            self.queue_read(frame);
        }
    }

    // ---- framing ----

    #[test]
    fn write_command_places_cmd_then_payload_then_pad() {
        let mut t = MockTransport::new();
        write_command(&mut t, 0xAB, &[0xDE, 0xAD]).unwrap();
        let w = &t.writes[0];
        assert_eq!(w[0], 0xAB);
        assert_eq!(w[1], 0xDE);
        assert_eq!(w[2], 0xAD);
        assert!(w[3..].iter().all(|&b| b == STOP));
    }

    // ---- decode ----

    #[test]
    fn decode_layer_event() {
        let mut f = [STOP; REPORT_SIZE];
        f[0] = wire::LAYER;
        f[1] = 3;
        assert_eq!(decode_event(&f), Event::LayerChanged(3));
    }

    #[test]
    fn decode_keydown_and_keyup() {
        let mut f = [STOP; REPORT_SIZE];
        f[0] = wire::KEYDOWN;
        f[1] = 5;
        f[2] = 2;
        assert_eq!(decode_event(&f), Event::KeyDown { col: 5, row: 2 });
        f[0] = wire::KEYUP;
        assert_eq!(decode_event(&f), Event::KeyUp { col: 5, row: 2 });
    }

    #[test]
    fn decode_error_event() {
        let mut f = [STOP; REPORT_SIZE];
        f[0] = wire::ERROR;
        f[1] = 0x42;
        assert_eq!(decode_event(&f), Event::Error { code: 0x42 });
    }

    #[test]
    fn decode_pairing_success() {
        let mut f = [STOP; REPORT_SIZE];
        f[0] = wire::PAIRING_SUCCESS;
        assert_eq!(decode_event(&f), Event::PairingSuccess);
    }

    #[test]
    fn decode_protocol_version() {
        let mut f = [STOP; REPORT_SIZE];
        f[0] = wire::GET_PROTOCOL_VERSION;
        f[1] = 0x04;
        assert_eq!(decode_event(&f), Event::ProtocolVersion(0x04));
    }

    #[test]
    fn decode_fw_version_string_strips_stop_and_null() {
        let mut f = [STOP; REPORT_SIZE];
        f[0] = wire::GET_FW_VERSION;
        // "25.0.1" + stop byte
        let s = b"25.0.1";
        f[1..1 + s.len()].copy_from_slice(s);
        // trailing 0xFE is already the pad byte -> decode boundary
        match decode_event(&f) {
            Event::FirmwareVersion(v) => assert_eq!(v, "25.0.1"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn decode_unknown_event_preserves_bytes() {
        let mut f = [STOP; REPORT_SIZE];
        f[0] = 0x77;
        match decode_event(&f) {
            Event::Unknown { bytes } => assert_eq!(bytes, f),
            other => panic!("unexpected: {other:?}"),
        }
    }

    // ---- handshake state machine ----

    fn fw_frame(s: &str) -> [u8; REPORT_SIZE] {
        let mut f = [STOP; REPORT_SIZE];
        f[0] = wire::GET_FW_VERSION;
        let bytes = s.as_bytes();
        f[1..1 + bytes.len()].copy_from_slice(bytes);
        f
    }

    fn single(cmd: u8, payload: &[u8]) -> [u8; REPORT_SIZE] {
        let mut f = [STOP; REPORT_SIZE];
        f[0] = cmd;
        f[1..1 + payload.len()].copy_from_slice(payload);
        f
    }

    #[test]
    fn handshake_happy_path_matches_protocol_and_captures_fw() {
        let mut t = MockTransport::new();
        t.queue_read(single(wire::GET_PROTOCOL_VERSION, &[PROTOCOL_VERSION]));
        t.queue_read(single(wire::PAIRING_SUCCESS, &[]));
        t.queue_read(fw_frame("25.0.1"));

        let client = Client::handshake(
            Box::new(t),
            Some("Voyager".into()),
            Duration::from_millis(100),
        )
        .expect("handshake succeeds");
        assert_eq!(client.protocol_version, PROTOCOL_VERSION);
        assert_eq!(client.firmware_version.as_deref(), Some("25.0.1"));
        assert_eq!(client.product_string.as_deref(), Some("Voyager"));
    }

    #[test]
    fn handshake_tolerates_older_protocol() {
        let mut t = MockTransport::new();
        t.queue_read(single(wire::GET_PROTOCOL_VERSION, &[0x03]));
        t.queue_read(single(wire::PAIRING_SUCCESS, &[]));
        t.queue_read(fw_frame("old"));

        let client = Client::handshake(Box::new(t), None, Duration::from_millis(100))
            .expect("older protocol still opens");
        assert_eq!(client.protocol_version, 0x03);
    }

    #[test]
    fn handshake_rejects_unknown_future_protocol() {
        let mut t = MockTransport::new();
        t.queue_read(single(wire::GET_PROTOCOL_VERSION, &[0x05]));

        let err = match Client::handshake(Box::new(t), None, Duration::from_millis(100)) {
            Ok(_) => panic!("must reject unknown version"),
            Err(e) => e,
        };
        let typed = err
            .downcast_ref::<HidOpenError>()
            .expect("typed HidOpenError");
        assert!(matches!(
            typed,
            HidOpenError::UnknownProtocolVersion {
                got: 0x05,
                ours: 0x04
            }
        ));
    }

    #[test]
    fn handshake_drains_unsolicited_events_before_pairing() {
        let mut t = MockTransport::new();
        t.queue_read(single(wire::GET_PROTOCOL_VERSION, &[PROTOCOL_VERSION]));
        // Firmware can emit a KEYUP before PAIRING_SUCCESS — must be
        // discarded silently without breaking the handshake.
        t.queue_read(single(wire::KEYUP, &[0, 0]));
        t.queue_read(single(wire::PAIRING_SUCCESS, &[]));
        t.queue_read(fw_frame("25.0.1"));

        let client = Client::handshake(Box::new(t), None, Duration::from_millis(100)).expect("ok");
        assert_eq!(client.firmware_version.as_deref(), Some("25.0.1"));
    }

    #[test]
    fn write_command_sequence_matches_handshake_order() {
        // The handshake state machine issues three writes in this
        // order: GET_PROTOCOL_VERSION → PAIRING_INIT → GET_FW_VERSION.
        // Verified directly at the write layer — the full client path
        // is exercised by the handshake_happy_path test above.
        let mut t = MockTransport::new();
        write_command(&mut t, wire::GET_PROTOCOL_VERSION, &[]).unwrap();
        write_command(&mut t, wire::PAIRING_INIT, &[]).unwrap();
        write_command(&mut t, wire::GET_FW_VERSION, &[]).unwrap();
        assert_eq!(t.writes[0][0], wire::GET_PROTOCOL_VERSION);
        assert_eq!(t.writes[1][0], wire::PAIRING_INIT);
        assert_eq!(t.writes[2][0], wire::GET_FW_VERSION);
    }

    // ---- event classification ----

    #[test]
    fn classify_projects_layer_and_error() {
        assert_eq!(
            classify(Event::LayerChanged(2)),
            WatchEvent::LayerChanged(2)
        );
        match classify(Event::Error { code: 0x10 }) {
            WatchEvent::Error(msg) => assert!(msg.contains("0x10")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn classify_drops_non_user_events() {
        assert_eq!(classify(Event::PairingSuccess), WatchEvent::Idle);
        assert_eq!(
            classify(Event::KeyDown { col: 0, row: 0 }),
            WatchEvent::Idle
        );
        assert_eq!(classify(Event::KeyUp { col: 0, row: 0 }), WatchEvent::Idle);
    }

    #[test]
    fn next_event_timeout_returns_idle() {
        let mut t = MockTransport::new();
        t.queue_read(single(wire::GET_PROTOCOL_VERSION, &[PROTOCOL_VERSION]));
        t.queue_read(single(wire::PAIRING_SUCCESS, &[]));
        t.queue_read(fw_frame("25.0.1"));
        // After handshake: one real LAYER, one timeout.
        t.queue_read(single(wire::LAYER, &[7]));
        t.queue_timeout();

        let mut client = Client::handshake(Box::new(t), None, Duration::from_millis(100)).unwrap();
        assert_eq!(client.next_event().unwrap(), WatchEvent::LayerChanged(7));
        assert_eq!(client.next_event().unwrap(), WatchEvent::Idle);
    }

    #[test]
    fn next_event_transport_error_surfaces_as_typed_io_error() {
        let mut t = MockTransport::new();
        t.queue_read(single(wire::GET_PROTOCOL_VERSION, &[PROTOCOL_VERSION]));
        t.queue_read(single(wire::PAIRING_SUCCESS, &[]));
        t.queue_read(fw_frame("25.0.1"));
        // Post-handshake: transport fails hard. Client must surface a
        // typed HidOpenError::Io, not a generic anyhow error blob.
        t.reads.push_back(Err(hidapi::HidError::HidApiError {
            message: "device gone".into(),
        }));

        let mut client = Client::handshake(Box::new(t), None, Duration::from_millis(100)).unwrap();
        let err = client
            .next_event()
            .expect_err("transport error must propagate");
        let typed = err
            .downcast_ref::<HidOpenError>()
            .expect("typed HidOpenError");
        assert!(matches!(typed, HidOpenError::Io(_)));
    }

    #[test]
    fn handshake_probe_timeout_becomes_firmware_handler_missing() {
        // If the very first probe (GET_PROTOCOL_VERSION) times out, the
        // firmware isn't running the Oryx HID handler at all. Surface
        // that specifically rather than as a generic "read timed out".
        let mut t = MockTransport::new();
        t.queue_timeout();

        let err = match Client::handshake(Box::new(t), None, Duration::from_millis(10)) {
            Ok(_) => panic!("probe timeout must be translated"),
            Err(e) => e,
        };
        let typed = err
            .downcast_ref::<HidOpenError>()
            .expect("typed HidOpenError");
        assert!(matches!(typed, HidOpenError::FirmwareHandlerMissing));
    }

    #[test]
    fn handshake_pairing_timeout_becomes_protocol_drift_hint() {
        // Probe succeeds but pairing never ack'd — different root cause
        // than probe timeout; should not claim the handler is missing.
        let mut t = MockTransport::new();
        t.queue_read(single(wire::GET_PROTOCOL_VERSION, &[PROTOCOL_VERSION]));
        t.queue_timeout();

        let err = match Client::handshake(Box::new(t), None, Duration::from_millis(10)) {
            Ok(_) => panic!("pairing timeout must be translated"),
            Err(e) => e,
        };
        let typed = err
            .downcast_ref::<HidOpenError>()
            .expect("typed HidOpenError");
        match typed {
            HidOpenError::Pairing(msg) => {
                assert!(msg.contains("drift"), "expected drift hint, got: {msg}");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    // ---- Phase 2: command byte-level encoding ----
    //
    // Each test drives a single command through `encode_command` and
    // asserts the exact wire layout (command byte + payload + padding).
    // The tests go through `execute_command` indirectly via a paired
    // `Client` so we also cover the transport-write path, matching how
    // the pump dispatches commands in production.

    /// Build a paired client backed by a `MockTransport` with the
    /// three-frame handshake sequence preloaded. Returned client has
    /// no tokio runtime attached — callers that need sustain timers
    /// must attach one explicitly.
    fn paired_client() -> Client {
        let mut t = MockTransport::new();
        t.queue_read(single(wire::GET_PROTOCOL_VERSION, &[PROTOCOL_VERSION]));
        t.queue_read(single(wire::PAIRING_SUCCESS, &[]));
        t.queue_read(fw_frame("25.0.1"));
        Client::handshake(Box::new(t), None, Duration::from_millis(100))
            .expect("handshake succeeds in mock")
    }

    /// The handshake writes three frames (probe, pair, fwver); a
    /// post-handshake command is the 4th write. Indexing by this
    /// constant keeps the intent obvious at the test site.
    const HANDSHAKE_WRITES: usize = 3;

    #[test]
    fn set_layer_writes_correct_bytes() {
        let mut client = paired_client();
        client
            .command_sender()
            .send(Command::SetLayer(3))
            .expect("queue");
        client.drain_commands().expect("drain ok");
        let w = tap_last_write(&client);
        assert_eq!(w[0], wire::SET_LAYER);
        assert_eq!(w[1], wire::LAYER_OP_LOCK);
        assert_eq!(w[2], 3);
        assert!(w[3..].iter().all(|&b| b == STOP), "tail must be padding");
    }

    #[test]
    fn unset_layer_writes_correct_bytes() {
        let mut client = paired_client();
        client
            .command_sender()
            .send(Command::UnsetLayer(2))
            .expect("queue");
        client.drain_commands().expect("drain ok");
        let w = tap_last_write(&client);
        assert_eq!(w[0], wire::SET_LAYER);
        assert_eq!(w[1], wire::LAYER_OP_UNLOCK);
        assert_eq!(w[2], 2);
        assert!(w[3..].iter().all(|&b| b == STOP));
    }

    #[test]
    fn rgb_control_writes_correct_bytes() {
        let mut client = paired_client();
        client
            .command_sender()
            .send(Command::RgbControl(true))
            .expect("queue");
        client.drain_commands().expect("drain ok");
        let w = tap_last_write(&client);
        assert_eq!(w[0], wire::RGB_CONTROL);
        assert_eq!(w[1], 0x01);
        assert!(w[2..].iter().all(|&b| b == STOP));

        client
            .command_sender()
            .send(Command::RgbControl(false))
            .expect("queue");
        client.drain_commands().expect("drain ok");
        let w = tap_last_write(&client);
        assert_eq!(w[0], wire::RGB_CONTROL);
        assert_eq!(w[1], 0x00);
    }

    #[test]
    fn set_rgb_led_writes_correct_bytes() {
        let mut client = paired_client();
        client
            .command_sender()
            .send(Command::SetRgbLed {
                led: 7,
                r: 0x11,
                g: 0x22,
                b: 0x33,
                sustain: Duration::ZERO,
            })
            .expect("queue");
        client.drain_commands().expect("drain ok");
        let w = tap_last_write(&client);
        assert_eq!(w[0], wire::SET_RGB_LED);
        assert_eq!(&w[1..5], &[7, 0x11, 0x22, 0x33]);
        assert!(w[5..].iter().all(|&b| b == STOP));
    }

    #[test]
    fn set_rgb_all_writes_correct_bytes() {
        let mut client = paired_client();
        client
            .command_sender()
            .send(Command::SetRgbAll {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC,
                sustain: Duration::ZERO,
            })
            .expect("queue");
        client.drain_commands().expect("drain ok");
        let w = tap_last_write(&client);
        assert_eq!(w[0], wire::SET_RGB_LED_ALL);
        assert_eq!(&w[1..4], &[0xAA, 0xBB, 0xCC]);
        assert!(w[4..].iter().all(|&b| b == STOP));
    }

    #[test]
    fn set_status_led_writes_correct_bytes() {
        let mut client = paired_client();
        client
            .command_sender()
            .send(Command::SetStatusLed {
                led: 4,
                on: true,
                sustain: Duration::ZERO,
            })
            .expect("queue");
        client.drain_commands().expect("drain ok");
        let w = tap_last_write(&client);
        assert_eq!(w[0], wire::SET_STATUS_LED);
        assert_eq!(w[1], 4);
        assert_eq!(w[2], 1);
        assert!(w[3..].iter().all(|&b| b == STOP));
    }

    #[test]
    fn increase_brightness_writes_correct_bytes() {
        let mut client = paired_client();
        client
            .command_sender()
            .send(Command::IncreaseBrightness)
            .expect("queue");
        client.drain_commands().expect("drain ok");
        let w = tap_last_write(&client);
        assert_eq!(w[0], wire::UPDATE_BRIGHTNESS);
        assert_eq!(w[1], wire::BRIGHTNESS_INCREASE);
        assert!(w[2..].iter().all(|&b| b == STOP));
    }

    #[test]
    fn decrease_brightness_writes_correct_bytes() {
        let mut client = paired_client();
        client
            .command_sender()
            .send(Command::DecreaseBrightness)
            .expect("queue");
        client.drain_commands().expect("drain ok");
        let w = tap_last_write(&client);
        assert_eq!(w[0], wire::UPDATE_BRIGHTNESS);
        assert_eq!(w[1], wire::BRIGHTNESS_DECREASE);
        assert!(w[2..].iter().all(|&b| b == STOP));
    }

    #[test]
    fn status_led_control_writes_correct_bytes() {
        let mut client = paired_client();
        client
            .command_sender()
            .send(Command::StatusLedControl(true))
            .expect("queue");
        client.drain_commands().expect("drain ok");
        let w = tap_last_write(&client);
        assert_eq!(w[0], wire::STATUS_LED_CONTROL);
        assert_eq!(w[1], 0x01);
        assert!(w[2..].iter().all(|&b| b == STOP));
    }

    /// Fetch the last frame the transport recorded. Routes through
    /// `Transport::recorded_writes`, a test-only method — no unsafe,
    /// no downcasting, and a release build can't even see it.
    fn tap_last_write(client: &Client) -> [u8; REPORT_SIZE] {
        *client
            .transport
            .recorded_writes()
            .last()
            .expect("at least one command frame written")
    }

    fn recorded(client: &Client) -> &[[u8; REPORT_SIZE]] {
        client.transport.recorded_writes()
    }

    #[test]
    fn command_queue_drains_between_reads() {
        // Queue two commands before the read; the pump must flush both
        // writes *before* it blocks on read. Verifies the FIFO order
        // and the read-after-drain contract.
        let mut client = paired_client();
        let sender = client.command_sender();
        sender.send(Command::SetLayer(1)).unwrap();
        sender.send(Command::UnsetLayer(1)).unwrap();

        // Pre-load a LAYER event so the read path has something to
        // consume; if drain ran *after* the read we'd see it out of
        // order in the transport-writes log.
        client
            .transport
            .queue_scripted_read(single(wire::LAYER, &[1]));

        let ev = client.next_event().expect("event");
        assert_eq!(ev, WatchEvent::LayerChanged(1));

        let writes = recorded(&client);
        // First handshake writes (3), then the two commands, then the
        // read. The writes log ends exactly here — no extra traffic.
        assert_eq!(writes.len(), HANDSHAKE_WRITES + 2);
        assert_eq!(writes[HANDSHAKE_WRITES][0], wire::SET_LAYER);
        assert_eq!(writes[HANDSHAKE_WRITES][1], wire::LAYER_OP_LOCK);
        assert_eq!(writes[HANDSHAKE_WRITES + 1][0], wire::SET_LAYER);
        assert_eq!(writes[HANDSHAKE_WRITES + 1][1], wire::LAYER_OP_UNLOCK);
    }

    /// Small cushion added to any "wait past the sustain" delay in
    /// tests. The tokio timer wheel resolves to ~1ms granularity and
    /// the task that sends the release has to be polled after the
    /// timer fires; a few milliseconds of headroom keeps the tests
    /// deterministic without artificially inflating them.
    const TIMER_SLOP: Duration = Duration::from_millis(20);

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn sustain_timer_sends_hand_back() {
        // A SetRgbAll with sustain schedules a RgbControl(false) timer
        // which, when it fires, enqueues the release command back on
        // the pump queue. Driven with real time because the release
        // task runs on the tokio runtime and needs to actually be
        // polled — the test-util `advance` path makes that much more
        // fragile than the 50ms real sleep we use here.
        let sustain = Duration::from_millis(50);
        let mut client = paired_client();
        client.set_runtime(tokio::runtime::Handle::current());
        let sender = client.command_sender();
        sender
            .send(Command::SetRgbAll {
                r: 0x10,
                g: 0x20,
                b: 0x30,
                sustain,
            })
            .unwrap();
        client.drain_commands().expect("first drain");

        // At this point the SET_RGB_LED_ALL write has happened but the
        // timer hasn't fired yet.
        let writes = recorded(&client);
        assert_eq!(writes.last().unwrap()[0], wire::SET_RGB_LED_ALL);
        let writes_before = writes.len();

        // Wait past the sustain; the timer fires and enqueues a
        // RgbControl(false) that the next drain will write.
        tokio::time::sleep(sustain + TIMER_SLOP).await;
        client.drain_commands().expect("second drain");

        let writes = recorded(&client);
        assert!(writes.len() > writes_before, "release must be written");
        let release = writes.last().unwrap();
        assert_eq!(release[0], wire::RGB_CONTROL);
        assert_eq!(release[1], 0x00);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn newer_rgb_command_cancels_prior_sustain() {
        // Two consecutive SetRgbAll calls; only the second's sustain
        // timer should fire a release. If both fired we'd see two
        // RgbControl(0) frames at the end.
        let sustain = Duration::from_millis(60);
        let mut client = paired_client();
        client.set_runtime(tokio::runtime::Handle::current());
        let sender = client.command_sender();
        sender
            .send(Command::SetRgbAll {
                r: 0x01,
                g: 0x02,
                b: 0x03,
                sustain,
            })
            .unwrap();
        client.drain_commands().expect("drain 1");

        // Still inside the first sustain window, issue a second
        // sustain-bearing call. This must bump the generation counter
        // so the first timer becomes a no-op.
        tokio::time::sleep(Duration::from_millis(20)).await;
        sender
            .send(Command::SetRgbAll {
                r: 0x11,
                g: 0x22,
                b: 0x33,
                sustain,
            })
            .unwrap();
        client.drain_commands().expect("drain 2");

        // Past the first timer's absolute deadline but not the second's.
        // (First fired at t+60ms; we're at t+20+50 = t+70ms from start,
        // but the second timer runs from t+20ms so its deadline is
        // t+20+60 = t+80ms — still ahead of us.)
        tokio::time::sleep(Duration::from_millis(50)).await;
        client
            .drain_commands()
            .expect("drain 3 — first timer silenced");

        // No RgbControl(0) should have been written yet.
        let writes = recorded(&client);
        assert!(
            writes.iter().all(|w| w[0] != wire::RGB_CONTROL),
            "cancelled first timer must not fire"
        );

        // Now past the second timer's deadline; it should have fired.
        tokio::time::sleep(Duration::from_millis(40) + TIMER_SLOP).await;
        client
            .drain_commands()
            .expect("drain 4 — second timer fires");

        let writes = recorded(&client);
        let rgb_controls: Vec<_> = writes
            .iter()
            .filter(|w| w[0] == wire::RGB_CONTROL)
            .collect();
        assert_eq!(
            rgb_controls.len(),
            1,
            "exactly one release must fire (the newer one)"
        );
        assert_eq!(rgb_controls[0][1], 0x00);
    }

    #[test]
    fn encode_command_is_pure_and_total() {
        // Sanity: each Command variant produces a valid (cmd_id, payload)
        // pair. Matches the byte-level tests above but in one place so
        // a future variant is caught by at least one assertion.
        let cases: &[(Command, u8, &[u8])] = &[
            (
                Command::SetLayer(0),
                wire::SET_LAYER,
                &[wire::LAYER_OP_LOCK, 0],
            ),
            (
                Command::UnsetLayer(15),
                wire::SET_LAYER,
                &[wire::LAYER_OP_UNLOCK, 15],
            ),
            (Command::RgbControl(true), wire::RGB_CONTROL, &[1]),
            (
                Command::StatusLedControl(false),
                wire::STATUS_LED_CONTROL,
                &[0],
            ),
            (
                Command::IncreaseBrightness,
                wire::UPDATE_BRIGHTNESS,
                &[wire::BRIGHTNESS_INCREASE],
            ),
            (
                Command::DecreaseBrightness,
                wire::UPDATE_BRIGHTNESS,
                &[wire::BRIGHTNESS_DECREASE],
            ),
        ];
        for (cmd, want_id, want_payload) in cases {
            let (id, payload, _sustain) = encode_command(cmd);
            assert_eq!(id, *want_id, "cmd id mismatch for {cmd:?}");
            assert_eq!(&payload, want_payload, "payload mismatch for {cmd:?}");
        }
    }

    #[test]
    fn headless_set_layer_happy_path() {
        // End-to-end inner-loop test for `headless::run_set_layer`:
        // scripted handshake, scripted LAYER(N) echo, assert the
        // Confirmed outcome and that SET_LAYER bytes hit the wire.
        use crate::watch::headless::{set_layer_on_client, SetLayerOutcome};

        let mut client = paired_client();
        // Scripted post-handshake reads: timeout, then the LAYER echo
        // the firmware is expected to emit after processing SET_LAYER.
        client
            .transport
            .queue_scripted_read(single(wire::LAYER, &[5]));

        let outcome = set_layer_on_client(&mut client, 5, Duration::from_millis(200))
            .expect("set_layer_on_client ok");
        assert_eq!(outcome, SetLayerOutcome::Confirmed);

        let writes = recorded(&client);
        let cmd_frame = writes
            .iter()
            .find(|w| w[0] == wire::SET_LAYER)
            .expect("SET_LAYER frame must be on the wire");
        assert_eq!(cmd_frame[1], wire::LAYER_OP_LOCK);
        assert_eq!(cmd_frame[2], 5);
    }

    #[test]
    fn headless_set_layer_times_out_without_echo() {
        // No LAYER event comes back → the outcome is TimedOut, not a
        // hang. Deadline is intentionally short so the test is fast.
        use crate::watch::headless::{set_layer_on_client, SetLayerOutcome};

        let mut client = paired_client();
        // Queue a handful of timeouts so `next_event` returns Idle
        // repeatedly, letting the deadline expire.
        for _ in 0..4 {
            client.transport.queue_scripted_read([STOP; REPORT_SIZE]);
        }
        let outcome = set_layer_on_client(&mut client, 2, Duration::from_millis(30))
            .expect("set_layer_on_client ok");
        assert_eq!(outcome, SetLayerOutcome::TimedOut);
    }

    #[test]
    fn sub_min_sustain_does_not_schedule_timer() {
        // MIN_SUSTAIN is the lower bound; anything below it is treated
        // as "no sustain" to avoid races. Verify schedule_channel
        // returns None rather than Some((_, 0)).
        assert!(schedule_channel(SustainChannel::Rgb, Duration::ZERO).is_none());
        assert!(schedule_channel(SustainChannel::Rgb, Duration::from_millis(1)).is_none());
        assert!(schedule_channel(SustainChannel::Rgb, MIN_SUSTAIN).is_some());
    }
}
