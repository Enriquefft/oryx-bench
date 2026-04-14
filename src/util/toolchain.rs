//! Detect whether supporting tools are on PATH.
//!
//! Used by `oryx-bench setup` and `oryx-bench status`. Detection is
//! pure (never modifies state). Verbose mode actually invokes each
//! detected tool with its standard version flag and surfaces the
//! output, so users can debug version-mismatch issues.

use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct ToolReport {
    pub tools: Vec<ToolStatus>,
}

#[derive(Debug, Clone)]
pub struct ToolStatus {
    pub name: &'static str,
    pub purpose: &'static str,
    pub path: Option<PathBuf>,
    /// The args we pass when querying the tool's version. Different
    /// tools use different conventions (`--version`, `version`).
    pub version_flag: &'static [&'static str],
}

/// `(executable, purpose, version_query_args)`. Each tool's version
/// query is whatever it actually accepts.
const TOOLS: &[(&str, &str, &[&str])] = &[
    (
        "qmk",
        "QMK firmware CLI (used by the future native backend)",
        &["--version"],
    ),
    (
        "arm-none-eabi-gcc",
        "ARM cross-compiler (used by the future native backend)",
        &["--version"],
    ),
    ("zig", "Zig compiler (Tier 2 overlay code)", &["version"]),
    ("docker", "Docker (the v0.1 build backend)", &["--version"]),
    (
        "zapp",
        "ZSA's official flasher — required by `oryx-bench flash`",
        &["--version"],
    ),
    // Note: `oryx-bench watch` talks directly to the keyboard over raw
    // HID; no daemon. Keymapp is intentionally not detected here — it
    // is not required by any oryx-bench command path.
];

/// Detect all known tools. Pure, idempotent.
pub fn detect() -> ToolReport {
    let tools = TOOLS
        .iter()
        .map(|(name, purpose, version_flag)| ToolStatus {
            name,
            purpose,
            path: which::which(name).ok(),
            version_flag,
        })
        .collect();
    ToolReport { tools }
}

impl ToolReport {
    pub fn render(&self, verbose: bool) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(out, "Toolchain detection:");
        for t in &self.tools {
            let marker = if t.path.is_some() {
                crate::util::term::OK.to_string()
            } else {
                "—".to_string()
            };
            let path = t
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(not found)".into());
            let _ = writeln!(out, "  {marker} {:<20}  {path}", t.name);
            if verbose {
                let _ = writeln!(out, "      {}", t.purpose);
                if let Some(version) = t.query_version() {
                    for line in version.lines() {
                        let _ = writeln!(out, "      version: {line}");
                    }
                }
            }
        }
        out
    }

    /// Used by `oryx-bench status` for compact rendering.
    pub fn summary(&self) -> Vec<(&'static str, bool)> {
        self.tools
            .iter()
            .map(|t| (t.name, t.path.is_some()))
            .collect()
    }
}

impl ToolStatus {
    /// Run `<tool> <version_flag>` and return the captured output,
    /// trimmed. Returns `None` if the tool isn't installed or the
    /// invocation fails — verbose render output stays clean rather
    /// than spewing error messages.
    fn query_version(&self) -> Option<String> {
        let path = self.path.as_ref()?;
        let output = Command::new(path).args(self.version_flag).output().ok()?;
        let raw = if !output.stdout.is_empty() {
            String::from_utf8_lossy(&output.stdout).to_string()
        } else {
            String::from_utf8_lossy(&output.stderr).to_string()
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}
