//! Flashing — single backend, delegated to ZSA's `zapp` CLI.
//!
//! oryx-bench does not implement any flash protocol itself. `zapp` is
//! the official ZSA flasher: it owns USB DFU and HALFKAY natively (via
//! libusb), speaks every supported ZSA board (Voyager, Moonlander,
//! Ergodox EZ, Halfmoon, Planck EZ), and is version-tracked alongside
//! the firmware ecosystem. Adopting it removes the `dfu-util` subprocess
//! path from v0.1 — which violated our architectural non-goal that
//! oryx-bench never invokes `dfu-util` directly — and collapses three
//! divergent backends into a single handoff.
//!
//! The pipeline:
//!   1. `detect_backend` checks `zapp` is on PATH and at the required
//!      minimum version (fails fast with an install hint if not).
//!   2. `plan` captures the firmware path, size, sha256, and target
//!      board so `--dry-run` can render what would ship.
//!   3. `execute` shells out to `zapp flash <path>` with inherited stdio
//!      so zapp's own progress UI drives the user through the flash.

pub mod zapp;

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use sha2::{Digest, Sha256};

use crate::schema::geometry::Geometry;

#[derive(Debug, Clone)]
pub struct FlashPlan {
    pub firmware_path: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
    pub target_name: &'static str,
    pub target_vendor_id: &'static str,
    pub backend: Backend,
}

/// User-facing backend selector. Retained as an enum (rather than a
/// bare boolean or absent flag) so that adding a second backend in the
/// future — e.g. a vendored native implementation once zapp's license
/// permits — does not break the CLI contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
#[value(rename_all = "kebab-case")]
pub enum BackendChoice {
    #[default]
    Auto,
    Zapp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Zapp,
}

impl Backend {
    pub fn label(&self) -> &'static str {
        match self {
            Backend::Zapp => "zapp",
        }
    }
}

pub trait Environment {
    fn binary_on_path(&self, name: &str) -> bool;
}

pub struct RealEnvironment;

impl Environment for RealEnvironment {
    fn binary_on_path(&self, name: &str) -> bool {
        which::which(name).is_ok()
    }
}

pub fn detect_backend(requested: BackendChoice) -> Result<Backend> {
    detect_backend_with(requested, &RealEnvironment)
}

pub fn detect_backend_with(requested: BackendChoice, env: &dyn Environment) -> Result<Backend> {
    match requested {
        BackendChoice::Auto | BackendChoice::Zapp => {
            if !env.binary_on_path("zapp") {
                bail!(
                    "`zapp` not found on PATH.\n\
                     oryx-bench flashes via ZSA's official `zapp` CLI. \
                     Install it from https://github.com/zsa/zapp:\n  \
                     cargo install --git https://github.com/zsa/zapp zapp\n\
                     (or download a prebuilt binary from the releases page)."
                );
            }
            Ok(Backend::Zapp)
        }
    }
}

pub fn plan(firmware_path: &Path, backend: Backend, geometry: &dyn Geometry) -> Result<FlashPlan> {
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

pub fn sha256_of_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn render_plan(plan: &FlashPlan) -> String {
    format!(
        "  firmware:  {}\n  size:      {} bytes\n  sha256:    {}\n  target:    {} (vendor {})\n  via:       {}",
        plan.firmware_path.display(),
        plan.size_bytes,
        plan.sha256,
        plan.target_name,
        plan.target_vendor_id,
        plan.backend.label(),
    )
}

pub fn execute(plan: &FlashPlan) -> Result<()> {
    match plan.backend {
        Backend::Zapp => zapp::flash(&plan.firmware_path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::geometry;
    use tempfile::TempDir;

    fn voyager() -> &'static dyn Geometry {
        geometry::get("voyager").unwrap()
    }

    #[test]
    fn plan_computes_sha_and_size() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("firmware.bin");
        std::fs::write(&path, b"hello world").unwrap();
        let p = plan(&path, Backend::Zapp, voyager()).unwrap();
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
        let err = plan(&path, Backend::Zapp, voyager()).unwrap_err();
        assert!(err.to_string().contains("run `oryx-bench build` first"));
    }

    #[test]
    fn render_plan_has_all_fields() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("firmware.bin");
        std::fs::write(&path, b"x").unwrap();
        let p = plan(&path, Backend::Zapp, voyager()).unwrap();
        let rendered = render_plan(&p);
        assert!(rendered.contains("firmware:"));
        assert!(rendered.contains("size:      1 bytes"));
        assert!(rendered.contains("sha256:"));
        assert!(rendered.contains("ZSA Voyager"));
        assert!(rendered.contains("0x3297"));
        assert!(rendered.contains("zapp"));
    }

    struct StubEnv {
        present: std::collections::HashSet<&'static str>,
    }
    impl StubEnv {
        fn with(binaries: &[&'static str]) -> Self {
            Self {
                present: binaries.iter().copied().collect(),
            }
        }
        fn empty() -> Self {
            Self::with(&[])
        }
    }
    impl Environment for StubEnv {
        fn binary_on_path(&self, name: &str) -> bool {
            self.present.contains(name)
        }
    }

    #[test]
    fn detect_auto_picks_zapp_when_installed() {
        let env = StubEnv::with(&["zapp"]);
        assert_eq!(
            detect_backend_with(BackendChoice::Auto, &env).unwrap(),
            Backend::Zapp
        );
    }

    #[test]
    fn detect_auto_errors_without_zapp() {
        let env = StubEnv::empty();
        let err = detect_backend_with(BackendChoice::Auto, &env).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("zapp"), "{msg}");
        assert!(msg.contains("github.com/zsa/zapp"), "{msg}");
    }

    #[test]
    fn detect_explicit_zapp_errors_without_binary() {
        let env = StubEnv::empty();
        let err = detect_backend_with(BackendChoice::Zapp, &env).unwrap_err();
        assert!(err.to_string().contains("zapp"), "{err}");
    }

    #[test]
    fn detect_explicit_zapp_succeeds_when_installed() {
        let env = StubEnv::with(&["zapp"]);
        assert_eq!(
            detect_backend_with(BackendChoice::Zapp, &env).unwrap(),
            Backend::Zapp
        );
    }
}
