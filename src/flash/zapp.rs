//! `zapp` flash backend — delegates to ZSA's official CLI flasher.
//!
//! `zapp` owns the critical path for ZSA flash protocols (STM32 DFU,
//! HALFKAY) natively via libusb. oryx-bench does not reimplement any of
//! that: we stage the firmware, hand the path to `zapp flash`, and let
//! its own TTY UI (prompt for reset, progress bar, completion message)
//! drive the user through the flash. This keeps the non-goal from
//! `ARCHITECTURE.md` intact — oryx-bench never invokes `dfu-util`
//! directly, and the flash protocol stays in ZSA's hands.
//!
//! zapp repo: <https://github.com/zsa/zapp>

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

/// Minimum zapp version we accept. Older releases pre-dated Voyager
/// support; a lower-bound check fails fast with an upgrade instruction
/// rather than letting the user hit an obscure runtime failure.
pub const MIN_VERSION: (u32, u32, u32) = (1, 0, 0);

/// Installation hint shown whenever `zapp` is missing or too old.
const INSTALL_HINT: &str = "Install zapp from https://github.com/zsa/zapp:\n  \
     cargo install --git https://github.com/zsa/zapp zapp\n\
     (or download a prebuilt binary from the releases page)";

/// Check that `zapp` is on PATH and at least [`MIN_VERSION`]. Returns
/// the detected version tuple on success.
pub fn ensure_available() -> Result<(u32, u32, u32)> {
    if which::which("zapp").is_err() {
        bail!("`zapp` not found on PATH.\n{INSTALL_HINT}");
    }
    let version = detect_version().context("querying `zapp --version`")?;
    if version < MIN_VERSION {
        bail!(
            "`zapp` {}.{}.{} is too old — oryx-bench requires >= {}.{}.{}.\n{INSTALL_HINT}",
            version.0,
            version.1,
            version.2,
            MIN_VERSION.0,
            MIN_VERSION.1,
            MIN_VERSION.2,
        );
    }
    Ok(version)
}

/// Invoke `zapp --version` and parse `"zapp X.Y.Z"` into a semver tuple.
fn detect_version() -> Result<(u32, u32, u32)> {
    let output = Command::new("zapp")
        .arg("--version")
        .output()
        .context("invoking `zapp --version`")?;
    if !output.status.success() {
        bail!(
            "`zapp --version` exited with status {}: {}",
            output
                .status
                .code()
                .map_or("killed by signal".into(), |c| c.to_string()),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_version(text.trim())
}

fn parse_version(s: &str) -> Result<(u32, u32, u32)> {
    let ver = s
        .split_whitespace()
        .find(|tok| tok.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .with_context(|| format!("no version number in `zapp --version` output: {s:?}"))?;
    let core = ver.split(['-', '+']).next().unwrap_or(ver);
    let mut parts = core.split('.');
    let major = parts
        .next()
        .and_then(|p| p.parse().ok())
        .with_context(|| format!("parsing major version from {ver:?}"))?;
    let minor = parts
        .next()
        .and_then(|p| p.parse().ok())
        .with_context(|| format!("parsing minor version from {ver:?}"))?;
    let patch = parts
        .next()
        .and_then(|p| p.parse().ok())
        .with_context(|| format!("parsing patch version from {ver:?}"))?;
    Ok((major, minor, patch))
}

/// Flash `firmware_path` by shelling out to `zapp flash <path>`.
/// stdio is inherited so zapp's progress bar and prompts reach the
/// user directly.
pub fn flash(firmware_path: &Path) -> Result<()> {
    ensure_available()?;
    let status = Command::new("zapp")
        .arg("flash")
        .arg(firmware_path)
        .status()
        .with_context(|| format!("invoking `zapp flash {}`", firmware_path.display()))?;
    if status.success() {
        return Ok(());
    }
    let code = status
        .code()
        .map_or("killed by signal".to_string(), |c| c.to_string());
    bail!("`zapp flash` exited with status {code}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_accepts_standard_cargo_output() {
        assert_eq!(parse_version("zapp 1.0.0").unwrap(), (1, 0, 0));
        assert_eq!(parse_version("zapp 1.2.3").unwrap(), (1, 2, 3));
    }

    #[test]
    fn parse_version_strips_prerelease_and_build_metadata() {
        assert_eq!(parse_version("zapp 1.0.0-rc.1").unwrap(), (1, 0, 0));
        assert_eq!(parse_version("zapp 2.5.0+git.deadbeef").unwrap(), (2, 5, 0));
    }

    #[test]
    fn parse_version_errors_on_garbage() {
        assert!(parse_version("").is_err());
        assert!(parse_version("zapp").is_err());
        assert!(parse_version("zapp x.y.z").is_err());
        assert!(parse_version("zapp 1.2").is_err());
    }
}
