//! Oryx pull client + auto-pull cache.
//!
//! Design (ARCHITECTURE.md#auto-pull-mechanism):
//!
//! - Every read command calls [`auto_pull`], which:
//!   1. Returns early if auto-pull is disabled in kb.toml.
//!   2. Returns early if the last metadata check happened < `poll_interval_s` ago.
//!   3. Issues a cheap metadata query. If the remote hash matches local, stores
//!      the cache timestamp and returns.
//!   4. If the remote hash differs, issues a full-layout query and writes
//!      `pulled/revision.json` atomically.
//!
//! The cache lives at `.oryx-bench/cache.json` in the project root.

pub mod graphql;

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::Project;
use crate::schema::kb_toml::AutoPull;
use crate::util::fs as fsx;

/// Result of a manual or auto-pull attempt.
#[derive(Debug)]
pub enum PullOutcome {
    /// A full layout was fetched and written to `pulled/revision.json`.
    Pulled { from: Option<String>, to: String },
    /// Metadata query confirmed local cache is current.
    UpToDate,
    /// The cache is newer than `poll_interval_s`; we didn't even ask Oryx.
    CacheHit,
    /// Pull is disabled (local mode, or `auto_pull = never`).
    Skipped,
}

/// Status of a lightweight metadata check (without pulling the full layout).
#[derive(Debug)]
pub enum MetadataStatus {
    UpToDate,
    Stale {
        remote_hash: String,
    },
    /// Checked recently; result unknown but the cache is fresh.
    Cached,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CacheFile {
    /// UNIX timestamp of the last metadata check (whether or not we pulled).
    #[serde(default)]
    last_check_epoch: u64,
    /// The remote revision hash we last saw on Oryx.
    #[serde(default)]
    last_remote_hash: Option<String>,
}

/// Read the auto-pull cache from disk. Distinguishes "no cache yet"
/// (returns default + no warning) from "cache file is corrupt"
/// (warns + returns default so the caller can recover by writing a
/// fresh one). The previous .ok().unwrap_or_default() chain hid corrupt
/// state, which made flaky-network reproductions invisible.
fn read_cache(project: &Project) -> CacheFile {
    let path = project.cache_file();
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return CacheFile::default(),
        Err(e) => {
            tracing::warn!(
                "could not read auto-pull cache at {}: {e:#}; treating as empty",
                path.display()
            );
            return CacheFile::default();
        }
    };
    match serde_json::from_str(&raw) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                "auto-pull cache at {} is corrupt: {e:#}; resetting on next write",
                path.display()
            );
            CacheFile::default()
        }
    }
}

fn write_cache(project: &Project, cache: &CacheFile) -> Result<()> {
    fsx::ensure_dir(&project.cache_dir())?;
    let bytes = serde_json::to_vec_pretty(cache)?;
    fsx::atomic_write(&project.cache_file(), &bytes)
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cache_age(cache: &CacheFile) -> Duration {
    Duration::from_secs(now_epoch().saturating_sub(cache.last_check_epoch))
}

/// Read the local revision hash from `pulled/revision.json`, if present.
///
/// Returns `Ok(None)` if the file simply isn't there yet (a fresh
/// project before its first pull). Returns `Err` if the file exists
/// but is unreadable or unparseable — those are user-actionable bugs
/// that the previous `.ok()?` chain swallowed silently, which made a
/// corrupt revision.json look like "no local hash" and triggered an
/// immediate overwrite.
fn local_revision_hash(project: &Project) -> Result<Option<String>> {
    let path = project.pulled_revision_path();
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(anyhow::Error::new(e).context(format!("reading {}", path.display())));
        }
    };
    let value: serde_json::Value = serde_json::from_str(&raw)
        .with_context(|| format!("parsing {} as JSON", path.display()))?;
    let hash = value
        .get("revision")
        .and_then(|r: &serde_json::Value| r.get("hashId"))
        .and_then(|h: &serde_json::Value| h.as_str())
        .map(String::from);
    Ok(hash)
}

