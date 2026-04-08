//! Filesystem helpers: atomic write and directory creation.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

/// Write `bytes` to `path` atomically and durably:
///
/// 1. Write the contents to a sibling temp file.
/// 2. `flush()` the user-space buffer.
/// 3. `sync_all()` to flush the kernel page cache for the file.
/// 4. Atomically rename the temp file over the destination.
/// 5. `sync_all()` on the parent directory so the new directory entry
///    is durable across power loss.
///
/// On crash at any point before step 4, the destination file is either
/// the previous version or absent — never partial. After step 5, the
/// new contents are guaranteed durable.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating parent {}", parent.display()))?;

    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("creating tempfile in {}", parent.display()))?;

    // Write the contents and flush both user-space and kernel buffers
    // for the file before the rename.
    tmp.write_all(bytes)
        .with_context(|| format!("writing tempfile for {}", path.display()))?;
    tmp.flush()
        .with_context(|| format!("flushing tempfile for {}", path.display()))?;
    tmp.as_file()
        .sync_all()
        .with_context(|| format!("fsync tempfile for {}", path.display()))?;

    tmp.persist(path)
        .map_err(|e| e.error)
        .with_context(|| format!("renaming temp into place: {}", path.display()))?;

    // Fsync the parent directory so the rename's new entry survives a
    // crash. On systems where opening a directory for fsync is not
    // permitted (Windows), the open itself fails — that's fine, the
    // file rename is atomic on those platforms anyway.
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Ensure the directory `dir` exists. No-op if already present.
pub fn ensure_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating directory {}", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn atomic_write_overwrites_existing() {
        let td = TempDir::new().unwrap();
        let p = td.path().join("out.txt");
        atomic_write(&p, b"hello\n").unwrap();
        atomic_write(&p, b"world\n").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "world\n");
    }

    #[test]
    fn ensure_dir_creates_nested() {
        let td = TempDir::new().unwrap();
        let nested = td.path().join("a/b/c");
        ensure_dir(&nested).unwrap();
        assert!(nested.is_dir());
    }
}
