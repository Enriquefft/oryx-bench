//! Build backend dispatcher.
//!
//! v0.1 supports the docker backend only. The native and nix backends
//! are not yet implemented; the dispatcher rejects them with a clear
//! error rather than silently falling through.

pub mod docker;

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::config::Project;
use crate::generate::Generated;
use crate::schema::kb_toml::BuildBackend;

/// Result of a build attempt.
#[derive(Debug, Clone)]
pub struct BuildOutput {
    pub firmware_bin: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
    pub from_cache: bool,
}

/// Dispatch to the configured backend.
pub fn build(project: &Project, generated: &Generated, dry_run: bool) -> Result<BuildOutput> {
    match project.cfg.build.backend {
        BuildBackend::Docker | BuildBackend::Auto => docker::build(project, generated, dry_run),
        backend @ (BuildBackend::Native | BuildBackend::Nix) => bail!(
            "build backend '{backend}' is not supported in v0.1 — set [build] backend = \"docker\" in kb.toml"
        ),
    }
}

/// Where the staged keymap directory lives in the project's `.oryx-bench/`.
pub fn build_dir(project: &Project) -> PathBuf {
    project.cache_dir().join("build")
}

/// Where the firmware .bin lives after a successful build.
pub fn firmware_path(project: &Project) -> PathBuf {
    build_dir(project).join("firmware.bin")
}

/// Where we cache the input sha so subsequent builds with the same
/// inputs can short-circuit.
pub fn build_sha_path(project: &Project) -> PathBuf {
    build_dir(project).join("build.sha")
}

/// Hash all the inputs that affect the firmware so the build cache can
/// detect "nothing changed".
pub fn input_sha(generated: &Generated, overlay_dir: Option<&Path>) -> Result<String> {
    use anyhow::Context;
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(generated.keymap_c.as_bytes());
    hasher.update(generated.features_c.as_bytes());
    hasher.update(generated.features_h.as_bytes());
    hasher.update(generated.config_h.as_bytes());
    hasher.update(generated.rules_mk.as_bytes());
    if let Some(dir) = overlay_dir {
        if dir.exists() {
            // Walk the overlay tree deterministically. Walkdir errors
            // (broken symlinks, permission denied, etc.) and read errors
            // are propagated rather than silently dropped — silent drops
            // would produce a sha that says "the inputs are stable" when
            // they actually contain unreadable files, leading to false
            // cache hits and stale firmware.
            let mut entries: Vec<walkdir::DirEntry> = Vec::new();
            for entry in walkdir::WalkDir::new(dir) {
                let entry = entry
                    .with_context(|| format!("walking overlay directory {}", dir.display()))?;
                if entry.file_type().is_file() {
                    entries.push(entry);
                }
            }
            entries.sort_by_key(|e| e.path().to_path_buf());
            for entry in entries {
                let bytes = std::fs::read(entry.path())
                    .with_context(|| format!("reading {}", entry.path().display()))?;
                hasher.update(entry.path().display().to_string().as_bytes());
                hasher.update(&bytes);
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}
