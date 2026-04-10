//! Per-rule positive + negative tests for every registered lint rule.

use oryx_bench::config::Project;
use oryx_bench::lint::{self, Severity};
use oryx_bench::schema::canonical::{
    CanonicalAction, CanonicalKey, CanonicalLayer, CanonicalLayout, LayerRef,
};
use oryx_bench::schema::keycode::{Keycode, Modifier};
use tempfile::TempDir;

fn test_project_with_features(features: &str) -> (TempDir, Project) {
    test_oryx_project_with_features(features)
}

/// Create an Oryx-mode (pre-detach) test project.
fn test_oryx_project_with_features(features: &str) -> (TempDir, Project) {
    let td = TempDir::new().unwrap();
    let root = td.path();
    std::fs::write(
        root.join("kb.toml"),
        r#"[layout]
hash_id = "test"
geometry = "voyager"
"#,
    )
    .unwrap();
    std::fs::create_dir_all(root.join("overlay")).unwrap();
    std::fs::write(root.join("overlay/features.toml"), features).unwrap();
    let project = Project::load_at(root).unwrap();
    (td, project)
}

/// Create a local-mode (post-detach) test project.
fn test_local_project_with_features(features: &str) -> (TempDir, Project) {
    let td = TempDir::new().unwrap();
    let root = td.path();
    std::fs::write(
        root.join("kb.toml"),
        r#"[layout]
geometry = "voyager"

[layout.local]
file = "layout.toml"
"#,
    )
    .unwrap();
    // The file doesn't need real content — lint receives the layout
    // as a parameter, not from disk.
    std::fs::write(root.join("layout.toml"), "").unwrap();
    std::fs::create_dir_all(root.join("overlay")).unwrap();
    std::fs::write(root.join("overlay/features.toml"), features).unwrap();
    let project = Project::load_at(root).unwrap();
    (td, project)
}

fn basic_layout() -> CanonicalLayout {
    CanonicalLayout {
        geometry: "voyager".into(),
        title: "Test".into(),
        layers: vec![
            CanonicalLayer {
                name: "Main".into(),
                position: 0,
                keys: vec![CanonicalKey::default(); 52],
            },
            CanonicalLayer {
                name: "SymNum".into(),
                position: 1,
                keys: vec![CanonicalKey::default(); 52],
            },
        ],
        combos: Vec::new(),
        config: Default::default(),
    }
}

#[test]
fn lt_on_high_freq_fires_on_bspc() {
    let (_td, project) = test_project_with_features("");
    let mut layout = basic_layout();
    layout.layers[0].keys[51] = CanonicalKey {
        tap: Some(CanonicalAction::Lt {
            layer: LayerRef::Name("SymNum".into()),
            tap: Box::new(CanonicalAction::Keycode(Keycode::KcBspc)),
        }),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues.iter().any(|i| i.rule_id == "lt-on-high-freq"));
}

