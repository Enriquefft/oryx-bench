//! `wally-cli` flash backend.
//!
//! Invokes `wally-cli <firmware.bin>` directly. Wally-cli is the official
//! ZSA flasher for the Voyager — it puts the keyboard into bootloader
//! mode and writes the firmware. We do not pass any extra flags so the
//! tool's own behavior is the only thing the user has to learn.

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

/// Flash `firmware_path` via wally-cli. Blocks until wally-cli finishes
/// or the user cancels (Ctrl-C).
pub fn flash(firmware_path: &Path) -> Result<()> {
    if which::which("wally-cli").is_err() {
        bail!(
            "`wally-cli` not found on PATH. Install it from https://github.com/zsa/wally-cli or use `--backend keymapp`."
        );
    }
    let status = Command::new("wally-cli")
        .arg(firmware_path)
        .status()
        .with_context(|| format!("invoking wally-cli {}", firmware_path.display()))?;
    if !status.success() {
        let code = status
            .code()
            .map_or("killed by signal".to_string(), |c| c.to_string());
        bail!("wally-cli exited with status {code}");
    }
    Ok(())
}