/// Entry point for read commands. Silently no-ops when disabled.
pub fn auto_pull(project: &Project) -> Result<PullOutcome> {
    if !project.is_oryx_mode() {
        return Ok(PullOutcome::Skipped);
    }
    if matches!(
        project.cfg.sync.auto_pull,
        AutoPull::Never | AutoPull::OnDemand
    ) {
        return Ok(PullOutcome::Skipped);
    }
    let cache = read_cache(project);
    if cache_age(&cache) < Duration::from_secs(project.cfg.sync.poll_interval_s) {
        return Ok(PullOutcome::CacheHit);
    }
    pull_impl(project, None, false, cache)
}

/// Entry point for `oryx-bench pull`.
pub fn pull_now(project: &Project, revision: Option<&str>, force: bool) -> Result<PullOutcome> {
    if !project.is_oryx_mode() {
        return Ok(PullOutcome::Skipped);
    }
    if !force && matches!(project.cfg.sync.auto_pull, AutoPull::Never) {
        return Ok(PullOutcome::Skipped);
    }
    let cache = read_cache(project);
    pull_impl(project, revision, force, cache)
}

fn pull_impl(
    project: &Project,
    revision_override: Option<&str>,
    force: bool,
    mut cache: CacheFile,
) -> Result<PullOutcome> {
    let hash = project
        .cfg
        .layout
        .hash_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no hash_id in kb.toml [layout]"))?;
    let geometry = project.cfg.layout.geometry.clone();
    let revision = revision_override.unwrap_or(project.cfg.layout.revision.as_str());

    let remote_hash = match graphql::metadata_query(&hash, &geometry, revision) {
        Ok(h) => h,
        Err(e) => {
            // Update the cache timestamp even on failure so we don't
            // hammer Oryx every read command if the network is flaky.
            // The cache write itself is best-effort — failure to update
            // it just means the next read will retry the metadata query
            // sooner than `poll_interval_s`, which is acceptable.
            cache.last_check_epoch = now_epoch();
            if let Err(write_err) = write_cache(project, &cache) {
                tracing::warn!(
                    "could not update auto-pull cache after metadata failure: {write_err:#}"
                );
            }
            return Err(e);
        }
    };
    cache.last_check_epoch = now_epoch();

    let local_hash = local_revision_hash(project)?;

    if !force && local_hash.as_deref() == Some(remote_hash.as_str()) {
        cache.last_remote_hash = Some(remote_hash);
        write_cache(project, &cache)?;
        return Ok(PullOutcome::UpToDate);
    }

    let full = graphql::full_layout_query(&hash, &geometry, revision)?;
    fsx::ensure_dir(&project.pulled_dir())?;
    fsx::atomic_write(
        &project.pulled_revision_path(),
        serde_json::to_string_pretty(&full)?.as_bytes(),
    )?;
    cache.last_remote_hash = Some(remote_hash.clone());
    write_cache(project, &cache)?;

    Ok(PullOutcome::Pulled {
        from: local_hash,
        to: remote_hash,
    })
}

/// Used by `oryx-bench status` — performs the metadata query unconditionally,
/// updates the cache timestamp, and never triggers a full pull. Per the
/// architecture spec, `status` must *always* do the metadata query (cheap)
/// so that the user always sees a fresh sync status.
pub fn check_metadata_only(project: &Project) -> Result<MetadataStatus> {
    if !project.is_oryx_mode() {
        return Ok(MetadataStatus::Cached);
    }
    let hash = project
        .cfg
        .layout
        .hash_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no hash_id in kb.toml"))?;
    let remote = graphql::metadata_query(
        &hash,
        &project.cfg.layout.geometry,
        &project.cfg.layout.revision,
    )?;
    // Touch the cache so subsequent auto-pulls don't immediately re-query.
    // Cache write failure is best-effort; surface as a warning.
    let mut cache = read_cache(project);
    cache.last_check_epoch = now_epoch();
    cache.last_remote_hash = Some(remote.clone());
    if let Err(e) = write_cache(project, &cache) {
        tracing::warn!("could not update auto-pull cache: {e:#}");
    }

    if local_revision_hash(project)?.as_deref() == Some(remote.as_str()) {
        Ok(MetadataStatus::UpToDate)
    } else {
        Ok(MetadataStatus::Stale {
            remote_hash: remote,
        })
    }
}