#[test]
fn lt_on_high_freq_does_not_fire_on_letter() {
    let (_td, project) = test_project_with_features("");
    let mut layout = basic_layout();
    layout.layers[0].keys[0] = CanonicalKey {
        tap: Some(CanonicalAction::Lt {
            layer: LayerRef::Name("SymNum".into()),
            tap: Box::new(CanonicalAction::Keycode(Keycode::KcA)),
        }),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(!issues.iter().any(|i| i.rule_id == "lt-on-high-freq"));
}

#[test]
fn lt_on_high_freq_error_in_local_mode() {
    let (_td, project) = test_local_project_with_features("");
    let mut layout = basic_layout();
    layout.layers[0].keys[51] = CanonicalKey {
        tap: Some(CanonicalAction::Lt {
            layer: LayerRef::Name("SymNum".into()),
            tap: Box::new(CanonicalAction::Keycode(Keycode::KcBspc)),
        }),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    let issue = issues.iter().find(|i| i.rule_id == "lt-on-high-freq").expect("rule should fire");
    assert_eq!(issue.severity, Severity::Error, "local mode uses default severity");
}

#[test]
fn lt_on_high_freq_downgraded_to_warning_in_oryx_mode() {
    let (_td, project) = test_oryx_project_with_features("");
    let mut layout = basic_layout();
    layout.layers[0].keys[51] = CanonicalKey {
        tap: Some(CanonicalAction::Lt {
            layer: LayerRef::Name("SymNum".into()),
            tap: Box::new(CanonicalAction::Keycode(Keycode::KcBspc)),
        }),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    let issue = issues.iter().find(|i| i.rule_id == "lt-on-high-freq").expect("rule should fire");
    assert_eq!(issue.severity, Severity::Warning, "Oryx mode downgrades to warning pre-detach");
}

#[test]
fn unreachable_layer_fires_on_orphan() {
    let (_td, project) = test_project_with_features("");
    let mut layout = basic_layout();
    layout.layers.push(CanonicalLayer {
        name: "Gaming".into(),
        position: 2,
        keys: vec![CanonicalKey::default(); 52],
    });
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues
        .iter()
        .any(|i| i.rule_id == "unreachable-layer" && i.layer.as_deref() == Some("Gaming")));
}

#[test]
fn unreachable_layer_does_not_fire_with_mo_ref() {
    let (_td, project) = test_project_with_features("");
    let mut layout = basic_layout();
    layout.layers.push(CanonicalLayer {
        name: "Gaming".into(),
        position: 2,
        keys: vec![CanonicalKey::default(); 52],
    });
    layout.layers[0].keys[0] = CanonicalKey {
        tap: Some(CanonicalAction::Mo {
            layer: LayerRef::Name("Gaming".into()),
        }),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(!issues
        .iter()
        .any(|i| i.rule_id == "unreachable-layer" && i.layer.as_deref() == Some("Gaming")));
}

#[test]
fn kc_no_in_overlay_fires_when_base_has_binding() {
    let (_td, project) = test_project_with_features("");
    let mut layout = basic_layout();
    layout.layers[0].keys[0] = CanonicalKey {
        tap: Some(CanonicalAction::Keycode(Keycode::KcA)),
        ..Default::default()
    };
    layout.layers[1].keys[0] = CanonicalKey {
        tap: Some(CanonicalAction::None),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues.iter().any(|i| i.rule_id == "kc-no-in-overlay"));
}

#[test]
fn orphaned_mod_tap_fires() {
    let (_td, project) = test_project_with_features("");
    let mut layout = basic_layout();
    layout.layers[0].keys[0] = CanonicalKey {
        tap: None,
        hold: Some(CanonicalAction::Modifier(Modifier::Lsft)),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues.iter().any(|i| i.rule_id == "orphaned-mod-tap"));
}

#[test]
fn unknown_keycode_fires_on_other() {
    let (_td, project) = test_project_with_features("");
    let mut layout = basic_layout();
    layout.layers[0].keys[0] = CanonicalKey {
        tap: Some(CanonicalAction::Keycode(Keycode::Other("KC_WOMBAT".into()))),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues.iter().any(|i| i.rule_id == "unknown-keycode"));
}

#[test]
fn unknown_layer_ref_fires_on_dangling_name() {
    let (_td, project) = test_project_with_features("");
    let mut layout = basic_layout();
    layout.layers[0].keys[0] = CanonicalKey {
        tap: Some(CanonicalAction::Mo {
            layer: LayerRef::Name("Ghost".into()),
        }),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues.iter().any(|i| i.rule_id == "unknown-layer-ref"));
}

#[test]
fn duplicate_action_fires_on_two_same_bindings() {
    let (_td, project) = test_project_with_features("");
    let mut layout = basic_layout();
    layout.layers[0].keys[0] = CanonicalKey {
        tap: Some(CanonicalAction::Keycode(Keycode::KcA)),
        ..Default::default()
    };
    layout.layers[0].keys[1] = CanonicalKey {
        tap: Some(CanonicalAction::Keycode(Keycode::KcA)),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues.iter().any(|i| i.rule_id == "duplicate-action"));
}

#[test]
fn mod_tap_on_vowel_fires() {
    let (_td, project) = test_project_with_features("");
    let mut layout = basic_layout();
    layout.layers[0].keys[0] = CanonicalKey {
        tap: Some(CanonicalAction::ModTap {
            mod_: Modifier::Lsft,
            tap: Box::new(CanonicalAction::Keycode(Keycode::KcA)),
        }),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues.iter().any(|i| i.rule_id == "mod-tap-on-vowel"));
}

#[test]
fn mod_tap_on_consonant_does_not_fire() {
    let (_td, project) = test_project_with_features("");
    let mut layout = basic_layout();
    layout.layers[0].keys[0] = CanonicalKey {
        tap: Some(CanonicalAction::ModTap {
            mod_: Modifier::Lsft,
            tap: Box::new(CanonicalAction::Keycode(Keycode::KcT)),
        }),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(!issues.iter().any(|i| i.rule_id == "mod-tap-on-vowel"));
}

#[test]
fn home_row_mods_asymmetric_fires_on_left_only() {
    let (_td, project) = test_project_with_features("");
    let mut layout = basic_layout();
    // index 12 = L_outer_home (left half outer/extension column home row)
    layout.layers[0].keys[12] = CanonicalKey {
        tap: Some(CanonicalAction::ModTap {
            mod_: Modifier::Lsft,
            tap: Box::new(CanonicalAction::Keycode(Keycode::KcS)),
        }),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues
        .iter()
        .any(|i| i.rule_id == "home-row-mods-asymmetric"));
}

#[test]
fn layer_name_collision_fires_warning_in_local_mode() {
    let (_td, project) = test_local_project_with_features("");
    let mut layout = basic_layout();
    // Both sanitize to "SYMNUM"
    layout.layers[0].name = "Sym+Num".into();
    layout.layers[1].name = "Sym Num".into();
    let issues = lint::run_all(&layout, &project).unwrap();
    let issue = issues.iter().find(|i| i.rule_id == "layer-name-collision").expect("rule should fire");
    assert_eq!(issue.severity, Severity::Warning, "local mode uses default severity");
}

#[test]
fn layer_name_collision_downgraded_to_info_in_oryx_mode() {
    let (_td, project) = test_oryx_project_with_features("");
    let mut layout = basic_layout();
    layout.layers[0].name = "Layer".into();
    layout.layers[1].name = "Layer".into();
    let issues = lint::run_all(&layout, &project).unwrap();
    let issue = issues.iter().find(|i| i.rule_id == "layer-name-collision").expect("rule should fire");
    assert_eq!(issue.severity, Severity::Info, "Oryx defaults cause collisions; downgraded to info pre-detach");
}

#[test]
fn tt_too_short_fires_below_150ms() {
    let (_td, project) = test_project_with_features(
        r#"
[config]
tapping_term_ms = 100
"#,
    );
    let mut layout = basic_layout();
    layout.layers[0].keys[0] = CanonicalKey {
        tap: Some(CanonicalAction::ModTap {
            mod_: Modifier::Lsft,
            tap: Box::new(CanonicalAction::Keycode(Keycode::KcA)),
        }),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues.iter().any(|i| i.rule_id == "tt-too-short"));
}

#[test]
fn tt_too_short_no_op_without_tap_holds() {
    let (_td, project) = test_project_with_features(
        r#"
[config]
tapping_term_ms = 50
"#,
    );
    let layout = basic_layout();
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(!issues.iter().any(|i| i.rule_id == "tt-too-short"));
}

#[test]
fn overlay_dangling_position_fires_on_bad_combo_position() {
    let (_td, project) = test_project_with_features(
        r#"
[[combos]]
keys = ["L_ghost_position"]
sends = "ESC"
"#,
    );
    let layout = basic_layout();
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues
        .iter()
        .any(|i| i.rule_id == "overlay-dangling-position"));
}

#[test]
fn overlay_dangling_keycode_fires_on_unbound_key_override() {
    let (_td, project) = test_project_with_features(
        r#"
[[key_overrides]]
mods = ["LSHIFT"]
key = "KC_F24"
sends = "DELETE"
"#,
    );
    let layout = basic_layout(); // has no F24 binding
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues
        .iter()
        .any(|i| i.rule_id == "overlay-dangling-keycode"));
}

#[test]
fn custom_keycode_undefined_fires_on_unassigned_user_slot() {
    let (_td, project) = test_project_with_features("");
    let mut layout = basic_layout();
    layout.layers[0].keys[0] = CanonicalKey {
        tap: Some(CanonicalAction::Custom(3)),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues
        .iter()
        .any(|i| i.rule_id == "custom-keycode-undefined"));
}

#[test]
fn unreferenced_custom_keycode_fires_for_orphan_macro() {
    let (_td, project) = test_project_with_features(
        r#"
[[macros]]
name = "CK_EMAIL"
sends = "you@example.com"
slot = "USER05"
"#,
    );
    let layout = basic_layout(); // nothing binds USER05
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues
        .iter()
        .any(|i| i.rule_id == "unreferenced-custom-keycode"));
}

#[test]
fn process_record_user_collision_detects_tier2_c_definition() {
    let (_td, project) = test_project_with_features("");
    std::fs::write(
        project.overlay_dir().join("bad.c"),
        "bool process_record_user(uint16_t keycode, keyrecord_t *record) { return true; }\n",
    )
    .unwrap();
    let layout = basic_layout();
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues
        .iter()
        .any(|i| i.rule_id == "process-record-user-collision"));
}

