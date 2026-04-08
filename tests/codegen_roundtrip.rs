//! Codegen correctness contract.
//!
//! Two layers of assertion:
//!
//! 1. **Structural round-trip** — parse the generated `keymap.c` source,
//!    extract each `LAYOUT_voyager(...)` block's positional arguments,
//!    walk them through the QMK arg order back to canonical indices,
//!    and verify that every key in the canonical layout appears at the
//!    expected position in the generated output. This is the
//!    deterministic, no-external-tools version of the codegen contract
//!    and catches the entire class of bugs we hit in M2 (off-by-N
//!    permutation, dropped hold side, wrong layer name resolution).
//!
//! 2. **`qmk c2json` integration** — when `qmk` is on PATH, additionally
//!    pipe the generated source through `qmk c2json` to verify it's
//!    structurally parseable by the upstream tool. Skips cleanly when
//!    `qmk` is unavailable so the contract test is meaningful in CI
//!    environments without the QMK toolchain installed.

use std::process::Command;

use oryx_bench::generate;
use oryx_bench::schema::canonical::{CanonicalAction, CanonicalLayout};
use oryx_bench::schema::features::FeaturesToml;
use oryx_bench::schema::geometry;
use oryx_bench::schema::oryx;

fn load_fixture() -> CanonicalLayout {
    let raw = include_str!("../examples/voyager-dvorak/pulled/revision.json");
    let oryx_layout: oryx::Layout = serde_json::from_str(raw).unwrap();
    CanonicalLayout::from_oryx(&oryx_layout).unwrap()
}

/// Extract the body of every `[<IDENT>] = LAYOUT_voyager(...)` block,
/// returning `(layer_ident, args)` pairs where `args` is the comma-split
/// list of QMK keycode tokens in argument order.
fn parse_layout_blocks(keymap_c: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut rest = keymap_c;
    while let Some(start) = rest.find("] = LAYOUT_voyager(") {
        // Find the layer ident: scan backwards from `start` for `[`.
        let head = &rest[..start];
        let bracket = head.rfind('[').expect("matching `[` for `]`");
        let ident = head[bracket + 1..].trim().to_string();

        // Find the matching closing `)`. The body is balanced parens
        // (LT(L, X), LCTL_T(KC_A), …) so track depth.
        let body_start = start + "] = LAYOUT_voyager(".len();
        let mut depth = 1usize;
        let mut i = body_start;
        let bytes = rest.as_bytes();
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        assert!(depth == 0, "unbalanced LAYOUT_voyager(...) block");
        let body = &rest[body_start..i];

        // Split top-level commas in the body.
        let args = split_top_level_args(body);
        out.push((ident, args));
        rest = &rest[i + 1..];
    }
    out
}

fn split_top_level_args(body: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in body.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    args.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        args.push(trimmed.to_string());
    }
    args
}

#[test]
fn structural_round_trip_against_fixture() {
    let canonical = load_fixture();
    let geom = geometry::get("voyager").unwrap();
    let features = FeaturesToml::default();
    let generated = generate::generate_all(&canonical, &features, geom, None).unwrap();

    // Pull the LAYOUT blocks out of the generated source.
    let blocks = parse_layout_blocks(&generated.keymap_c);
    assert_eq!(
        blocks.len(),
        canonical.layers.len(),
        "expected one LAYOUT_voyager block per canonical layer"
    );

    let qmk_order = geom.qmk_arg_order();

    // For each emitted block, walk it back through the QMK arg order
    // and verify the canonical key at each index renders to the same
    // string the generator emitted.
    for (block_ident, args) in &blocks {
        assert_eq!(
            args.len(),
            geom.matrix_key_count(),
            "block {block_ident} has wrong arg count"
        );

        // Find the canonical layer whose sanitized ident matches.
        let layer = canonical
            .layers
            .iter()
            .find(|l| sanitize(&l.name) == *block_ident)
            .unwrap_or_else(|| panic!("no canonical layer matches block ident {block_ident}"));

        for (qmk_pos, arg) in args.iter().enumerate() {
            let canonical_idx = qmk_order[qmk_pos];
            let key = &layer.keys[canonical_idx];
            let expected = render_key(key);
            assert_eq!(
                arg, &expected,
                "block {block_ident}: QMK pos {qmk_pos} (canonical idx {canonical_idx}) — \
                 generated `{arg}` but canonical key renders as `{expected}`"
            );
        }
    }
}

