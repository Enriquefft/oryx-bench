//! `kb.toml` project configuration schema.
//!
//! Single source of truth for every kb.toml setting. Every field is
//! actually consumed by some piece of the codebase — there are no
//! "documented but ignored" fields. The previous schema had several
//! that promised behavior the binary didn't deliver (e.g. `[flash]
//! backend` was parsed but the flash command only read its CLI flag);
//! those have been removed and the example fixture updated to match.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbToml {
    pub layout: Layout,
    #[serde(default)]
    pub build: Build,
    #[serde(default)]
    pub sync: Sync,
    #[serde(default)]
    pub lint: Lint,
}

impl KbToml {
    /// Validate cross-field invariants that serde can't express
    /// directly. Called by `Project::load_at` after the raw TOML
    /// parses, so an invalid combination is rejected at project-load
    /// time with a clear error rather than silently producing weird
    /// runtime behavior at the first command that touches the bad
    /// field.
    ///
    /// Each branch below documents what would happen *without* the
    /// guard, so a future maintainer can see the user-visible bug
    /// the validation prevents.
    pub fn validate(&self) -> Result<(), String> {
        // ── Sync rate limits ────────────────────────────────────────
        if self.sync.warn_if_stale_s == 0 {
            return Err(
                "[sync] warn_if_stale_s must be > 0; use a large value like 999999999 to effectively disable the not-pulled-recently lint, or remove the line to use the default (1 day)".to_string()
            );
        }
        if self.sync.poll_interval_s == 0 {
            return Err(
                "[sync] poll_interval_s must be > 0; lower values let auto-pull hammer Oryx's metadata endpoint with no rate limit".to_string()
            );
        }

        // ── Geometry: must be a known board ─────────────────────────
        // Without this check, a typo like `geometry = "voyger"`
        // would propagate to the first command that calls
        // `geometry::get(...)`, producing a different error message
        // at every call site (`build`, `flash`, `lint`, `show`,
        // `find`, `explain`, ...). Catching it here gives one clear
        // error with the supported list, immediately at load.
        if !crate::schema::geometry::is_known(&self.layout.geometry) {
            return Err(format!(
                "[layout] unknown geometry '{}' — supported: {}",
                self.layout.geometry,
                crate::schema::geometry::supported_slugs()
            ));
        }

        // ── Mode mutex: exactly one of (hash_id, layout.local) ─────
        // Both set or both unset is silently inconsistent under the
        // current `is_oryx_mode()` / `is_local_mode()` predicates,
        // which read different fields and can disagree. Reject up
        // front so a malformed project never reaches a command.
        match (self.layout.hash_id.is_some(), self.layout.local.is_some()) {
            (false, false) => {
                return Err(
                    "[layout] must specify either `hash_id = \"...\"` (Oryx mode) or `[layout.local] file = \"...\"` (local mode)".to_string()
                );
            }
            (true, true) => {
                return Err(
                    "[layout] cannot have both `hash_id` (Oryx mode) and `[layout.local]` (local mode) — pick one".to_string()
                );
            }
            _ => {}
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    /// Present iff this is an Oryx-mode project.
    #[serde(default)]
    pub hash_id: Option<String>,
    pub geometry: String,
    /// Default "latest" — or a specific revision hash to pin.
    #[serde(default = "default_revision")]
    pub revision: String,
    /// Present iff this is a local-mode project.
    #[serde(default)]
    pub local: Option<LocalLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLayout {
    pub file: String,
}

fn default_revision() -> String {
    "latest".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Build {
    /// Build backend selector. v0.1 supports `docker` and `auto`
    /// (which currently resolves to docker). Native and Nix backends
    /// land in a future release; they are accepted by the schema so
    /// users can pin a value forwards-compatibly, but the dispatcher
    /// rejects them with a clear error.
    #[serde(default)]
    pub backend: BuildBackend,
}

/// `[build] backend` selector. Modeled as a typed enum (rather than a
/// `String`) so that typos like `"dockre"` fail at `kb.toml` parse
/// time with a `unknown variant` error instead of being matched
/// against an "unknown backend" branch deep in the dispatcher.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BuildBackend {
    /// Reproducible Docker build via the QMK toolchain image.
    /// Default and the only fully-supported backend in v0.1.
    #[default]
    Docker,
    /// "Pick the best available." Currently always resolves to
    /// `Docker`. The intent is that future versions can probe for a
    /// local QMK install and prefer it when available.
    Auto,
    /// Native QMK toolchain on the host. Reserved; not implemented in
    /// v0.1 — the dispatcher rejects this with a pointer to `docker`.
    Native,
    /// Nix-driven build. Reserved; not implemented in v0.1.
    Nix,
}

impl BuildBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Auto => "auto",
            Self::Native => "native",
            Self::Nix => "nix",
        }
    }
}

impl std::fmt::Display for BuildBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sync {
    /// `serde(default)` here uses `AutoPull::default()` (which is
    /// `OnRead`), so `[sync] auto_pull` is optional in kb.toml and
    /// the default lives in exactly one place: the `#[default]`
    /// annotation on the `AutoPull` enum below.
    #[serde(default)]
    pub auto_pull: AutoPull,
    /// How often to poll Oryx for layout updates, in seconds. Used by
    /// the auto-pull cache to gate the cheap metadata query.
    #[serde(default = "default_poll_interval")]
    pub poll_interval_s: u64,
    /// How long since the last full pull before `not-pulled-recently`
    /// fires, in seconds. Default: 1 day.
    #[serde(default = "default_warn_stale")]
    pub warn_if_stale_s: u64,
}