// =============================================================================
// Fixture-based tests — exercise the cross-tier rules against the real
// voyager-dvorak fixture so future schema changes that break preconditions
// surface immediately.
// =============================================================================

fn fixture_project_and_layout() -> (TempDir, Project, CanonicalLayout) {
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
    std::fs::create_dir_all(root.join("pulled")).unwrap();
    std::fs::write(
        root.join("pulled/revision.json"),
        include_str!("../examples/voyager-dvorak/pulled/revision.json"),
    )
    .unwrap();
    std::fs::create_dir_all(root.join("overlay")).unwrap();
    std::fs::write(
        root.join("overlay/features.toml"),
        include_str!("../examples/voyager-dvorak/overlay/features.toml"),
    )
    .unwrap();

    let project = Project::load_at(root).unwrap();
    let raw = std::fs::read_to_string(root.join("pulled/revision.json")).unwrap();
    let oryx_layout: oryx_bench::schema::oryx::Layout = serde_json::from_str(&raw).unwrap();
    let layout = CanonicalLayout::from_oryx(&oryx_layout).unwrap();
    (td, project, layout)
}

#[test]
fn unused_feature_flag_fires_for_empty_section() {
    let (_td, project) = test_project_with_features(
        r#"
[features]
key_overrides = true
"#,
    );
    let layout = basic_layout();
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues.iter().any(|i| i.rule_id == "unused-feature-flag"));
}

