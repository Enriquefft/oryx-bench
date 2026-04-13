//! Project root discovery and kb.toml loading.
//!
//! An oryx-bench "project" is any directory that contains (or is a
//! descendant of a directory containing) a `kb.toml` file. Discovery walks
//! upward from the cwd (or a user-supplied `--project` override) until it
//! finds one.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::error::ProjectError;
use crate::schema::kb_toml::KbToml;

#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub cfg: KbToml,
}

impl Project {
    /// Discover the project root by walking up from `start_dir` looking
    /// for `kb.toml`.
    pub fn discover(start_dir: Option<&Path>) -> Result<Self> {
        let start = match start_dir {
            Some(p) => p.to_path_buf(),
            None => std::env::current_dir().context("failed to read current working directory")?,
        };

        let Some(root) = find_root_containing_kb_toml(&start) else {
            return Err(ProjectError::NotFound(start).into());
        };

        Self::load_at(&root)
    }

    /// Load an explicit project at `root` without walking the tree.
    pub fn load_at(root: &Path) -> Result<Self> {
        let kb_path = root.join("kb.toml");
        let raw = std::fs::read_to_string(&kb_path)
            .with_context(|| format!("reading {}", kb_path.display()))?;
        let cfg: KbToml = toml::from_str(&raw).map_err(|source| ProjectError::InvalidConfig {
            path: kb_path.clone(),
            source,
        })?;
        // Validate cross-field invariants. Failing here means the
        // user gets a clear "your kb.toml is wrong" error at every
        // CLI command instead of one specific lint or pull
        // misbehaving silently.
        cfg.validate().map_err(|msg| {
            ProjectError::Other(format!("kb.toml at {}: {msg}", kb_path.display()))
        })?;
        Ok(Project {
            root: root.to_path_buf(),
            cfg,
        })
    }

    pub fn pulled_dir(&self) -> PathBuf {
        self.root.join("pulled")
    }

    pub fn pulled_revision_path(&self) -> PathBuf {
        self.pulled_dir().join("revision.json")
    }

    pub fn pulled_at_path(&self) -> PathBuf {
        self.pulled_dir().join("pulled-at.iso")
    }

    pub fn overlay_dir(&self) -> PathBuf {
        self.root.join("overlay")
    }

    pub fn overlay_features_path(&self) -> PathBuf {
        self.overlay_dir().join("features.toml")
    }

    pub fn local_layout_path(&self) -> Option<PathBuf> {
        self.cfg
            .layout
            .local
            .as_ref()
            .map(|l| self.root.join(&l.file))
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join(".oryx-bench")
    }

    pub fn cache_file(&self) -> PathBuf {
        self.cache_dir().join("cache.json")
    }

    pub fn is_oryx_mode(&self) -> bool {
        self.cfg.layout.hash_id.is_some()
    }

    pub fn is_local_mode(&self) -> bool {
        self.cfg.layout.local.is_some()
    }
}

fn find_root_containing_kb_toml(start: &Path) -> Option<PathBuf> {
    let mut cur: &Path = start;
    loop {
        if cur.join("kb.toml").is_file() {
            return Some(cur.to_path_buf());
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn discover_walks_upward_to_find_kb_toml() {
        let td = TempDir::new().unwrap();
        let root = td.path();
        std::fs::write(
            root.join("kb.toml"),
            r#"[layout]
hash_id = "yrbLx"
geometry = "voyager"
"#,
        )
        .unwrap();
        let nested = root.join("a/b/c");
        std::fs::create_dir_all(&nested).unwrap();

        let project = Project::discover(Some(&nested)).unwrap();
        assert_eq!(
            project.root.canonicalize().unwrap(),
            root.canonicalize().unwrap()
        );
        assert!(project.is_oryx_mode());
    }

    #[test]
    fn discover_returns_not_found_without_kb_toml() {
        let td = TempDir::new().unwrap();
        // Walk starts from inside the temp dir. If a kb.toml exists in
        // an ancestor directory (e.g. stale /tmp/kb.toml), discover
        // will find it — that's correct behavior, not a bug. In that
        // case we verify the found root is above our temp dir rather
        // than inside it (our temp dir has no kb.toml).
        let inner = td.path().join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        match Project::discover(Some(&inner)) {
            Err(e) => {
                assert!(
                    matches!(
                        e.downcast::<ProjectError>().unwrap(),
                        ProjectError::NotFound(_)
                    ),
                    "expected NotFound"
                );
            }
            Ok(project) => {
                // Ancestor pollution — verify the found root is NOT our
                // temp dir (which has no kb.toml).
                assert_ne!(
                    project.root.canonicalize().unwrap(),
                    td.path().canonicalize().unwrap(),
                    "should not find kb.toml inside our empty temp dir"
                );
            }
        }
    }
}