/// Default `sync.poll_interval_s`. Matches the architecture spec's
/// 60-second cap on metadata-query rate. Public so the `init` command
/// template references it instead of duplicating the literal `60`.
pub const DEFAULT_POLL_INTERVAL_S: u64 = 60;

/// Default `sync.warn_if_stale_s`: one day, in seconds. Public so the
/// `init` command template references it instead of duplicating the
/// literal `86400`.
pub const DEFAULT_WARN_IF_STALE_S: u64 = 24 * 60 * 60;

fn default_poll_interval() -> u64 {
    DEFAULT_POLL_INTERVAL_S
}

fn default_warn_stale() -> u64 {
    DEFAULT_WARN_IF_STALE_S
}

impl Default for Sync {
    fn default() -> Self {
        Self {
            auto_pull: AutoPull::default(),
            poll_interval_s: default_poll_interval(),
            warn_if_stale_s: default_warn_stale(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutoPull {
    #[default]
    OnRead,
    OnDemand,
    Never,
}

impl AutoPull {
    /// Stable, snake_case form. Matches what serde writes to TOML so
    /// templated config files (e.g. the `init` command's kb.toml) can
    /// reference this single source of truth instead of hardcoding
    /// the string literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OnRead => "on_read",
            Self::OnDemand => "on_demand",
            Self::Never => "never",
        }
    }
}

impl std::fmt::Display for AutoPull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Lint {
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default)]
    pub strict: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_kb_toml() {
        let raw = include_str!("../../examples/voyager-dvorak/kb.toml");
        let cfg: KbToml = toml::from_str(raw).expect("example kb.toml parses");
        assert_eq!(cfg.layout.geometry, "voyager");
        assert!(cfg.layout.local.is_some(), "example should use local mode");
        assert!(
            cfg.layout.hash_id.is_none(),
            "example should not have hash_id in local mode"
        );
    }

    #[test]
    fn parses_minimal_oryx_mode() {
        let raw = r#"
[layout]
hash_id = "abc"
geometry = "voyager"
"#;
        let cfg: KbToml = toml::from_str(raw).unwrap();
        assert_eq!(cfg.layout.revision, "latest");
        assert!(matches!(cfg.sync.auto_pull, AutoPull::OnRead));
    }

    #[test]
    fn parses_local_mode() {
        let raw = r#"
[layout]
geometry = "voyager"

[layout.local]
file = "layout.toml"
"#;
        let cfg: KbToml = toml::from_str(raw).unwrap();
        assert!(cfg.layout.hash_id.is_none());
        assert_eq!(cfg.layout.local.as_ref().unwrap().file, "layout.toml");
    }

    #[test]
    fn validate_rejects_warn_if_stale_zero() {
        let raw = r#"
[layout]
hash_id = "abc"
geometry = "voyager"

[sync]
warn_if_stale_s = 0
"#;
        let cfg: KbToml = toml::from_str(raw).unwrap();
        let err = cfg.validate().unwrap_err();
        assert!(
            err.contains("warn_if_stale_s"),
            "expected warn_if_stale_s in error: {err}"
        );
    }

    #[test]
    fn validate_rejects_poll_interval_zero() {
        let raw = r#"
[layout]
hash_id = "abc"
geometry = "voyager"

[sync]
poll_interval_s = 0
"#;
        let cfg: KbToml = toml::from_str(raw).unwrap();
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("poll_interval_s"));
    }

    #[test]
    fn validate_accepts_defaults() {
        let raw = r#"
[layout]
hash_id = "abc"
geometry = "voyager"
"#;
        let cfg: KbToml = toml::from_str(raw).unwrap();
        cfg.validate().unwrap();
    }

    #[test]
    fn validate_rejects_unknown_geometry() {
        let raw = r#"
[layout]
hash_id = "abc"
geometry = "voyger"
"#;
        let cfg: KbToml = toml::from_str(raw).unwrap();
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("unknown geometry"));
        assert!(err.contains("voyger"));
        assert!(
            err.contains("voyager"),
            "should list supported in error: {err}"
        );
    }

    #[test]
    fn validate_rejects_neither_oryx_nor_local_mode() {
        let raw = r#"
[layout]
geometry = "voyager"
"#;
        let cfg: KbToml = toml::from_str(raw).unwrap();
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("hash_id") && err.contains("layout.local"));
    }

    #[test]
    fn validate_rejects_both_oryx_and_local_mode() {
        let raw = r#"
[layout]
hash_id = "abc"
geometry = "voyager"

[layout.local]
file = "layout.toml"
"#;
        let cfg: KbToml = toml::from_str(raw).unwrap();
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("cannot have both"));
    }

    #[test]
    fn validate_accepts_local_mode_only() {
        let raw = r#"
[layout]
geometry = "voyager"

[layout.local]
file = "layout.toml"
"#;
        let cfg: KbToml = toml::from_str(raw).unwrap();
        cfg.validate().unwrap();
    }
}
