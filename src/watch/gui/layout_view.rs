//! Pixel-precise split-keyboard renderer.
//!
//! Reuses the canonical `GridLayout` from the geometry trait — the same
//! description the ASCII renderer consumes — to place key rectangles
//! in physical split-half order. Works with any geometry that
//! implements the trait; no per-keyboard GUI code.

use egui::{Align2, Color32, Context, FontFamily, FontId, Rect, Stroke, Ui, Vec2};

use crate::render::ascii::friendly_key;
use crate::schema::canonical::{CanonicalLayer, CanonicalLayout};
use crate::schema::geometry::{Geometry, ThumbKeyWidth};

use super::theme;

const KEY_SIZE: f32 = 48.0;
const KEY_GAP: f32 = 4.0;
const HALF_GAP: f32 = 40.0;
const ROW_GAP: f32 = 4.0;
const THUMB_SPACING: f32 = 8.0;

/// Options for a single frame's render.
pub struct RenderOpts<'a> {
    pub layout: &'a CanonicalLayout,
    pub geometry: &'a dyn Geometry,
    pub active_layer: Option<usize>,
    /// Matrix indices to highlight above the default coloring (next-key
    /// pulse, combo participants). Empty for the bare indicator.
    pub highlight: &'a [usize],
}

/// Draw the full split keyboard centered in the provided ui's available
/// rect. Returns the pixel rect actually used so callers can stack
/// widgets under it.
pub fn draw(ui: &mut Ui, opts: &RenderOpts<'_>) -> Rect {
    let grid = opts.geometry.ascii_layout();
    let layer = opts
        .active_layer
        .and_then(|idx| opts.layout.layers.get(idx));

    // Width of one half = widest row of that half.
    let widest = |rows: &[crate::schema::geometry::GridRow], side: Side| -> usize {
        rows.iter()
            .map(|r| match side {
                Side::Left => r.left.len(),
                Side::Right => r.right.len(),
            })
            .max()
            .unwrap_or(0)
    };
    let left_cols = widest(grid.rows, Side::Left);
    let right_cols = widest(grid.rows, Side::Right);
    let half_width = cols_to_px(left_cols.max(right_cols));

    let rows_h = grid.rows.len() as f32 * (KEY_SIZE + ROW_GAP) - ROW_GAP;
    let thumbs_h = if grid.thumb_clusters.is_empty() {
        0.0
    } else {
        KEY_SIZE + THUMB_SPACING
    };
    let total_w = half_width * 2.0 + HALF_GAP;
    let total_h = rows_h + thumbs_h;

    let avail = ui.available_rect_before_wrap();
    let origin = egui::pos2(
        avail.center().x - total_w / 2.0,
        avail.center().y - total_h / 2.0,
    );

    let painter = ui.painter_at(avail);

    // Rows.
    let ctx = ui.ctx().clone();
    for (row_idx, row) in grid.rows.iter().enumerate() {
        let y = origin.y + row_idx as f32 * (KEY_SIZE + ROW_GAP);
        // Right-align the right half against its widest row so the
        // halves mirror physically when a row is short.
        let right_offset =
            origin.x + half_width + HALF_GAP + (half_width - cols_to_px(row.right.len()));
        draw_row(&ctx, &painter, row.left, origin.x, y, layer, opts);
        draw_row(&ctx, &painter, row.right, right_offset, y, layer, opts);
    }

    // Thumb clusters below the main grid.
    if !grid.thumb_clusters.is_empty() {
        let thumb_y = origin.y + rows_h + THUMB_SPACING;
        for cluster in grid.thumb_clusters {
            let mut x = match cluster.hand {
                crate::schema::geometry::Hand::Left => {
                    origin.x + half_width - thumb_cluster_width(cluster)
                }
                crate::schema::geometry::Hand::Right => origin.x + half_width + HALF_GAP,
            };
            for key in cluster.keys {
                let w = match key.width {
                    ThumbKeyWidth::Standard => KEY_SIZE,
                    ThumbKeyWidth::Wide => KEY_SIZE * 1.5,
                };
                let rect = Rect::from_min_size(egui::pos2(x, thumb_y), Vec2::new(w, KEY_SIZE));
                draw_key(&ctx, &painter, rect, Some(key.index), layer, opts);
                x += w + KEY_GAP;
            }
        }
    }

    Rect::from_min_size(origin, Vec2::new(total_w, total_h))
}

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

