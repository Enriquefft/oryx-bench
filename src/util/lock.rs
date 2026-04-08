//! Project-scoped exclusive file locks.
//!
//! Used by long-running mutating operations (`build`, `pull`) so two
//! concurrent `oryx-bench` invocations against the same project can't
//! corrupt each other's caches or staged outputs. The lock is an
//! advisory file lock taken on a path under `.oryx-bench/`.
//!
//! Acquired with `ProjectLock::acquire(path)` and released when the
//! returned guard is dropped. Failure to acquire surfaces as a clear
//! error rather than blocking indefinitely.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fs2::FileExt;

/// An exclusive lock on a path under `.oryx-bench/`. Releases on drop.
#[derive(Debug)]
pub struct ProjectLock {
    /// The locked file. We hold the handle for the entire scope; the
    /// kernel releases the lock when the handle is dropped.
    file: File,
    /// Path of the lock file, kept for diagnostic messages.
    #[allow(dead_code)]
    path: PathBuf,
}

impl ProjectLock {
    /// Take an exclusive lock on `lock_path`, creating the file if it
    /// doesn't exist. The parent directory must already exist.
    ///
    /// Errors if the lock cannot be acquired (e.g. another process is
    /// holding it). The error message names the lock path so the user
    /// can investigate.
    pub fn acquire(lock_path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(lock_path)
            .with_context(|| format!("opening lock file {}", lock_path.display()))?;
        file.try_lock_exclusive().with_context(|| {
            format!(
                "could not acquire exclusive lock on {} — another oryx-bench instance is running",
                lock_path.display()
            )
        })?;
        Ok(ProjectLock {
            file,
            path: lock_path.to_path_buf(),
        })
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        // Best-effort unlock; the kernel will release on file close anyway.
        let _ = FileExt::unlock(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn first_acquire_succeeds() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("lock");
        let _lock = ProjectLock::acquire(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn second_acquire_in_same_process_blocks() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("lock");
        let _first = ProjectLock::acquire(&path).unwrap();
        // fs2 advisory locks held by the same process from a different
        // file handle will fail try_lock_exclusive — verify.
        let err = ProjectLock::acquire(&path).unwrap_err();
        assert!(
            err.to_string().contains("could not acquire exclusive lock"),
            "expected lock-contention error, got: {err:#}"
        );
    }

    #[test]
    fn drop_releases_lock() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("lock");
        {
            let _lock = ProjectLock::acquire(&path).unwrap();
        }
        // After the guard is dropped, a fresh acquire must succeed.
        let _again = ProjectLock::acquire(&path).unwrap();
    }
}
