//! Lint rule registry. One file per rule in this directory.

use super::LintRule;

pub mod custom_keycode_undefined;
pub mod duplicate_action;
pub mod home_row_mods_asymmetric;
pub mod kc_no_in_overlay;
pub mod large_firmware;
pub mod layer_name_collision;
pub mod lt_on_high_freq;
pub mod mod_tap_on_vowel;
pub mod not_pulled_recently;
pub mod orphaned_mod_tap;
pub mod oryx_newer_than_build;
pub mod overlay_dangling_keycode;
pub mod overlay_dangling_position;
pub mod process_record_user_collision;
pub mod tt_too_short;
pub mod unbound_tapping_term;
pub mod unknown_keycode;
pub mod unknown_layer_ref;
pub mod unreachable_layer;
pub mod unreferenced_custom_keycode;
pub mod unused_feature_flag;

/// Return every registered rule, in stable order.
pub fn registry() -> Vec<&'static dyn LintRule> {
    vec![
        // Visual-layout rules
        &lt_on_high_freq::Rule,
        &unreachable_layer::Rule,
        &kc_no_in_overlay::Rule,
        &orphaned_mod_tap::Rule,
        &unknown_keycode::Rule,
        &unknown_layer_ref::Rule,
        &duplicate_action::Rule,
        &mod_tap_on_vowel::Rule,
        &home_row_mods_asymmetric::Rule,
        &layer_name_collision::Rule,
        // Cross-tier rules
        &overlay_dangling_position::Rule,
        &overlay_dangling_keycode::Rule,
        &custom_keycode_undefined::Rule,
        &unreferenced_custom_keycode::Rule,
        &process_record_user_collision::Rule,
        &unbound_tapping_term::Rule,
        &unused_feature_flag::Rule,
        // Build/sync state rules
        &tt_too_short::Rule,
        &not_pulled_recently::Rule,
        &oryx_newer_than_build::Rule,
        &large_firmware::Rule,
    ]
}
