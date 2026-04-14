//! Flashing — board-aware backend selection.
//!
//! Three backends, selected based on the keyboard's DFU parameters:
//!
//! 1. **dfu-util** — direct invocation with board-specific vendor ID,
//!    product ID, and start address. Required for boards whose bootloader
//!    uses a non-STM32 USB ID (e.g. ZSA Voyager at `3297:0791`).
//! 2. **wally-cli** — ZSA's CLI flasher. Only works with boards whose
//!    bootloader enumerates as STM32 DFU (`0483:df11`).
//! 3. **Keymapp GUI handoff** — stages firmware and prints instructions.
//!    Always available; used as the fallback when no CLI flasher is
//!    installed.
//!
//! `Auto` mode inspects the board's [`Geometry::dfu_params`] to pick the
//! right backend. Boards with a ZSA-specific bootloader (Voyager) get
//! `dfu-util`; boards with an STM32 bootloader (Moonlander) prefer
//! `wally-cli` then `dfu-util`; boards without DFU fall back to Keymapp.

pub mod dfu_util;
pub mod keymapp;
pub mod wally;

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use sha2::{Digest, Sha256};

use crate::schema::geometry::{DfuParams, Geometry, STM32_DFU_VENDOR};

#[derive(Debug, Clone)]
pub struct FlashPlan {
    pub firmware_path: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
    pub target_name: &'static str,
    pub target_vendor_id: &'static str,
    pub backend: Backend,
    pub dfu_params: Option<DfuParams>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
#[value(rename_all = "kebab-case")]
pub enum BackendChoice {
    #[default]
    Auto,
    DfuUtil,
    Wally,
    Keymapp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    DfuUtil,
    Wally,
    Keymapp,
}

impl Backend {
    pub fn label(&self) -> &'static str {
        match self {
            Backend::DfuUtil => "dfu-util",
            Backend::Wally => "wally-cli",
            Backend::Keymapp => "Keymapp (manual)",
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

pub fn detect_backend(requested: BackendChoice, geom: &dyn Geometry) -> Result<Backend> {
    detect_backend_with(requested, geom, &RealEnvironment)
}

pub fn detect_backend_with(
    requested: BackendChoice,
    geom: &dyn Geometry,
    env: &dyn Environment,
) -> Result<Backend> {
    let dfu = geom.dfu_params();
    match requested {
        BackendChoice::Auto => auto_detect(dfu, geom, env),
        BackendChoice::DfuUtil => {
            if dfu.is_none() {
                bail!(
                    "the {} does not have DFU bootloader parameters — \
                     use `--backend keymapp` instead.",
                    geom.display_name(),
                );
            }
            if !env.binary_on_path("dfu-util") {
                bail!("`dfu-util` not found on PATH. Install it or use `--backend keymapp`.");
            }
            Ok(Backend::DfuUtil)
        }
        BackendChoice::Wally => {
            if let Some(p) = dfu {
                if p.vendor_id != STM32_DFU_VENDOR {
                    bail!(
                        "`wally-cli` does not support the {} — its bootloader \
                         uses vendor {:#06x}, not STM32 DFU ({:#06x}). \
                         Use `--backend dfu-util` or `--backend auto`.",
                        geom.display_name(),
                        p.vendor_id,
                        STM32_DFU_VENDOR,
                    );
                }
            }
            if !env.binary_on_path("wally-cli") {
                bail!(
                    "`wally-cli` not found on PATH. Install it from \
                     https://github.com/zsa/wally-cli or use `--backend keymapp`."
                );
            }
            Ok(Backend::Wally)
        }
        BackendChoice::Keymapp => Ok(Backend::Keymapp),
    }
}

fn auto_detect(
    dfu: Option<DfuParams>,
    geom: &dyn Geometry,
    env: &dyn Environment,
) -> Result<Backend> {
    let Some(params) = dfu else {
        return Ok(Backend::Keymapp);
    };
    if params.vendor_id == STM32_DFU_VENDOR {
        if env.binary_on_path("wally-cli") {
            return Ok(Backend::Wally);
        }
        if env.binary_on_path("dfu-util") {
            return Ok(Backend::DfuUtil);
        }
        return Ok(Backend::Keymapp);
    }
    if env.binary_on_path("dfu-util") {
        return Ok(Backend::DfuUtil);
    }
    eprintln!(
        "warning: `dfu-util` not found on PATH — the {} requires it for \
         direct flashing. Falling back to Keymapp GUI handoff.",
        geom.display_name(),
    );
    Ok(Backend::Keymapp)
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
        dfu_params: geometry.dfu_params(),
    })
}

pub fn sha256_of_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn render_plan(plan: &FlashPlan) -> String {
    let mut out = format!(
        "  firmware:  {}\n  size:      {} bytes\n  sha256:    {}\n  target:    {} (vendor {})\n  via:       {}",
        plan.firmware_path.display(),
        plan.size_bytes,
        plan.sha256,
        plan.target_name,
        plan.target_vendor_id,
        plan.backend.label(),
    );
    if let (Backend::DfuUtil, Some(ref params)) = (plan.backend, &plan.dfu_params) {
        out.push_str(&format!(
            "\n  dfu args:  -d {} -a {} -s {}",
            params.device_id(),
            params.alt_setting,
            params.address_spec(),
        ));
    }
    out
}

pub fn execute(plan: &FlashPlan) -> Result<()> {
    match plan.backend {
        Backend::DfuUtil => {
            let params = plan.dfu_params.as_ref().ok_or_else(|| {
                anyhow::anyhow!("BUG: DfuUtil backend selected but FlashPlan has no dfu_params")
            })?;
            dfu_util::flash(&plan.firmware_path, params)
        }
        Backend::Wally => wally::flash(&plan.firmware_path),
        Backend::Keymapp => keymapp::handoff(&plan.firmware_path),
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
    fn plan_carries_dfu_params_for_voyager() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("firmware.bin");
        std::fs::write(&path, b"x").unwrap();
        let p = plan(&path, Backend::DfuUtil, voyager()).unwrap();
        let params = p.dfu_params.expect("Voyager plan should carry DFU params");
        assert_eq!(params.vendor_id, 0x3297);
        assert_eq!(params.product_id, 0x0791);
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

    #[test]
    fn render_plan_shows_dfu_args_for_dfu_util_backend() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("firmware.bin");
        std::fs::write(&path, b"x").unwrap();
        let p = plan(&path, Backend::DfuUtil, voyager()).unwrap();
        let rendered = render_plan(&p);
        assert!(rendered.contains("dfu-util"));
        assert!(rendered.contains("3297:0791"));
        assert!(rendered.contains("0x08002000:leave"));
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
    fn detect_keymapp_always_succeeds() {
        let env = StubEnv::empty();
        assert_eq!(
            detect_backend_with(BackendChoice::Keymapp, voyager(), &env).unwrap(),
            Backend::Keymapp
        );
    }

    #[test]
    fn detect_dfu_util_explicit_ok_when_installed() {
        let env = StubEnv::with(&["dfu-util"]);
        assert_eq!(
            detect_backend_with(BackendChoice::DfuUtil, voyager(), &env).unwrap(),
            Backend::DfuUtil
        );
    }

    #[test]
    fn detect_dfu_util_explicit_errors_when_missing() {
        let env = StubEnv::empty();
        let err = detect_backend_with(BackendChoice::DfuUtil, voyager(), &env).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("dfu-util") && msg.contains("PATH"), "{msg}");
    }

    #[test]
    fn detect_wally_explicit_rejects_voyager() {
        let env = StubEnv::with(&["wally-cli"]);
        let err = detect_backend_with(BackendChoice::Wally, voyager(), &env).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("wally-cli") && msg.contains("does not support"),
            "{msg}"
        );
        assert!(
            msg.contains("--backend dfu-util") || msg.contains("--backend auto"),
            "{msg}"
        );
    }

