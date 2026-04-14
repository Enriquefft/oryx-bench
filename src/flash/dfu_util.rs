//! `dfu-util` flash backend.

use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::schema::geometry::DfuParams;

/// Flash `firmware_path` via `dfu-util` using the given DFU parameters.
pub fn flash(firmware_path: &Path, params: &DfuParams) -> Result<()> {
    if which::which("dfu-util").is_err() {
        bail!(
            "`dfu-util` not found on PATH. Install it:\n  \
             • Debian/Ubuntu: sudo apt install dfu-util\n  \
             • Fedora:        sudo dnf install dfu-util\n  \
             • Arch:          sudo pacman -S dfu-util\n  \
             • NixOS:         add dfu-util to environment.systemPackages\n  \
             • macOS:         brew install dfu-util\n\n\
             Or use `--backend keymapp` for GUI-based flashing."
        );
    }

    let device_id = params.device_id();

    eprintln!("Put your keyboard in bootloader mode:");
    eprintln!("  Press the reset button on the back of the keyboard.");
    eprintln!("  (Small paperclip hole — the LED will turn off when");
    eprintln!("   the bootloader is active.)");
    eprintln!();
    eprint!("Press Enter when ready (Ctrl+C to abort)… ");
    io::stderr().flush().context("flushing stderr")?;

    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .context("reading stdin")?;

    eprintln!("Flashing via dfu-util (device {device_id})…");

    let output = Command::new("dfu-util")
        .arg("-d")
        .arg(&device_id)
        .arg("-a")
        .arg(params.alt_setting.to_string())
        .arg("-s")
        .arg(params.address_spec())
        .arg("-D")
        .arg(firmware_path)
        .output()
        .context("invoking dfu-util")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = output
        .status
        .code()
        .map_or("killed by signal".into(), |c: i32| c.to_string());

    if stderr.contains("Cannot open DFU device")
        || stderr.contains("No DFU capable USB device available")
        || stderr.contains("Could not open device")
    {
        bail!(
            "dfu-util could not open the bootloader device ({device_id}), exit {code}.\n\n\
             Common causes:\n\
             1. Keyboard not in bootloader mode — press the reset button and try again.\n\
             2. Permission denied — add a udev rule so your user can access the device:\n\
                echo 'SUBSYSTEMS==\"usb\", ATTRS{{idVendor}}==\"{:04x}\", \
             ATTRS{{idProduct}}==\"{:04x}\", MODE:=\"0666\"' \\\n\
                  | sudo tee /etc/udev/rules.d/50-zsa.rules\n\
                sudo udevadm control --reload-rules && sudo udevadm trigger\n\
             3. Or run with elevated privileges: sudo oryx-bench flash --backend dfu-util\n\n\
             dfu-util stderr:\n{stderr}",
            params.vendor_id,
            params.product_id,
        );
    }

    bail!("dfu-util failed (exit {code}):\n{stderr}");
}
