//! Flashing — wally-cli + Keymapp GUI handoff.
//!
//! v0.1 supports two backends with detection and fallback:
//!
//! 1. **wally-cli** if on PATH — invoked directly with the firmware path.
//! 2. **Keymapp GUI handoff** as fallback — copies the `.bin` to
//!    `~/.cache/oryx-bench/firmware.bin` and prints platform-specific
//!    instructions for opening Keymapp's flasher manually.
//!
//! We **never** invoke `dfu-util` directly. The Voyager's flashing
//! protocol is custom and bricking risk is real.

pub mod keymapp;
pub mod wally;

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use sha2::{Digest, Sha256};

/// A snapshot of "what we'd flash, where, and how" — produced before any
/// destructive action so callers (and `--dry-run`) can show the user
/// exactly what's about to happen.
#[derive(Debug, Clone)]
pub struct FlashPlan {
    pub firmware_path: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
    pub target_name: &'static str,
    pub target_vendor_id: &'static str,
    pub backend: Backend,
}

/// User-facing `--backend` choice. Includes `Auto`, which is then
/// resolved at runtime by [`detect_backend`] into a concrete
/// [`Backend`]. Modeled as a `clap::ValueEnum` so a typo like
/// `--backend dfu-util` is rejected at argument-parse time with a
/// list of valid values, instead of being matched against an "unknown
/// backend" branch deep inside `detect_backend`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
#[value(rename_all = "lower")]
pub enum BackendChoice {
    /// Prefer wally-cli if it's on PATH; otherwise fall back to keymapp.
    #[default]
    Auto,
    /// Force `wally-cli`. Errors if it isn't installed.
    Wally,
    /// Force the Keymapp GUI handoff.
    Keymapp,
}

/// Concrete backend that will perform the flash. Distinct from
/// [`BackendChoice`] because `Auto` is not a concrete strategy — it's
/// resolved into one of these by [`detect_backend`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Direct invocation of `wally-cli` (preferred when available).
    Wally,
    /// Stage the firmware in `~/.cache/oryx-bench/` and tell the user to
    /// open Keymapp manually.
    Keymapp,
}

impl Backend {
    pub fn label(&self) -> &'static str {
        match self {
            Backend::Wally => "wally-cli",
            Backend::Keymapp => "Keymapp (manual)",
        }
    }
}

/// Abstraction over the bits of the host environment that
/// `detect_backend` needs to query. Concrete impl
/// [`RealEnvironment`] consults the live PATH via `which::which`.
/// Tests pass a stub so the "wally not installed" path is reachable
/// without mutating CI's PATH.
///
/// The single method takes a binary name (rather than having one
/// `wally_on_path()` method per binary) so adding a future probe —
/// e.g. `dfu-util` for a hypothetical alternate backend — doesn't
/// require growing the trait. The same `StubEnv` can serve every
/// binary check.
pub trait Environment {
    /// True iff a binary with the given name is on PATH.
    fn binary_on_path(&self, name: &str) -> bool;
}

/// Production environment: actually probes PATH.
pub struct RealEnvironment;

impl Environment for RealEnvironment {
    fn binary_on_path(&self, name: &str) -> bool {
        which::which(name).is_ok()
    }
}

/// Resolve a user-facing [`BackendChoice`] into a concrete [`Backend`]
/// using the real environment.
pub fn detect_backend(requested: BackendChoice) -> Result<Backend> {
    detect_backend_with(requested, &RealEnvironment)
}

/// Lower-level form that takes an [`Environment`] so tests can
/// inject a stub for the wally-on-PATH probe.
///
/// `Auto` prefers wally-cli if it's on PATH and falls back to Keymapp.
/// Explicit `Wally` errors if `wally-cli` isn't installed; explicit
/// `Keymapp` always succeeds because the handoff has no dependencies.
pub fn detect_backend_with(requested: BackendChoice, env: &dyn Environment) -> Result<Backend> {
    match requested {
        BackendChoice::Wally => {
            if !env.binary_on_path("wally-cli") {
                bail!(
                    "`wally-cli` not found on PATH. Install it from https://github.com/zsa/wally-cli or use `--backend keymapp`."
                );
            }
            Ok(Backend::Wally)
        }
        BackendChoice::Keymapp => Ok(Backend::Keymapp),
        BackendChoice::Auto => {
            if env.binary_on_path("wally-cli") {
                Ok(Backend::Wally)
            } else {
                Ok(Backend::Keymapp)
            }
        }
    }
}

