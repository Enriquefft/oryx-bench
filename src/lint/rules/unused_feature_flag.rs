//! `unused-feature-flag` — `[features]` enables a flag whose corresponding
//! `features.toml` section is empty.
//!
//! Catches the case where a user toggles `key_overrides = true` but
//! never adds any `[[key_overrides]]` entries — the firmware ends up
//! larger than necessary because QMK still compiles in the feature.

use crate::lint::{Issue, LintContext, LintRule, Severity};

pub struct Rule;

/// Feature-flag → "is the corresponding section empty?" predicate.
///
/// This is the single source of truth for which flags the rule
/// audits. Adding a new declarative subsection to `features.toml`
/// means adding one row here — the previous version of this rule
/// hardcoded the same knowledge in two places (the list of names AND
/// the empty-check), which led to `combos = true` with no entries
/// getting reported while `macros = true` with no entries did not.
///
/// **Known gap**: an unknown flag (typo in `[features]` like
/// `dance_party_enable = true`) returns `None` and is silently
/// accepted. Catching typos is intentionally a separate concern and
/// belongs in a future `unknown-feature-flag` lint that compares
/// against the QMK feature-flag registry. For now, the typo just
/// produces a `_ENABLE = yes` line in `rules.mk` that QMK ignores
/// — annoying but not dangerous.
fn flag_is_unused(feature_name: &str, f: &crate::schema::features::FeaturesToml) -> Option<bool> {
    match feature_name {
        "key_overrides" => Some(f.key_overrides.is_empty()),
        "combos" => Some(f.combos.is_empty()),
        "macros" => Some(f.macros.is_empty()),
        "tap_dance" | "mouse_keys" | "caps_word" | "auto_shift" | "leader_key" | "oled"
        | "rgb_matrix" | "rgblight" | "audio" | "haptic" | "unicode" | "unicodemap" | "ucis"
        | "backlight" | "encoder" => {
            // These QMK features are toggled purely via rules.mk / config.h
            // and don't have a corresponding `[[section]]` in features.toml
            // where we'd look for entries. The rule cannot tell whether
            // they're actually consumed, so it doesn't fire for them.
            None
        }
        _ => None,
    }
}

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "unused-feature-flag"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn description(&self) -> &'static str {
        "`features.toml` `[features]` enables a declarative flag whose corresponding section is empty."
    }
    fn why_bad(&self) -> &'static str {
        "QMK compiles the feature into the firmware regardless of whether you use it. The result is a larger binary that wastes flash space — relevant on the Voyager which has only ~64KB. Either add entries to the section or set the flag to false."
    }
    fn fix_example(&self) -> &'static str {
        "Either add an entry (e.g. `[[key_overrides]]` for `key_overrides = true`), or set the flag to `false`."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let mut out = Vec::new();
        let f = ctx.features;
        // Walk every enabled flag in `[features]`, not just a hardcoded
        // list — otherwise a user who enables `macros = true` with no
        // [[macros]] entries would get a silent empty compile.
        for (name, &enabled) in &f.features {
            if !enabled {
                continue;
            }
            if flag_is_unused(name.as_str(), f) == Some(true) {
                out.push(Issue {
                    rule_id: self.id().to_string(),
                    severity: self.severity(),
                    message: format!(
                        "feature flag '{name} = true' but no [[{name}]] entries — wastes flash space"
                    ),
                    layer: None,
                    position_index: None,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::features::{Combo, FeaturesToml, KeyOverride, MacroDef};

    fn features_with_flag(name: &str, enabled: bool) -> FeaturesToml {
        let mut f = FeaturesToml::default();
        f.features.insert(name.to_string(), enabled);
        f
    }

    #[test]
    fn fires_for_empty_key_overrides() {
        let f = features_with_flag("key_overrides", true);
        assert_eq!(flag_is_unused("key_overrides", &f), Some(true));
    }

    #[test]
    fn does_not_fire_when_entries_exist() {
        let mut f = features_with_flag("key_overrides", true);
        f.key_overrides.push(KeyOverride {
            mods: vec!["LSHIFT".into()],
            key: "BSPC".into(),
            sends: "DELETE".into(),
            layers: None,
        });
        assert_eq!(flag_is_unused("key_overrides", &f), Some(false));
    }

    #[test]
    fn fires_for_empty_combos() {
        let f = features_with_flag("combos", true);
        assert_eq!(flag_is_unused("combos", &f), Some(true));
    }

    #[test]
    fn does_not_fire_for_combos_when_entries_exist() {
        let mut f = features_with_flag("combos", true);
        f.combos.push(Combo {
            keys: vec!["KC_A".into(), "KC_B".into()],
            sends: "KC_ESC".into(),
            layer: None,
            timeout_ms: None,
        });
        assert_eq!(flag_is_unused("combos", &f), Some(false));
    }

    #[test]
    fn fires_for_empty_macros() {
        // Regression: prior version of the rule silently ignored `macros`
        // because it only had a hardcoded pair for key_overrides + combos.
        let f = features_with_flag("macros", true);
        assert_eq!(flag_is_unused("macros", &f), Some(true));
    }

    #[test]
    fn does_not_fire_for_macros_when_entries_exist() {
        let mut f = features_with_flag("macros", true);
        f.macros.push(MacroDef {
            name: "CK_EMAIL".into(),
            sends: "foo@example.com".into(),
            slot: Some("USER00".into()),
        });
        assert_eq!(flag_is_unused("macros", &f), Some(false));
    }

    #[test]
    fn does_not_fire_for_non_declarative_flags() {
        // mouse_keys has no corresponding features.toml section — the
        // rule can't reason about its use, so it must return None to
        // avoid a false positive.
        let f = features_with_flag("mouse_keys", true);
        assert_eq!(flag_is_unused("mouse_keys", &f), None);
    }

    #[test]
    fn unknown_flag_returns_none() {
        let f = features_with_flag("dance_party_enable", true);
        assert_eq!(flag_is_unused("dance_party_enable", &f), None);
    }
}