#[test]
fn unused_feature_flag_does_not_fire_when_section_populated() {
    let (_td, project) = test_project_with_features(
        r#"
[features]
key_overrides = true

[[key_overrides]]
mods = ["LSHIFT"]
key = "BSPC"
sends = "DELETE"
"#,
    );
    let mut layout = basic_layout();
    // Bind BSPC so the dangling-keycode rule is happy.
    layout.layers[0].keys[51] = CanonicalKey {
        tap: Some(CanonicalAction::Keycode(Keycode::KcBspc)),
        ..Default::default()
    };
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(!issues.iter().any(|i| i.rule_id == "unused-feature-flag"));
}

#[test]
fn unbound_tapping_term_fires_when_binding_not_in_layout() {
    let (_td, project) = test_project_with_features(
        r#"
[[tapping_term_per_key]]
binding = "LCTL_T(KC_F12)"
ms = 180
"#,
    );
    let layout = basic_layout(); // no F12 binding
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues.iter().any(|i| i.rule_id == "unbound-tapping-term"));
}

#[test]
fn large_firmware_does_not_fire_without_built_binary() {
    let (_td, project) = test_project_with_features("");
    let layout = basic_layout();
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(!issues.iter().any(|i| i.rule_id == "large-firmware"));
}

#[test]
fn large_firmware_fires_when_binary_is_close_to_budget() {
    let (td, project) = test_project_with_features("");
    let build_dir = td.path().join(".oryx-bench/build");
    std::fs::create_dir_all(&build_dir).unwrap();
    // 62KB > the 60KB warning threshold but < the 64KB hard limit.
    std::fs::write(build_dir.join("firmware.bin"), vec![0u8; 62 * 1024]).unwrap();
    let layout = basic_layout();
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(issues.iter().any(|i| i.rule_id == "large-firmware"));
}