/// Build a [`FlashPlan`] for `firmware_path`. Verifies the file exists.
///
/// Pulls the target name and USB vendor ID from `geometry`'s trait
/// methods so the dry-run output never drifts from the canonical
/// per-board metadata in `crate::schema::geometry`.
pub fn plan(
    firmware_path: &Path,
    backend: Backend,
    geometry: &dyn crate::schema::geometry::Geometry,
) -> Result<FlashPlan> {
    if !firmware_path.exists() {
        bail!(
            "no firmware at {} — run `oryx-bench build` first",
            firmware_path.display()
        );
    }
    let size_bytes = std::fs::metadata(firmware_path)
        .with_context(|| format!("statting {}", firmware_path.display()))?
        .len();
    let sha256 = sha256_of_file(firmware_path)?;

    Ok(FlashPlan {
        firmware_path: firmware_path.to_path_buf(),
        size_bytes,
        sha256,
        target_name: geometry.display_name(),
        target_vendor_id: geometry.usb_vendor_id(),
        backend,
    })
}

/// Compute the SHA-256 of a file as a lowercase hex string.
///
/// Single source of truth for firmware hashing. Both [`plan`] and the
/// docker build backend use this so the hash format never drifts
/// between "what the user sees in --dry-run" and "what the build
/// cache stores".
pub fn sha256_of_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Render the plan as a multi-line string for `--dry-run` and the
/// confirmation prompt.
pub fn render_plan(plan: &FlashPlan) -> String {
    format!(
        "  firmware:  {}\n  size:      {} bytes\n  sha256:    {}\n  target:    {} (vendor {})\n  via:       {}",
        plan.firmware_path.display(),
        plan.size_bytes,
        plan.sha256,
        plan.target_name,
        plan.target_vendor_id,
        plan.backend.label()
    )
}

/// Execute the plan. This is the irreversible step. Callers must have
/// already confirmed with the user.
pub fn execute(plan: &FlashPlan) -> Result<()> {
    match plan.backend {
        Backend::Wally => wally::flash(&plan.firmware_path),
        Backend::Keymapp => keymapp::handoff(&plan.firmware_path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::geometry;
    use tempfile::TempDir;

    fn voyager() -> &'static dyn geometry::Geometry {
        geometry::get("voyager").unwrap()
    }

    #[test]
    fn plan_computes_sha_and_size() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("firmware.bin");
        std::fs::write(&path, b"hello world").unwrap();
        let p = plan(&path, Backend::Keymapp, voyager()).unwrap();
        assert_eq!(p.size_bytes, 11);
        assert_eq!(
            p.sha256,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn plan_fails_when_firmware_missing() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("missing.bin");
        let err = plan(&path, Backend::Keymapp, voyager()).unwrap_err();
        assert!(err.to_string().contains("run `oryx-bench build` first"));
    }

    #[test]
    fn render_plan_has_all_fields() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("firmware.bin");
        std::fs::write(&path, b"x").unwrap();
        let p = plan(&path, Backend::Wally, voyager()).unwrap();
        let rendered = render_plan(&p);
        assert!(rendered.contains("firmware:"));
        assert!(rendered.contains("size:      1 bytes"));
        assert!(rendered.contains("sha256:"));
        assert!(rendered.contains("ZSA Voyager"));
        assert!(rendered.contains("0x3297"));
        assert!(rendered.contains("wally-cli"));
    }

    /// Stub environment for tests: lets each test pin whether a
    /// given binary is "on PATH" without touching the real shell
    /// env. Generic over binary name so this fixture serves every
    /// future flash backend's probe (dfu-util, etc.).
    struct StubEnv {
        present: std::collections::HashSet<&'static str>,
    }
    impl StubEnv {
        fn with_wally(present: bool) -> Self {
            let mut set = std::collections::HashSet::new();
            if present {
                set.insert("wally-cli");
            }
            Self { present: set }
        }
    }
    impl Environment for StubEnv {
        fn binary_on_path(&self, name: &str) -> bool {
            self.present.contains(name)
        }
    }

    #[test]
    fn detect_backend_keymapp_explicit() {
        // keymapp doesn't need a binary on PATH.
        assert_eq!(
            detect_backend(BackendChoice::Keymapp).unwrap(),
            Backend::Keymapp
        );
    }

    #[test]
    fn detect_backend_wally_explicit_errors_when_missing() {
        let env = StubEnv::with_wally(false);
        let err = detect_backend_with(BackendChoice::Wally, &env).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("wally-cli") && msg.contains("PATH"),
            "expected wally-cli error: {msg}"
        );
        // The error must point at a recovery action.
        assert!(msg.contains("--backend keymapp"));
    }

    #[test]
    fn detect_backend_wally_explicit_succeeds_when_present() {
        let env = StubEnv::with_wally(true);
        assert_eq!(
            detect_backend_with(BackendChoice::Wally, &env).unwrap(),
            Backend::Wally
        );
    }

    #[test]
    fn detect_backend_auto_prefers_wally_when_present() {
        let env = StubEnv::with_wally(true);
        assert_eq!(
            detect_backend_with(BackendChoice::Auto, &env).unwrap(),
            Backend::Wally
        );
    }

    #[test]
    fn detect_backend_auto_falls_back_to_keymapp() {
        let env = StubEnv::with_wally(false);
        assert_eq!(
            detect_backend_with(BackendChoice::Auto, &env).unwrap(),
            Backend::Keymapp
        );
    }
}
