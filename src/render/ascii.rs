//! Hand-rolled split-grid renderer.
//!
//! Not built on `tabled` — the Voyager's shape doesn't fit `tabled`'s
//! rectangular model. ~100 lines of straightforward formatting code.

use crate::schema::canonical::CanonicalLayer;
use crate::schema::geometry::{Geometry, GridLayout, Hand, ThumbCluster};

use super::RenderOptions;

/// Render a single layer as an ASCII split-grid keyboard picture.
pub fn render_layer(geom: &dyn Geometry, layer: &CanonicalLayer, opts: &RenderOptions) -> String {
    let grid = geom.ascii_layout();
    // Fits "KC_BSPC" and shorter. Longer names are truncated and the
    // final visible character is replaced with `…` so the user can
    // see at a glance that the displayed text is not the full binding.
    const CELL_WIDTH: usize = 7;

    // Pre-format each cell.
    let cell = |idx: usize| -> String {
        let s = if opts.show_position_names {
            geom.index_to_position(idx).unwrap_or("?").to_string()
        } else {
            layer
                .keys
                .get(idx)
                .map(|k| k.display())
                .unwrap_or_else(|| "KC_NO".to_string())
        };
        let formatted = truncate_with_ellipsis(&s, CELL_WIDTH);
        format!("{formatted:<CELL_WIDTH$}")
    };

    let mut out = String::new();

    // Main matrix rows
    for row in grid.rows {
        let mut line = String::new();
        for maybe_idx in row.left {
            line.push('|');
            line.push_str(&match *maybe_idx {
                Some(i) => cell(i),
                None => " ".repeat(CELL_WIDTH),
            });
        }
        line.push('|');
        // Gap between halves
        line.push_str("    ");
        for maybe_idx in row.right {
            line.push('|');
            line.push_str(&match *maybe_idx {
                Some(i) => cell(i),
                None => " ".repeat(CELL_WIDTH),
            });
        }
        line.push('|');
        out.push_str(&line);
        out.push('\n');
    }

    // Thumb clusters — render below the main matrix, aligned under the
    // two halves they belong to.
    if !grid.thumb_clusters.is_empty() {
        out.push('\n');
        let (left_thumbs, right_thumbs) = split_thumbs_by_hand(grid);
        let max_len = left_thumbs.len().max(right_thumbs.len());
        for i in 0..max_len {
            let left_cell = left_thumbs
                .get(i)
                .map(|&idx| cell(idx))
                .unwrap_or_else(|| " ".repeat(CELL_WIDTH));
            let right_cell = right_thumbs
                .get(i)
                .map(|&idx| cell(idx))
                .unwrap_or_else(|| " ".repeat(CELL_WIDTH));
            out.push_str(&format!(
                "{:>pad$}|{left_cell}|    |{right_cell}|\n",
                "",
                pad = CELL_WIDTH * 4 + 5,
            ));
        }
    }
    out
}

/// Truncate `s` to at most `width` characters. When truncation happens
/// the last kept character is replaced with `…` so the user can tell
/// at a glance that the cell is eliding content rather than showing
/// the full binding. (The previous renderer silently cut characters
/// off the right edge, which made `LT(Sym+Num, KC_BSPC)` and
/// `LT(Sym+Num,` look identical in the grid.)
fn truncate_with_ellipsis(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn split_thumbs_by_hand(grid: &'static GridLayout) -> (Vec<usize>, Vec<usize>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for cluster in grid.thumb_clusters {
        let ThumbCluster { hand, keys } = cluster;
        match hand {
            Hand::Left => left.extend_from_slice(keys),
            Hand::Right => right.extend_from_slice(keys),
        }
    }
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::canonical::CanonicalKey;
    use crate::schema::geometry::voyager::Voyager;

    #[test]
    fn renders_empty_layer() {
        let layer = CanonicalLayer {
            name: "Test".into(),
            position: 0,
            keys: vec![CanonicalKey::default(); 52],
        };
        let out = render_layer(&Voyager, &layer, &RenderOptions::default());
        assert!(out.contains("KC_NO"));
        assert!(out.lines().count() > 4);
    }

    #[test]
    fn truncate_with_ellipsis_no_change_when_fits() {
        assert_eq!(truncate_with_ellipsis("KC_A", 7), "KC_A");
        assert_eq!(truncate_with_ellipsis("KC_BSPC", 7), "KC_BSPC");
    }

    #[test]
    fn truncate_with_ellipsis_appends_marker() {
        // 7 chars kept — 6 first chars + ellipsis.
        assert_eq!(truncate_with_ellipsis("LT(Sym+Num, KC_BSPC)", 7), "LT(Sym…");
    }

    #[test]
    fn truncate_with_ellipsis_handles_zero_width() {
        assert_eq!(truncate_with_ellipsis("anything", 0), "");
    }

    #[test]
    fn truncate_with_ellipsis_width_one() {
        // Width 1 means the ellipsis alone.
        assert_eq!(truncate_with_ellipsis("anything", 1), "…");
    }

    #[test]
    fn renders_position_names_with_flag() {
        let layer = CanonicalLayer {
            name: "Test".into(),
            position: 0,
            keys: vec![CanonicalKey::default(); 52],
        };
        let out = render_layer(
            &Voyager,
            &layer,
            &RenderOptions {
                show_position_names: true,
            },
        );
        // Position names like "L_pinky_num" are wider than our 7-char
        // cell, so they render with the ellipsis truncation marker.
        // Asserting on the shorter prefix + ellipsis keeps the test
        // checking "position names render" without pinning an exact
        // cell width.
        assert!(
            out.contains("L_pink…"),
            "expected truncated position name with ellipsis in output:\n{out}"
        );
    }
}
