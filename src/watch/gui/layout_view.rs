//! Pixel-precise split-keyboard renderer.
//!
//! Drives off `Geometry::physical_layout()` — per-key top-left (x, y)
//! in 1u units plus an optional cluster rotation — so columnar stagger,
//! split gap, and angled thumb clusters are a pure geometry concern.
//! This module is generic across keyboards; nothing here knows about
//! Voyager specifically.
//!
//! Rendering path, per key:
//!   1. Scale 1u → pixels using a single `KEY_UNIT_PX` so the entire
//!      board scales uniformly with the viewport.
//!   2. Translate so the board's bbox is centered in the available rect.
//!   3. If the key's `rot_deg != 0`, rotate the cap's four corners
//!      around its pivot and draw as a convex polygon with a rotated
//!      text label. Un-rotated keys take the fast rect path.

use egui::{
    epaint::{PathShape, TextShape},
    Align2, Color32, Context, FontFamily, FontId, Pos2, Rect, Shape, Stroke, Ui, Vec2,
};

use crate::render::ascii::friendly_key;
use crate::schema::canonical::{CanonicalLayer, CanonicalLayout};
use crate::schema::geometry::{Geometry, PhysicalKey};

use super::theme;

/// 1u in pixels. The entire layout scales linearly with this constant,
/// so it's also the knob for "make the keys bigger" complaints.
const KEY_UNIT_PX: f32 = 56.0;
/// Cap footprint as a fraction of a 1u cell. Leaves a thin gutter
/// between keys without dropping back to a separate `GAP` constant —
/// the gutter is implicit in the cap shrinking by (1 - factor).
const CAP_FACTOR: f32 = 0.92;
/// Corner radius in pixels for non-rotated caps. Rotated caps fall
/// back to sharp corners (rotated rounded-rect would require bezier
/// path work egui doesn't expose directly, and the thumb clusters
/// still read fine with square corners at this size).
const CAP_CORNER_PX: f32 = 6.0;
/// Bottom padding in 1u units. Without this the rotated thumb corner
/// can clip the footer.
const BBOX_BOTTOM_PAD: f32 = 0.25;

/// Options for a single frame's render.
pub struct RenderOpts<'a> {
    pub layout: &'a CanonicalLayout,
    pub geometry: &'a dyn Geometry,
    pub active_layer: Option<usize>,
    /// Matrix indices to highlight above the default coloring (next-key
    /// pulse, combo participants). Empty for the bare indicator.
    pub highlight: &'a [usize],
    /// Matrix indices the firmware currently reports as held down.
    /// Drawn distinct from `highlight` so a combo-hint and a real press
    /// never alias.
    pub pressed: &'a [usize],
}

/// Draw the full split keyboard centered in the provided ui's available
/// rect. Returns the pixel rect actually used.
pub fn draw(ui: &mut Ui, opts: &RenderOpts<'_>) -> Rect {
    let physical = opts.geometry.physical_layout();
    let layer = opts
        .active_layer
        .and_then(|idx| opts.layout.layers.get(idx));

    let avail = ui.available_rect_before_wrap();

    // Auto-fit: pick the scale that makes the board fill the
    // smaller dimension with a small margin. KEY_UNIT_PX is the
    // "preferred" size; we shrink below it if needed, and never
    // exceed it (tiny layouts don't balloon to fill a huge window).
    const MARGIN_PX: f32 = 24.0;
    let scale_x = (avail.width() - MARGIN_PX) / physical.width;
    let scale_y = (avail.height() - MARGIN_PX) / (physical.height + BBOX_BOTTOM_PAD);
    let scale = scale_x.min(scale_y).clamp(16.0, KEY_UNIT_PX);

    let total_w = physical.width * scale;
    let total_h = physical.height * scale;
    let origin = egui::pos2(
        avail.center().x - total_w / 2.0,
        avail.center().y - total_h / 2.0,
    );

    let painter = ui.painter_at(avail);
    let ctx = ui.ctx().clone();

    for pk in physical.keys {
        draw_physical_key(&ctx, &painter, origin, scale, pk, layer, opts);
    }

    Rect::from_min_size(origin, Vec2::new(total_w, total_h))
}