#[test]
fn fixture_lt_on_high_freq_fires() {
    let (_td, project, layout) = fixture_project_and_layout();
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(
        issues.iter().any(|i| i.rule_id == "lt-on-high-freq"),
        "fixture has LT(SymNum, BSPC); rule must fire"
    );
}

#[test]
fn fixture_overlay_dangling_keycode_correctly_classifies() {
    let (_td, project, layout) = fixture_project_and_layout();
    let issues = lint::run_all(&layout, &project).unwrap();
    let dangling: Vec<&oryx_bench::lint::Issue> = issues
        .iter()
        .filter(|i| i.rule_id == "overlay-dangling-keycode")
        .collect();
    // The fixture has a key_override for BSPC (bound on the right thumb
    // via LT) and one for ESC (NOT bound anywhere — a documented
    // example of cross-tier mismatch the rule catches). So the rule
    // should fire for ESC but not for BSPC.
    let messages: Vec<&str> = dangling.iter().map(|i| i.message.as_str()).collect();
    assert!(
        !messages.iter().any(|m| m.contains("'BSPC'")),
        "BSPC is bound on the right thumb; rule should not fire: {dangling:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("'ESC'")),
        "ESC is not bound; rule should fire: {dangling:?}"
    );
}

#[test]
fn fixture_overlay_dangling_position_does_not_fire() {
    let (_td, project, layout) = fixture_project_and_layout();
    let issues = lint::run_all(&layout, &project).unwrap();
    let dangling = issues
        .iter()
        .filter(|i| i.rule_id == "overlay-dangling-position")
        .collect::<Vec<_>>();
    // The fixture has no combo entries with bad position names; rule
    // should be silent on the achordion bindings since they reference
    // existing layers (Sym+Num, Brd+Sys).
    assert!(
        dangling.is_empty(),
        "rule fired against the fixture: {dangling:?}"
    );
}

#[test]
fn fixture_no_unknown_layer_ref() {
    let (_td, project, layout) = fixture_project_and_layout();
    let issues = lint::run_all(&layout, &project).unwrap();
    let unknown = issues
        .iter()
        .filter(|i| i.rule_id == "unknown-layer-ref")
        .collect::<Vec<_>>();
    assert!(unknown.is_empty(), "rule fired on the fixture: {unknown:?}");
}

#[test]
fn fixture_orphaned_mod_tap_fires_on_real_layout() {
    // The voyager-dvorak fixture has positions encoded as
    // `tap=null, hold=Modifier(LSFT/LCTL/...)` — orphaned mod-taps that
    // the rule should catch.
    let (_td, project, layout) = fixture_project_and_layout();
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(
        issues.iter().any(|i| i.rule_id == "orphaned-mod-tap"),
        "fixture has orphaned mod-taps; rule must fire"
    );
}

#[test]
fn fixture_home_row_mods_asymmetric_does_not_fire_on_balanced_layout() {
    // The voyager-dvorak fixture has 2 orphaned mod-taps on the left
    // (LSHIFT at L_outer_home, LCTL at L_outer_bottom) AND a real mod-tap
    // on the right thumb (LALT_T(ENTER) at R_thumb_inner). Both halves
    // have mods, so the rule should NOT fire.
    let (_td, project, layout) = fixture_project_and_layout();
    let issues = lint::run_all(&layout, &project).unwrap();
    assert!(
        !issues
            .iter()
            .any(|i| i.rule_id == "home-row-mods-asymmetric"),
        "fixture has mods on both halves; rule should not fire"
    );
}

#[test]
fn clean_layout_has_no_issues() {
    let (_td, project) = test_project_with_features("");
    let layout = basic_layout();
    let issues = lint::run_all(&layout, &project).unwrap();
    // An empty layout trivially has no issues except possibly unreachable-layer.
    // SymNum is position 1 and is unreferenced → expected to fire.
    // We only assert there are no ERROR-level issues other than unreachable-layer.
    for issue in &issues {
        if matches!(issue.severity, Severity::Error) {
            assert_eq!(
                issue.rule_id, "unreachable-layer",
                "unexpected error: {issue:?}"
            );
        }
    }
}
