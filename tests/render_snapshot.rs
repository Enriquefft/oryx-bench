//! Insta snapshot tests for the ASCII renderer against the voyager-dvorak
//! fixture. Catches column-alignment and thumb-cluster-placement regressions.

use oryx_bench::render::{self, RenderOptions};
use oryx_bench::schema::canonical::CanonicalLayout;
use oryx_bench::schema::geometry;
use oryx_bench::schema::oryx;

fn load_fixture() -> CanonicalLayout {
    let raw = include_str!("../examples/voyager-dvorak/pulled/revision.json");
    let oryx_layout: oryx::Layout = serde_json::from_str(raw).unwrap();
    CanonicalLayout::from_oryx(&oryx_layout).unwrap()
}

fn render_layer_by_name(name: &str, opts: RenderOptions) -> String {
    let layout = load_fixture();
    let geom = geometry::get("voyager").unwrap();
    let layer = layout
        .layers
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no layer named {name}"));
    render::ascii::render_layer(geom, layer, &opts)
}

#[test]
fn snapshot_main_layer() {
    let out = render_layer_by_name("Main", RenderOptions::default());
    insta::assert_snapshot!("main_layer", out);
}

#[test]
fn snapshot_sym_num_layer() {
    let out = render_layer_by_name("Sym+Num", RenderOptions::default());
    insta::assert_snapshot!("sym_num_layer", out);
}

#[test]
fn snapshot_brd_sys_layer() {
    let out = render_layer_by_name("Brd+Sys", RenderOptions::default());
    insta::assert_snapshot!("brd_sys_layer", out);
}

#[test]
fn snapshot_gaming_layer() {
    let out = render_layer_by_name("Gaming", RenderOptions::default());
    insta::assert_snapshot!("gaming_layer", out);
}

#[test]
fn snapshot_main_with_position_names() {
    let out = render_layer_by_name(
        "Main",
        RenderOptions {
            show_position_names: true,
        },
    );
    insta::assert_snapshot!("main_position_names", out);
}
