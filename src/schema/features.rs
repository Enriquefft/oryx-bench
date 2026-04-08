//! `overlay/features.toml` — Tier 1 declarative QMK features.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeaturesToml {
    #[serde(default)]
    pub config: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub achordion: Option<Achordion>,
    #[serde(default)]
    pub key_overrides: Vec<KeyOverride>,
    #[serde(default)]
    pub macros: Vec<MacroDef>,
    #[serde(default)]
    pub combos: Vec<Combo>,
    #[serde(default)]
    pub tapping_term_per_key: Vec<TappingTermPerKey>,
    #[serde(default)]
    pub features: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Achordion {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub chord_strategy: ChordStrategy,
    #[serde(default)]
    pub timeout: Vec<AchordionTimeout>,
    #[serde(default)]
    pub no_streak: Vec<AchordionBinding>,
    #[serde(default)]
    pub same_hand_allow: Vec<SameHandAllow>,
}

/// Achordion's same-hand-vs-opposite-hand resolution strategy. The
/// previous schema had this as a `String` and silently fell through to
/// `opposite_hands` for any typo, which made misconfiguration invisible
/// — the user thought they'd set `"always"` and got the default
/// behavior instead. Modeling it as a typed enum makes invalid values
/// fail at `kb.toml` parse time with a clear "unknown variant" error.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChordStrategy {
    /// Resolve as HOLD only when the next key is on the opposite half
    /// of the keyboard. Default and recommended.
    #[default]
    OppositeHands,
    /// Always resolve as HOLD. Effectively disables achordion's
    /// disambiguation — same as not running the helper at all.
    Always,
    /// Never resolve as HOLD via achordion's helper; falls back to
    /// the QMK default tapping-term timer.
    Never,
}

impl ChordStrategy {
    /// Stable, human-readable name. Matches the snake_case form used
    /// in `kb.toml` so error messages and `oryx-bench diff` output
    /// quote what the user actually wrote.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OppositeHands => "opposite_hands",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

impl std::fmt::Display for ChordStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchordionTimeout {
    pub binding: String,
    pub ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchordionBinding {
    pub binding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SameHandAllow {
    pub tap_hold: String,
    pub other: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyOverride {
    pub mods: Vec<String>,
    pub key: String,
    pub sends: String,
    #[serde(default)]
    pub layers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroDef {
    pub name: String,
    pub sends: String,
    #[serde(default)]
    pub slot: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Combo {
    pub keys: Vec<String>,
    pub sends: String,
    #[serde(default)]
    pub layer: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TappingTermPerKey {
    pub binding: String,
    pub ms: u32,
}

impl FeaturesToml {
    /// Effective tapping term from the `[config]` section, if set.
    ///
    /// Returns an error if the value is present but outside the valid
    /// QMK range `1..=65535` (QMK stores tapping terms as `uint16_t`,
    /// and `0` is nonsensical as a disambiguation window). The
    /// previous implementation used `as u32` which silently wrapped
    /// negative integers into giant positive ones and truncated
    /// out-of-range values without warning.
    pub fn tapping_term_ms(&self) -> anyhow::Result<Option<u32>> {
        let Some(v) = self.config.get("tapping_term_ms") else {
            return Ok(None);
        };
        let i = v.as_integer().ok_or_else(|| {
            anyhow::anyhow!(
                "features.toml [config] tapping_term_ms must be an integer, got {}",
                v.type_str()
            )
        })?;
        if !(1..=65535).contains(&i) {
            anyhow::bail!(
                "features.toml [config] tapping_term_ms = {i} is out of range; QMK requires 1..=65535"
            );
        }
        Ok(Some(i as u32))
    }

    /// Load from a path, returning an empty FeaturesToml if the file
    /// doesn't exist (which is valid — features.toml is optional).
    pub fn load_or_default(path: &std::path::Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path)?;
        let parsed = toml::from_str(&raw)?;
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_features() {
        let raw = include_str!("../../examples/voyager-dvorak/overlay/features.toml");
        let features: FeaturesToml = toml::from_str(raw).expect("example features.toml parses");
        assert_eq!(features.tapping_term_ms().unwrap(), Some(220));
        let achordion = features.achordion.unwrap();
        assert!(achordion.enabled);
        assert_eq!(achordion.chord_strategy, ChordStrategy::OppositeHands);
        assert_eq!(achordion.timeout.len(), 2);
        assert_eq!(features.key_overrides.len(), 2);
    }

    #[test]
    fn empty_features_parses() {
        let raw = "";
        let features: FeaturesToml = toml::from_str(raw).unwrap();
        assert!(features.achordion.is_none());
        assert!(features.key_overrides.is_empty());
    }

    #[test]
    fn tapping_term_ms_rejects_negative() {
        let raw = r#"
[config]
tapping_term_ms = -5
"#;
        let features: FeaturesToml = toml::from_str(raw).unwrap();
        let err = features.tapping_term_ms().unwrap_err();
        assert!(
            err.to_string().contains("out of range"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn tapping_term_ms_rejects_zero() {
        let raw = r#"
[config]
tapping_term_ms = 0
"#;
        let features: FeaturesToml = toml::from_str(raw).unwrap();
        assert!(features.tapping_term_ms().is_err());
    }

    #[test]
    fn tapping_term_ms_rejects_too_large() {
        let raw = r#"
[config]
tapping_term_ms = 100000
"#;
        let features: FeaturesToml = toml::from_str(raw).unwrap();
        assert!(features.tapping_term_ms().is_err());
    }

    #[test]
    fn tapping_term_ms_rejects_non_integer() {
        let raw = r#"
[config]
tapping_term_ms = "oops"
"#;
        let features: FeaturesToml = toml::from_str(raw).unwrap();
        let err = features.tapping_term_ms().unwrap_err();
        assert!(err.to_string().contains("must be an integer"));
    }

    #[test]
    fn tapping_term_ms_none_when_absent() {
        let raw = "";
        let features: FeaturesToml = toml::from_str(raw).unwrap();
        assert_eq!(features.tapping_term_ms().unwrap(), None);
    }
}
