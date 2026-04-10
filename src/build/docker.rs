//! Docker build backend.
//!
//! Stages the generator-owned files (`keymap.c`, `_features.c`,
//! `_features.h`, `config.h`, `rules.mk`) into
//! `.oryx-bench/build/keymap/`, takes an exclusive build lock so two
//! concurrent `oryx-bench build` invocations can't race the cache or
//! the staged keymap directory, then invokes the bundled
//! `ghcr.io/enriquefft/oryx-bench-qmk:<tag>` image with the project
//! mounted and runs `qmk compile -kb zsa/voyager -km oryx-bench`.
//! Captures the resulting `.bin`, sha256s it via [`flash::sha256_of_file`],
//! and copies into `firmware_path()`.
//!
//! On Linux the docker invocation passes `--user $UID:$GID` so the
//! produced files in the bind-mounted project directory are owned by
//! the invoking user, not root. The spurious `.bin` left in the
//! project root by `qmk compile` is removed after staging.

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::config::Project;
use crate::flash::sha256_of_file;
use crate::generate::Generated;
use crate::util::fs as fsx;
use crate::util::lock::ProjectLock;

use super::{build_dir, build_sha_path, firmware_path, input_sha, BuildOutput};

/// Pinned image tag — derived from `CARGO_PKG_VERSION` so the image
/// version always matches the binary version. Each release ships its
/// own pinned image at `ghcr.io/enriquefft/oryx-bench-qmk:v<VERSION>`.
pub const IMAGE_TAG: &str = concat!(
    "ghcr.io/enriquefft/oryx-bench-qmk:v",
    env!("CARGO_PKG_VERSION")
);

/// `qmk compile` writes its output to the project root with this name.
/// We move it under `.oryx-bench/build/firmware.bin` after staging and
/// delete the project-root copy so the user's git tree stays clean.
const QMK_OUTPUT_NAMES: &[&str] = &["zsa_voyager_oryx-bench.bin", "oryx-bench.bin"];

pub fn build(project: &Project, generated: &Generated, dry_run: bool) -> Result<BuildOutput> {
    let dir = build_dir(project);
    fsx::ensure_dir(&dir)?;

    // Take an exclusive build lock so two concurrent `oryx-bench build`
    // instances can't corrupt each other's staged keymap dir or cache
    // file. Held for the entire build (including the docker run); the
    // lock guard releases on drop.
    let _lock = ProjectLock::acquire(&dir.join("build.lock"))
        .context("acquiring build lock — is another oryx-bench build running?")?;

    // Stage generated files. Every file the build pipeline writes is
    // owned by the codegen layer; we never invent any here.
    let keymap_dir = dir.join("keymap");
    fsx::ensure_dir(&keymap_dir)?;
    fsx::atomic_write(&keymap_dir.join("keymap.c"), generated.keymap_c.as_bytes())?;
    fsx::atomic_write(
        &keymap_dir.join("_features.c"),
        generated.features_c.as_bytes(),
    )?;
    fsx::atomic_write(
        &keymap_dir.join("_features.h"),
        generated.features_h.as_bytes(),
    )?;
    fsx::atomic_write(&keymap_dir.join("config.h"), generated.config_h.as_bytes())?;
    fsx::atomic_write(&keymap_dir.join("rules.mk"), generated.rules_mk.as_bytes())?;

    // Compute input sha and consult the cache.
    let sha = input_sha(generated, Some(&project.overlay_dir()))?;
    let cached_sha = std::fs::read_to_string(build_sha_path(project)).ok();
    let cache_hit = cached_sha.as_deref() == Some(sha.as_str()) && firmware_path(project).exists();

    if dry_run {
        return Ok(BuildOutput {
            firmware_bin: firmware_path(project),
            size_bytes: 0,
            sha256: sha,
            from_cache: cache_hit,
        });
    }

    if cache_hit {
        let path = firmware_path(project);
        let bytes_len = std::fs::metadata(&path)
            .with_context(|| format!("statting {}", path.display()))?
            .len();
        let sha256 = sha256_of_file(&path)?;
        return Ok(BuildOutput {
            firmware_bin: path,
            size_bytes: bytes_len,
            sha256,
            from_cache: true,
        });
    }

    // Real docker invocation. Surface a friendly error if docker is missing.
    if which::which("docker").is_err() {
        bail!(
            "`docker` not found on PATH. The v0.1 build backend requires docker — install it from https://docs.docker.com/get-docker/ or run `oryx-bench setup` to see what's missing."
        );
    }

    let mut cmd = Command::new("docker");
    cmd.arg("run").arg("--rm");
    // On Unix, run inside the container as the invoking user so the
    // produced files in the bind-mounted project root are owned by
    // them, not by root.
    #[cfg(unix)]
    {
        let meta = std::fs::metadata(&project.root)
            .with_context(|| format!("statting {}", project.root.display()))?;
        cmd.arg("--user")
            .arg(format!("{}:{}", meta.uid(), meta.gid()));
    }
    cmd.arg("-v")
        .arg(format!("{}:/work", project.root.display()))
        .arg("-w")
        .arg("/work")
        .arg(IMAGE_TAG)
        .args(["qmk", "compile", "-kb", "zsa/voyager", "-km", "oryx-bench"]);

    let output = cmd.output().context("invoking docker")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let code = output
            .status
            .code()
            .map_or("killed by signal".to_string(), |c| c.to_string());
        bail!(
            "docker build failed (exit {code}):\nstderr:\n{stderr}\nstdout:\n{stdout}"
        );
    }

    // Locate the produced .bin. `qmk compile` writes to the project root;
    // we move it into the build cache and delete the project-root copy
    // so the user's git tree stays clean.
    let produced = QMK_OUTPUT_NAMES
        .iter()
        .map(|name| project.root.join(name))
        .find(|p| p.exists())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "docker build claimed success but no .bin file found at any of: {QMK_OUTPUT_NAMES:?}"
            )
        })?;

    let bytes =
        std::fs::read(&produced).with_context(|| format!("reading {}", produced.display()))?;
    fsx::atomic_write(&firmware_path(project), &bytes)?;
    fsx::atomic_write(&build_sha_path(project), sha.as_bytes())?;
    // Remove the project-root copy now that it's safely staged.
    std::fs::remove_file(&produced).with_context(|| format!("removing {}", produced.display()))?;

    let firmware = firmware_path(project);
    let sha256 = sha256_of_file(&firmware)?;
    Ok(BuildOutput {
        firmware_bin: firmware,
        size_bytes: bytes.len() as u64,
        sha256,
        from_cache: false,
    })
}