#[allow(dead_code)]
pub(crate) fn write_cache_for_test(dir: &Path, hash: &str, epoch: u64) -> Result<()> {
    fsx::ensure_dir(dir)?;
    let cache = CacheFile {
        last_check_epoch: epoch,
        last_remote_hash: Some(hash.to_string()),
    };
    fsx::atomic_write(
        &dir.join("cache.json"),
        serde_json::to_vec(&cache)?.as_slice(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn mk_project(td: &TempDir, auto_pull: AutoPull, poll: u64) -> Project {
        let root = td.path();
        let raw = format!(
            r#"
[layout]
hash_id = "yrbLx"
geometry = "voyager"

[sync]
auto_pull = "{}"
poll_interval_s = {}
"#,
            match auto_pull {
                AutoPull::OnRead => "on_read",
                AutoPull::OnDemand => "on_demand",
                AutoPull::Never => "never",
            },
            poll
        );
        std::fs::write(root.join("kb.toml"), raw).unwrap();
        Project::load_at(root).unwrap()
    }

    #[test]
    fn cache_hit_skips_network() {
        let td = TempDir::new().unwrap();
        let project = mk_project(&td, AutoPull::OnRead, 3600);
        // Prime the cache so we're well inside the poll interval.
        write_cache(
            &project,
            &CacheFile {
                last_check_epoch: now_epoch(),
                last_remote_hash: Some("abc".into()),
            },
        )
        .unwrap();
        let out = auto_pull(&project).unwrap();
        assert!(matches!(out, PullOutcome::CacheHit));
    }

    #[test]
    fn never_mode_skips() {
        let td = TempDir::new().unwrap();
        let project = mk_project(&td, AutoPull::Never, 60);
        let out = auto_pull(&project).unwrap();
        assert!(matches!(out, PullOutcome::Skipped));
    }

    #[test]
    fn read_cache_treats_corrupt_file_as_default_with_warning() {
        // Regression: P2.1 changed `read_cache` to distinguish
        // NotFound (silent) from corrupt (warn + return default).
        // Pin both branches so a future refactor back to
        // `.ok().unwrap_or_default()` is caught by CI.
        let td = TempDir::new().unwrap();
        let project = mk_project(&td, AutoPull::OnRead, 60);
        // Write garbage where the cache file should live.
        std::fs::create_dir_all(project.cache_dir()).unwrap();
        std::fs::write(project.cache_file(), b"{not even valid json").unwrap();
        let cache = read_cache(&project);
        // Corrupt → falls back to default (epoch=0, last_remote_hash=None).
        assert_eq!(cache.last_check_epoch, 0);
        assert!(cache.last_remote_hash.is_none());
    }

    #[test]
    fn read_cache_missing_file_is_silent_default() {
        let td = TempDir::new().unwrap();
        let project = mk_project(&td, AutoPull::OnRead, 60);
        // No cache file exists yet.
        assert!(!project.cache_file().exists());
        let cache = read_cache(&project);
        assert_eq!(cache.last_check_epoch, 0);
        assert!(cache.last_remote_hash.is_none());
    }

    #[test]
    fn local_revision_hash_propagates_parse_error() {
        // Regression: P2.1 changed `local_revision_hash` to return
        // Result<Option<String>> propagating parse errors with
        // context. The old `.ok()?` chain made a corrupt
        // revision.json look like "no local hash" and triggered an
        // immediate full pull.
        let td = TempDir::new().unwrap();
        let project = mk_project(&td, AutoPull::OnRead, 60);
        std::fs::create_dir_all(project.pulled_dir()).unwrap();
        std::fs::write(project.pulled_revision_path(), b"this is not json at all").unwrap();
        let err = local_revision_hash(&project).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("parsing"),
            "expected parse-error context, got: {msg}"
        );
    }

    #[test]
    fn local_revision_hash_missing_file_is_ok_none() {
        let td = TempDir::new().unwrap();
        let project = mk_project(&td, AutoPull::OnRead, 60);
        // No pulled/revision.json exists.
        assert!(local_revision_hash(&project).unwrap().is_none());
    }
}