/// Re-render a canonical key using the same logic as `keymap.rs::emit_key`,
/// for round-trip comparison. We can't reach into the generator's private
/// `emit_key` function from an integration test, so we re-derive the string
/// form here using the public canonical action display + collapse rules.
fn render_key(key: &oryx_bench::schema::canonical::CanonicalKey) -> String {
    use oryx_bench::schema::canonical::LayerRef;
    fn render_action(action: &CanonicalAction) -> String {
        match action {
            CanonicalAction::Keycode(kc) => kc.canonical_name().into_owned(),
            CanonicalAction::Modifier(m) => format!("KC_{}", m.canonical_name()),
            CanonicalAction::Mo { layer } => format!("MO({})", render_layer(layer)),
            CanonicalAction::Tg { layer } => format!("TG({})", render_layer(layer)),
            CanonicalAction::To { layer } => format!("TO({})", render_layer(layer)),
            CanonicalAction::Tt { layer } => format!("TT({})", render_layer(layer)),
            CanonicalAction::Df { layer } => format!("DF({})", render_layer(layer)),
            CanonicalAction::Lt { layer, tap } => {
                format!("LT({}, {})", render_layer(layer), render_action(tap))
            }
            CanonicalAction::ModTap { mod_, tap } => {
                format!("{}_T({})", mod_.canonical_name(), render_action(tap))
            }
            CanonicalAction::Modified { mods, base } => {
                let mut out = render_action(base);
                for m in mods.iter().rev() {
                    out = format!("{}({})", m.canonical_name(), out);
                }
                out
            }
            CanonicalAction::Custom(_) => "KC_NO /* missing CK for slot */".into(),
            CanonicalAction::Transparent => "KC_TRNS".into(),
            CanonicalAction::None => "KC_NO".into(),
        }
    }
    fn render_layer(r: &LayerRef) -> String {
        match r {
            LayerRef::Name(n) => sanitize(n),
            LayerRef::Index(i) => i.to_string(),
        }
    }
    match (&key.tap, &key.hold) {
        (Some(t), Some(h)) => {
            let synthetic = match (t, h) {
                (CanonicalAction::Keycode(_), CanonicalAction::Mo { layer }) => {
                    CanonicalAction::Lt {
                        layer: layer.clone(),
                        tap: Box::new(t.clone()),
                    }
                }
                (CanonicalAction::Keycode(_), CanonicalAction::Modifier(m)) => {
                    CanonicalAction::ModTap {
                        mod_: m.clone(),
                        tap: Box::new(t.clone()),
                    }
                }
                _ => t.clone(),
            };
            render_action(&synthetic)
        }
        (Some(a), None) | (None, Some(a)) => render_action(a),
        (None, None) => "KC_NO".to_string(),
    }
}

fn sanitize(name: &str) -> String {
    oryx_bench::schema::naming::sanitize_c_ident(name)
}

#[test]
fn keymap_c_is_parseable_by_qmk_when_available() {
    if which::which("qmk").is_err() {
        eprintln!("skip: `qmk` not on PATH (install qmk to enable the c2json check)");
        return;
    }

    let canonical = load_fixture();
    let geom = geometry::get("voyager").unwrap();
    let features = FeaturesToml::default();
    let generated = generate::generate_all(&canonical, &features, geom, None).unwrap();

    let td = tempfile::TempDir::new().unwrap();
    let keymap_path = td.path().join("keymap.c");
    std::fs::write(&keymap_path, &generated.keymap_c).unwrap();
    let out = Command::new("qmk")
        .args([
            "c2json",
            "--no-cpp",
            "-kb",
            "zsa/voyager",
            "-km",
            "oryx-bench",
        ])
        .arg(&keymap_path)
        .output()
        .expect("invoking qmk c2json");
    assert!(
        out.status.success(),
        "qmk c2json rejected the generated keymap.c:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn generator_with_features_produces_features_c() {
    let raw = include_str!("../examples/voyager-dvorak/pulled/revision.json");
    let oryx_layout: oryx::Layout = serde_json::from_str(raw).unwrap();
    let canonical = CanonicalLayout::from_oryx(&oryx_layout).unwrap();
    let geom = geometry::get("voyager").unwrap();
    let features_raw = include_str!("../examples/voyager-dvorak/overlay/features.toml");
    let features: FeaturesToml = toml::from_str(features_raw).unwrap();
    let gen = generate::generate_all(&canonical, &features, geom, None).unwrap();

    // Key overrides emit a key_override_t table.
    assert!(gen.features_c.contains("ko_make_basic"));
    assert!(gen.features_c.contains("MOD_LSHIFT"));
    // config.h reflects the [config] section.
    assert!(gen.config_h.contains("#define TAPPING_TERM_MS 220"));
    // rules.mk enables KEY_OVERRIDE_ENABLE.
    assert!(gen.rules_mk.contains("KEY_OVERRIDE_ENABLE = yes"));
}