    #[test]
    fn detect_wally_explicit_errors_when_missing() {
        let env = StubEnv::empty();
        let err = detect_backend_with(BackendChoice::Wally, &Stm32Board, &env).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("wally-cli") && msg.contains("PATH"), "{msg}");
        assert!(msg.contains("--backend keymapp"), "{msg}");
    }

    #[test]
    fn detect_wally_explicit_succeeds_for_stm32_board() {
        let env = StubEnv::with(&["wally-cli"]);
        assert_eq!(
            detect_backend_with(BackendChoice::Wally, &Stm32Board, &env).unwrap(),
            Backend::Wally
        );
    }

    #[test]
    fn auto_voyager_prefers_dfu_util() {
        let env = StubEnv::with(&["dfu-util", "wally-cli"]);
        assert_eq!(
            detect_backend_with(BackendChoice::Auto, voyager(), &env).unwrap(),
            Backend::DfuUtil
        );
    }

    #[test]
    fn auto_voyager_falls_back_to_keymapp_without_dfu_util() {
        let env = StubEnv::with(&["wally-cli"]);
        assert_eq!(
            detect_backend_with(BackendChoice::Auto, voyager(), &env).unwrap(),
            Backend::Keymapp
        );
    }

    #[test]
    fn auto_stm32_prefers_wally() {
        let env = StubEnv::with(&["wally-cli", "dfu-util"]);
        assert_eq!(
            detect_backend_with(BackendChoice::Auto, &Stm32Board, &env).unwrap(),
            Backend::Wally
        );
    }

    #[test]
    fn auto_stm32_falls_back_to_dfu_util() {
        let env = StubEnv::with(&["dfu-util"]);
        assert_eq!(
            detect_backend_with(BackendChoice::Auto, &Stm32Board, &env).unwrap(),
            Backend::DfuUtil
        );
    }

    #[test]
    fn auto_stm32_falls_back_to_keymapp() {
        let env = StubEnv::empty();
        assert_eq!(
            detect_backend_with(BackendChoice::Auto, &Stm32Board, &env).unwrap(),
            Backend::Keymapp
        );
    }

    #[test]
    fn auto_no_dfu_board_always_keymapp() {
        let env = StubEnv::with(&["wally-cli", "dfu-util"]);
        assert_eq!(
            detect_backend_with(BackendChoice::Auto, &NoDfuBoard, &env).unwrap(),
            Backend::Keymapp
        );
    }

    struct Stm32Board;
    impl Geometry for Stm32Board {
        fn id(&self) -> &'static str {
            "stm32-test"
        }
        fn display_name(&self) -> &'static str {
            "Test STM32 Board"
        }
        fn matrix_key_count(&self) -> usize {
            1
        }
        fn encoder_count(&self) -> usize {
            0
        }
        fn position_to_index(&self, _: &str) -> Option<usize> {
            None
        }
        fn index_to_position(&self, _: usize) -> Option<&'static str> {
            None
        }
        fn ascii_layout(&self) -> &'static geometry::GridLayout {
            unimplemented!()
        }
        fn qmk_keyboard(&self) -> &'static str {
            "test/stm32"
        }
        fn layout_macro(&self) -> &'static str {
            "LAYOUT"
        }
        fn qmk_arg_order(&self) -> &'static [usize] {
            &[0]
        }
        fn usb_vendor_id(&self) -> &'static str {
            "0x0483"
        }
        fn flash_budget_bytes(&self) -> u64 {
            64 * 1024
        }
        fn dfu_params(&self) -> Option<DfuParams> {
            Some(DfuParams {
                vendor_id: STM32_DFU_VENDOR,
                product_id: 0xDF11,
                alt_setting: 0,
                start_address: 0x0800_0000,
            })
        }
    }

    struct NoDfuBoard;
    impl Geometry for NoDfuBoard {
        fn id(&self) -> &'static str {
            "no-dfu-test"
        }
        fn display_name(&self) -> &'static str {
            "Test No-DFU Board"
        }
        fn matrix_key_count(&self) -> usize {
            1
        }
        fn encoder_count(&self) -> usize {
            0
        }
        fn position_to_index(&self, _: &str) -> Option<usize> {
            None
        }
        fn index_to_position(&self, _: usize) -> Option<&'static str> {
            None
        }
        fn ascii_layout(&self) -> &'static geometry::GridLayout {
            unimplemented!()
        }
        fn qmk_keyboard(&self) -> &'static str {
            "test/nodfu"
        }
        fn layout_macro(&self) -> &'static str {
            "LAYOUT"
        }
        fn qmk_arg_order(&self) -> &'static [usize] {
            &[0]
        }
        fn usb_vendor_id(&self) -> &'static str {
            "0x0000"
        }
        fn flash_budget_bytes(&self) -> u64 {
            64 * 1024
        }
    }
}