fn cols_to_px(n: usize) -> f32 {
    if n == 0 {
        0.0
    } else {
        n as f32 * KEY_SIZE + (n - 1) as f32 * KEY_GAP
    }
}

fn thumb_cluster_width(cluster: &crate::schema::geometry::ThumbCluster) -> f32 {
    let mut w = 0.0;
    for (i, key) in cluster.keys.iter().enumerate() {
        if i > 0 {
            w += KEY_GAP;
        }
        w += match key.width {
            ThumbKeyWidth::Standard => KEY_SIZE,
            ThumbKeyWidth::Wide => KEY_SIZE * 1.5,
        };
    }
    w
}

fn draw_row(
    ctx: &Context,
    painter: &egui::Painter,
    row: &[Option<usize>],
    start_x: f32,
    y: f32,
    layer: Option<&CanonicalLayer>,
    opts: &RenderOpts<'_>,
) {
    for (col, slot) in row.iter().enumerate() {
        let x = start_x + col as f32 * (KEY_SIZE + KEY_GAP);
        let rect = Rect::from_min_size(egui::pos2(x, y), Vec2::splat(KEY_SIZE));
        draw_key(ctx, painter, rect, *slot, layer, opts);
    }
}

fn draw_key(
    ctx: &Context,
    painter: &egui::Painter,
    rect: Rect,
    idx: Option<usize>,
    layer: Option<&CanonicalLayer>,
    opts: &RenderOpts<'_>,
) {
    let Some(i) = idx else { return };
    let highlighted = opts.highlight.contains(&i);
    let fill = if highlighted {
        theme::KEY_ACCENT
    } else {
        theme::KEY
    };
    let stroke = Stroke::new(
        1.0,
        if highlighted {
            Color32::WHITE
        } else {
            Color32::from_black_alpha(160)
        },
    );
    painter.rect(rect, 6.0, fill, stroke);

    let label = layer
        .and_then(|l| l.keys.get(i))
        .map(|k| friendly_key(k, &opts.layout.layers))
        .unwrap_or_default();
    if !label.is_empty() {
        let color = if highlighted {
            Color32::BLACK
        } else {
            theme::TEXT
        };
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            &label,
            fit_font(ctx, &label, rect.width()),
            color,
        );
    }
}

/// Pick a font size that keeps `label` inside `max_width` (with a small
/// padding). Prevents labels like `RALT(KC_NO)` or `ENT/Symbo` from
/// clipping past the key cell.
///
/// Uses egui's real text layouter — `Fonts::layout_no_wrap` — so the
/// measured width is exactly what the painter will render, even across
/// non-ASCII (combining marks, emoji, variable-width glyphs). No
/// character-width heuristic; no magic constants beyond the font-size
/// range we're willing to shrink into.
fn fit_font(ctx: &Context, label: &str, max_width: f32) -> FontId {
    const DEFAULT: f32 = 13.0;
    const MIN: f32 = 8.5;
    const PADDING: f32 = 6.0;
    let budget = (max_width - PADDING).max(1.0);

    // Ask egui to lay the label out at `size` and return the width. We
    // binary-step from DEFAULT down by small increments; in practice
    // one or two iterations converge, and the worst case is
    // O((DEFAULT-MIN)/step) ≈ 10 layouts per key, which is well below
    // the per-frame budget.
    let measure = |size: f32| -> f32 {
        let font = FontId::new(size, FontFamily::Proportional);
        ctx.fonts(|f| {
            f.layout_no_wrap(label.to_owned(), font, Color32::WHITE)
                .size()
                .x
        })
    };

    if measure(DEFAULT) <= budget {
        return FontId::proportional(DEFAULT);
    }
    // Step down 0.5pt at a time; clamped at MIN. A label that can't
    // fit even at MIN clips gracefully — that's an extreme-long-label
    // edge case the layout itself should flag in `lint`.
    let mut size = DEFAULT - 0.5;
    while size > MIN {
        if measure(size) <= budget {
            return FontId::proportional(size);
        }
        size -= 0.5;
    }
    FontId::proportional(MIN)
}
