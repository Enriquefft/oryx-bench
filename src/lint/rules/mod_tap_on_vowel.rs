//! `mod-tap-on-vowel` — home-row mod on a vowel position.

use crate::lint::{Issue, LintContext, LintRule, Severity};
use crate::schema::canonical::CanonicalAction;

pub struct Rule;

impl LintRule for Rule {
    fn id(&self) -> &'static str {
        "mod-tap-on-vowel"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn description(&self) -> &'static str {
        "Home-row mod (`MT(MOD_*, KC_<vowel>)`) on a vowel position."
    }
    fn why_bad(&self) -> &'static str {
        "Vowels appear in fast bigrams in many languages, which causes more mod-tap misfires than mods on consonants."
    }
    fn fix_example(&self) -> &'static str {
        "Either accept (and add achordion to mitigate), or move the mod to a consonant position."
    }

    fn check(&self, ctx: &LintContext) -> Vec<Issue> {
        let mut out = Vec::new();
        for layer in &ctx.layout.layers {
            for (idx, key) in layer.keys.iter().enumerate() {
                if let Some(CanonicalAction::ModTap { tap, .. }) = &key.tap {
                    if let Some(kc) = tap.tap_keycode() {
                        if kc.is_vowel() {
                            out.push(Issue {
                                rule_id: self.id().to_string(),
                                severity: self.severity(),
                                message: format!(
                                    "mod-tap on vowel {} — vowel bigrams cause misfires",
                                    kc.canonical_name()
                                ),
                                layer: Some(layer.name.clone()),
                                position_index: Some(idx),
                            });
                        }
                    }
                }
            }
        }
        out
    }
}
