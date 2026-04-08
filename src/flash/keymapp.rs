//! Keymapp GUI handoff backend.
//!
//! Used as the fallback when `wally-cli` is not on PATH (or the user
//! explicitly passes `--backend keymapp`). Copies the firmware to a
//! known cache location, prints platform-specific instructions for
//! opening Keymapp's "Flash from file" dialog, and exits cleanly.
//!
//! This always works, even on a system where wally-cli isn't available
//! and the user only has Keymapp installed via the ZSA installer.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use directories::ProjectDirs;

use crate::util::fs as fsx;

/// Stage the firmware at the OS-specific user cache directory
/// (`$XDG_CACHE_HOME/oryx-bench/firmware.bin` on Linux,
/// `~/Library/Caches/io.zsa.oryx-bench/firmware.bin` on macOS,
/// `%LOCALAPPDATA%\zsa\oryx-bench\cache\firmware.bin` on Windows) and
/// print Keymapp instructions. Returns Ok even though no hardware was
/// touched — the user is responsible for the manual step.
pub fn handoff(firmware_path: &Path) -> Result<()> {
    let staged = stage(firmware_path)?;
    println!(
        "{} Staged firmware at {}",
        crate::util::term::OK,
        staged.display()
    );
    println!();
    print_instructions(&staged);
    Ok(())
}

/// Compute and create the cache directory; copy the firmware in.
///
/// The destination directory is resolved via `ProjectDirs` in normal
/// use. For tests, [`stage_into`] takes an explicit directory so we
/// never touch the real user cache — the previous implementation
/// staged into `~/.cache/oryx-bench/firmware.bin` during `cargo test`
/// which both polluted the user's home and raced with their real
/// flash state.
pub fn stage(firmware_path: &Path) -> Result<PathBuf> {
    stage_into(firmware_path, &cache_dir()?)
}

/// Lower-level staging: writes into an explicit destination dir.
/// Used by the unit tests with a tempdir so they're sandboxed from
/// the user's real cache location.
pub fn stage_into(firmware_path: &Path, dest_dir: &Path) -> Result<PathBuf> {
    fsx::ensure_dir(dest_dir)?;
    let dest = dest_dir.join("firmware.bin");
    let bytes = std::fs::read(firmware_path)
        .with_context(|| format!("reading {}", firmware_path.display()))?;
    fsx::atomic_write(&dest, &bytes)?;
    Ok(dest)
}

/// User cache dir for staged firmware files. Errors cleanly if the OS
/// doesn't expose a user cache directory (e.g. a sandboxed environment
/// with no `HOME`). The caller can switch to `--backend wally` or
/// invoke Keymapp's flasher directly with the project's
/// `.oryx-bench/build/firmware.bin`.
pub fn cache_dir() -> Result<PathBuf> {
    let Some(dirs) = ProjectDirs::from("io", "zsa", "oryx-bench") else {
        bail!(
            "could not determine the user cache directory \
             (no HOME / XDG_CACHE_HOME / equivalent set). Use \
             `--backend wally` or open Keymapp directly against \
             .oryx-bench/build/firmware.bin in your project."
        );
    };
    Ok(dirs.cache_dir().to_path_buf())
}

fn print_instructions(staged: &Path) {
    let path_str = staged.display();
    println!("Open Keymapp and flash the file above:");
    if cfg!(target_os = "linux") {
        println!("  1. Launch Keymapp (or run `keymapp` from a terminal)");
        println!("  2. Click 'Flash' → 'Browse'");
        println!("  3. Select: {path_str}");
        println!("  4. Press the reset button on the back of the Voyager");
        println!("  5. Wait for Keymapp to confirm the flash succeeded");
    } else if cfg!(target_os = "macos") {
        println!("  1. Open Keymapp.app from /Applications");
        println!("  2. Click 'Flash' → 'Choose file…'");
        println!("  3. Select: {path_str}");
        println!("  4. Press the reset button on the back of the Voyager");
        println!("  5. Wait for Keymapp to confirm the flash succeeded");
    } else if cfg!(target_os = "windows") {
        println!("  1. Open Keymapp from the Start menu");
        println!("  2. Click 'Flash' → 'Browse'");
        println!("  3. Select: {path_str}");
        println!("  4. Press the reset button on the back of the Voyager");
        println!("  5. Wait for Keymapp to confirm the flash succeeded");
    } else {
        println!("  1. Open Keymapp");
        println!("  2. Use its 'Flash from file' option to select: {path_str}");
        println!("  3. Press the reset button on the back of the Voyager");
    }
    println!();
    println!("oryx-bench does not invoke dfu-util directly — the Voyager's");
    println!("flashing protocol is custom and bricking risk is real.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn stage_into_writes_firmware_to_dest() {
        // Tests exercise `stage_into` with an explicit tempdir so
        // they never touch the real user cache. The previous version
        // of this test called `stage()` which wrote into
        // `~/.cache/oryx-bench/firmware.bin` and potentially clobbered
        // a user's real staged firmware.
        let src_td = TempDir::new().unwrap();
        let dst_td = TempDir::new().unwrap();
        let src = src_td.path().join("input.bin");
        std::fs::write(&src, b"firmware bytes").unwrap();

        let staged = stage_into(&src, dst_td.path()).unwrap();
        assert!(staged.is_file());
        assert!(staged.ends_with("firmware.bin"));
        assert!(staged.starts_with(dst_td.path()));
        let written = std::fs::read(&staged).unwrap();
        assert_eq!(&written, b"firmware bytes");
        // TempDir's Drop handles cleanup — no manual unlink needed.
    }
}
