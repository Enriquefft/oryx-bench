//! Round-trip test: from_oryx -> render_layout_toml -> from_local must produce
//! the same canonical layout.
//!
//! This test would have caught the `from_local` normalization bug where
//! tap+hold combinations were not collapsed into LT/ModTap in the local
//! parsing path (fixed in the `normalize_tap_hold` commit).

use oryx_bench::schema::canonical::{CanonicalAction, CanonicalLayout};
use oryx_bench::schema::layout;
use oryx_bench::schema::oryx;

fn load_oryx_canonical() -> CanonicalLayout {
    let raw = include_str!("../examples/voyager-dvorak/pulled/revision.json");
    let oryx_layout: oryx::Layout = serde_json::from_str(raw).unwrap();
    CanonicalLayout::from_oryx(&oryx_layout).unwrap()
}

/// Both `KC_NO` and `KC_TRNS` are semantically "inactive" positions. The
/// Oryx fixture uses `KC_TRNS` for fall-through keys on non-base layers,
/// but `render_layout_toml` treats both as empty and omits them. When
/// `from_local` reads back without `inherit`, those positions become
/// `KC_NO`. For the round-trip contract, both are equivalent: the
/// position is inactive.
fn is_inactive(action: &Option<CanonicalAction>) -> bool {
    matches!(
        action,
        None | Some(CanonicalAction::None) | Some(CanonicalAction::Transparent)
    )
}

#[test]
fn oryx_to_layout_toml_to_local_produces_equivalent_canonical() {
    // Step 1: Load fixture and convert via the Oryx path.
    let oryx_canonical = load_oryx_canonical();

    // Step 2: Render to layout.toml text.
    let layout_toml = layout::render_layout_toml(&oryx_canonical).unwrap();

    // Step 3: Parse the layout.toml back into a LayoutFile.
    let parsed: layout::LayoutFile =
        toml::from_str(&layout_toml).expect("rendered layout.toml must parse back cleanly");

    // Step 4: Convert to canonical via the local path.
    let local_canonical = CanonicalLayout::from_local(&parsed).unwrap();

    // Step 5: Compare structure.
    // -- Geometry and title must match.
    assert_eq!(
        oryx_canonical.geometry.as_str(),
        local_canonical.geometry.as_str(),
        "geometry mismatch"
    );
    assert_eq!(
        oryx_canonical.title, local_canonical.title,
        "title mismatch"
    );

    // -- Same number of layers.
    assert_eq!(
        oryx_canonical.layers.len(),
        local_canonical.layers.len(),
        "layer count mismatch"
    );

    // -- Compare every layer name, position, and key display string.
    //    For active keys (tap/hold that are neither KC_NO nor KC_TRNS),
    //    the display strings must match exactly. For inactive keys, both
    //    sides must agree the position is inactive (KC_NO or KC_TRNS are
    //    interchangeable — see `is_inactive` doc comment above).
    for (o_layer, l_layer) in oryx_canonical
        .layers
        .iter()
        .zip(local_canonical.layers.iter())
    {
        assert_eq!(o_layer.name, l_layer.name, "layer name mismatch");
        assert_eq!(
            o_layer.position, l_layer.position,
            "layer position mismatch (layer {})",
            o_layer.name
        );
        assert_eq!(
            o_layer.keys.len(),
            l_layer.keys.len(),
            "key count mismatch in layer '{}'",
            o_layer.name
        );
        for (i, (o_key, l_key)) in o_layer.keys.iter().zip(l_layer.keys.iter()).enumerate() {
            let o_active = !is_inactive(&o_key.tap) || !is_inactive(&o_key.hold);
            let l_active = !is_inactive(&l_key.tap) || !is_inactive(&l_key.hold);
            assert_eq!(
                o_active,
                l_active,
                "active/inactive mismatch in layer '{}' at index {} \
                 (oryx tap={} hold={}, local tap={} hold={})",
                o_layer.name,
                i,
                o_key.display(),
                o_key.hold.as_ref().map(|a| a.display()).unwrap_or_default(),
                l_key.display(),
                l_key.hold.as_ref().map(|a| a.display()).unwrap_or_default(),
            );
            if o_active {
                assert_eq!(
                    o_key.display(),
                    l_key.display(),
                    "active key mismatch in layer '{}' at index {}",
                    o_layer.name,
                    i
                );
            }
        }
    }

    // Note: combos and config are Oryx-only data that do not survive the
    // layout.toml round-trip by design. `from_local` always produces empty
    // combos and config. This is expected and correct — layout.toml is the
    // hand-editable subset, not a lossless Oryx archive.
}