fn draw_physical_key(
    ctx: &Context,
    painter: &egui::Painter,
    origin: Pos2,
    scale: f32,
    pk: &PhysicalKey,
    layer: Option<&CanonicalLayer>,
    opts: &RenderOpts<'_>,
) {
    // Press state dominates highlight state — when the user is actually
    // pressing a combo-participant, we want to see the press, not the
    // compositional hint that says "this key is part of a combo".
    let pressed = opts.pressed.contains(&pk.index);
    let highlighted = !pressed && opts.highlight.contains(&pk.index);
    let glow = layer
        .and_then(|l| l.keys.get(pk.index))
        .and_then(|k| k.glow_color)
        .map(rgb_to_color32);

    let fill = if pressed {
        theme::KEY_PRESSED
    } else if highlighted {
        theme::KEY_ACCENT
    } else {
        theme::KEY
    };
    let stroke_width = if pressed { 1.8 } else { 1.0 };
    let stroke_color = if pressed || highlighted {
        Color32::WHITE
    } else if let Some(c) = glow {
        // Colored glow only surfaces when the cap isn't already
        // shouting at the user for a more urgent reason.
        c
    } else {
        Color32::from_black_alpha(160)
    };
    let stroke = Stroke::new(stroke_width, stroke_color);

    // Pre-rotation corner positions in pixel space, inset by CAP_FACTOR
    // so the gutter is automatic regardless of scale.
    let unit = scale;
    let cap_w = pk.w * unit * CAP_FACTOR;
    let cap_h = pk.h * unit * CAP_FACTOR;
    let inset = (1.0 - CAP_FACTOR) * 0.5;
    let key_origin_x = origin.x + (pk.x + inset * pk.w) * unit;
    let key_origin_y = origin.y + (pk.y + inset * pk.h) * unit;
    let center = Pos2::new(key_origin_x + cap_w / 2.0, key_origin_y + cap_h / 2.0);

    let label = layer
        .and_then(|l| l.keys.get(pk.index))
        .map(|k| friendly_key(k, &opts.layout.layers))
        .unwrap_or_default();
    let text_color = if pressed || highlighted {
        Color32::BLACK
    } else {
        theme::TEXT
    };

    if pk.rot_deg == 0.0 {
        let rect = Rect::from_min_size(
            Pos2::new(key_origin_x, key_origin_y),
            Vec2::new(cap_w, cap_h),
        );
        // Underlay a soft color halo for keys carrying a glow_color.
        if let Some(c) = glow {
            if !pressed && !highlighted {
                paint_halo_rect(painter, rect, c);
            }
        }
        painter.rect(rect, CAP_CORNER_PX, fill, stroke);
        if !label.is_empty() {
            painter.text(
                center,
                Align2::CENTER_CENTER,
                &label,
                fit_font(ctx, &label, cap_w),
                text_color,
            );
        }
        return;
    }

    // Rotated cap. Compute the four corners around the pivot, emit a
    // convex polygon for the cap + a rotated text shape for the label.
    let pivot = Pos2::new(
        origin.x + pk.rot_origin_x * unit,
        origin.y + pk.rot_origin_y * unit,
    );
    let theta = pk.rot_deg.to_radians();
    let rotate = |p: Pos2| -> Pos2 {
        let dx = p.x - pivot.x;
        let dy = p.y - pivot.y;
        let (s, c) = theta.sin_cos();
        Pos2::new(pivot.x + dx * c - dy * s, pivot.y + dx * s + dy * c)
    };
    let tl = Pos2::new(key_origin_x, key_origin_y);
    let tr = Pos2::new(key_origin_x + cap_w, key_origin_y);
    let br = Pos2::new(key_origin_x + cap_w, key_origin_y + cap_h);
    let bl = Pos2::new(key_origin_x, key_origin_y + cap_h);
    let corners = [rotate(tl), rotate(tr), rotate(br), rotate(bl)];

    if let Some(c) = glow {
        if !pressed && !highlighted {
            paint_halo_polygon(painter, &corners, c);
        }
    }
    painter.add(Shape::Path(PathShape::convex_polygon(
        corners.to_vec(),
        fill,
        stroke,
    )));

    if !label.is_empty() {
        let rotated_center = rotate(center);
        let font = fit_font(ctx, &label, cap_w);
        let galley = ctx.fonts(|f| f.layout_no_wrap(label.clone(), font, text_color));
        // `TextShape::angle` rotates around `pos`, which is the top-left
        // of the galley — shift so the rotation pivots around the
        // visual center instead.
        let galley_size = galley.size();
        let offset = Vec2::new(-galley_size.x / 2.0, -galley_size.y / 2.0);
        let (s, c) = theta.sin_cos();
        let rotated_offset = Vec2::new(offset.x * c - offset.y * s, offset.x * s + offset.y * c);
        let mut text_shape = TextShape::new(rotated_center + rotated_offset, galley, text_color);
        text_shape.angle = theta;
        painter.add(Shape::Text(text_shape));
    }
}

/// A soft colored bloom behind the cap — read as "this key's PCB LED
/// is set to this color" without competing with the cap fill for
/// foreground attention.
fn paint_halo_rect(painter: &egui::Painter, rect: Rect, color: Color32) {
    let bloom = rect.expand(6.0);
    let faded = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 90);
    painter.rect_filled(bloom, CAP_CORNER_PX + 4.0, faded);
}

fn paint_halo_polygon(painter: &egui::Painter, corners: &[Pos2; 4], color: Color32) {
    // Extrude each corner outward from the polygon centroid by a
    // small, scale-agnostic amount. Cheaper than offsetting along
    // edge normals and visually indistinguishable at this scale.
    let cx = corners.iter().map(|p| p.x).sum::<f32>() / 4.0;
    let cy = corners.iter().map(|p| p.y).sum::<f32>() / 4.0;
    let faded = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 90);
    let expanded: Vec<Pos2> = corners
        .iter()
        .map(|p| {
            let dx = p.x - cx;
            let dy = p.y - cy;
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            Pos2::new(p.x + dx / len * 6.0, p.y + dy / len * 6.0)
        })
        .collect();
    painter.add(Shape::Path(PathShape::convex_polygon(
        expanded,
        faded,
        Stroke::NONE,
    )));
}

fn rgb_to_color32((r, g, b): (u8, u8, u8)) -> Color32 {
    Color32::from_rgb(r, g, b)
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
    let mut size = DEFAULT - 0.5;
    while size > MIN {
        if measure(size) <= budget {
            return FontId::proportional(size);
        }
        size -= 0.5;
    }
    FontId::proportional(MIN)
}
