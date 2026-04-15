//! ZSA Voyager — 52 matrix keys, 0 encoders, split layout with 2-key thumb clusters.
//!
//! ## Position naming scheme
//!
//! Positions are named `<HAND>_<COL>_<ROW>` where:
//!
//! - `HAND` ∈ { `L`, `R` } — left or right half
//! - `COL` ∈ { `outer`, `pinky`, `ring`, `middle`, `index`, `inner` } —
//!   the physical column. `outer` is the leftmost extension column on
//!   the left half (and the rightmost on the right half — the outermost
//!   edge column). `inner` is the column closest to the keyboard's
//!   split gap (the second-from-thumb column in standard QWERTY/Dvorak).
//! - `ROW` ∈ { `num`, `top`, `home`, `bottom` } — top to bottom
//!
//! Thumb keys are named `L_thumb_inner` / `L_thumb_outer` (and mirror
//! for the right half), where "inner" is the thumb key closer to the
//! split gap.
//!
//! ## Matrix indices
//!
//! Two index orderings are relevant: Oryx's `keys[]` serialization
//! order (which is what the canonical layout uses) and QMK's
//! `LAYOUT_voyager(...)` macro positional argument order (which the
//! codegen layer permutes into via `Geometry::qmk_arg_order`).
//!
//! The serialization order Oryx uses is:
//!
//!   indices  0..24  → left half rows 0-3 (4 rows × 6 columns)
//!   indices 24..26  → LEFT THUMB cluster (inner, outer)
//!   indices 26..50  → right half rows 0-3 (mirrored: inner→outer)
//!   indices 50..52  → RIGHT THUMB cluster (inner, outer)
//!
//! Verified by reading `examples/voyager-dvorak/pulled/revision.json`
//! and the QMK fork's `keyboards/zsa/voyager/keyboard.json`.

use super::{
    Geometry, GridLayout, GridRow, Hand, PhysicalKey, PhysicalLayout, ThumbCluster, ThumbKey,
    ThumbKeyWidth,
};

pub struct Voyager;

impl Geometry for Voyager {
    fn id(&self) -> &'static str {
        "voyager"
    }

    fn display_name(&self) -> &'static str {
        "ZSA Voyager"
    }

    fn matrix_key_count(&self) -> usize {
        52
    }

    fn encoder_count(&self) -> usize {
        0
    }

    fn position_to_index(&self, name: &str) -> Option<usize> {
        POSITION_TABLE
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, i)| *i)
    }

    fn index_to_position(&self, index: usize) -> Option<&'static str> {
        POSITION_TABLE
            .iter()
            .find(|(_, i)| *i == index)
            .map(|(n, _)| *n)
    }

    fn matrix_to_index(&self, row: u8, col: u8) -> Option<usize> {
        MATRIX_TABLE
            .iter()
            .find(|(r, c, _)| *r == row && *c == col)
            .map(|(_, _, idx)| *idx)
    }

    fn ascii_layout(&self) -> &'static GridLayout {
        &GRID
    }

    fn physical_layout(&self) -> &'static PhysicalLayout {
        &PHYSICAL
    }

    fn qmk_keyboard(&self) -> &'static str {
        "zsa/voyager"
    }

    fn layout_macro(&self) -> &'static str {
        "LAYOUT_voyager"
    }

    fn qmk_arg_order(&self) -> &'static [usize] {
        &QMK_ARG_ORDER
    }

    fn hand(&self, index: usize) -> Option<Hand> {
        // Oryx serializes the keys[] array in this order:
        //   [0..24]  left half rows 0-3
        //   [24..26] left thumb cluster
        //   [26..50] right half rows 0-3
        //   [50..52] right thumb cluster
        if index < 26 {
            Some(Hand::Left)
        } else if index < 52 {
            Some(Hand::Right)
        } else {
            None
        }
    }

    fn usb_vendor_id(&self) -> &'static str {
        // ZSA Technology Labs USB vendor ID; matches the value in
        // QMK's keyboards/zsa/voyager/info.json.
        "0x3297"
    }

    fn flash_budget_bytes(&self) -> u64 {
        // Voyager uses an STM32F303 with 64KB of internal flash. The
        // bootloader reserves the first few KB but for this rule's
        // "approaching the budget" warning the headline 64KB number
        // is the right anchor — what fails to link is "image > 64KB",
        // not "image > 60KB".
        64 * 1024
    }
}

// =============================================================================
// Voyager position table
// =============================================================================
//
// The Voyager has 4 rows × 6 columns on each half = 48 matrix keys, plus
// 2 thumb keys per half = 52 total.
//
// Left half (indices 0..24):
//   Row 0:   0  1  2  3  4  5
//   Row 1:   6  7  8  9 10 11
//   Row 2:  12 13 14 15 16 17
//   Row 3:  18 19 20 21 22 23
//
// Right half (indices 24..48):
//   Row 0:  24 25 26 27 28 29
//   Row 1:  30 31 32 33 34 35
//   Row 2:  36 37 38 39 40 41
//   Row 3:  42 43 44 45 46 47
//
// Thumbs:
//   Left:   48 (inner), 49 (outer)
//   Right:  50 (inner), 51 (outer)

// =============================================================================
// Position table — single canonical vocabulary
// =============================================================================
//
// Naming convention is **column-first**: each position is `<HAND>_<COL>_<ROW>`
// where `COL` reflects the *physical column* on the Voyager and `ROW` reflects
// the *physical row*. The Voyager has 6 columns per half:
//
//     outer | pinky | ring | middle | index | inner
//
// where `outer` is the leftmost extension column on the left half (and
// the rightmost on the right half — i.e. the outermost edge column), and
// `inner` is the column closest to the keyboard's split gap.
//
// The 4 rows are named (from top to bottom): `num`, `top`, `home`, `bottom`.
//
// **Why column-first**: the physical-finger column is the stable anchor —
// "pinky" really means "the column where the pinky rests". Layout-specific
// names like `L_pinky_q` are anti-patterns because they encode a single
// layout's binding into a position name; switching from QWERTY to Dvorak
// would invalidate every such name.
//
// **Serialization order**: Oryx's `keys[]` array is laid out as:
//   indices  0..24  → left half rows 0-3 (4 rows × 6 columns)
//   indices 24..26  → LEFT THUMB cluster (inner, outer)
//   indices 26..50  → right half rows 0-3 (mirrored: inner→outer)
//   indices 50..52  → RIGHT THUMB cluster (inner, outer)
//
// Verified against `examples/voyager-dvorak/pulled/revision.json`:
// idx 24=KC_SPACE (left thumb inner), 25=KC_CAPS (left thumb outer),
// idx 50=KC_ENTER/LALT (right thumb inner), 51=KC_BSPC/MO (right thumb outer).
#[rustfmt::skip]
const POSITION_TABLE: &[(&str, usize)] = &[
    // ── Left half ────────────────────────────────────────────────────────────
    // row 0 (number row)
    ("L_outer_num",    0),
    ("L_pinky_num",    1),
    ("L_ring_num",     2),
    ("L_middle_num",   3),
    ("L_index_num",    4),
    ("L_inner_num",    5),

    // row 1 (top letter row)
    ("L_outer_top",    6),
    ("L_pinky_top",    7),
    ("L_ring_top",     8),
    ("L_middle_top",   9),
    ("L_index_top",    10),
    ("L_inner_top",    11),

    // row 2 (home row)
    ("L_outer_home",   12),
    ("L_pinky_home",   13),
    ("L_ring_home",    14),
    ("L_middle_home",  15),
    ("L_index_home",   16),
    ("L_inner_home",   17),

    // row 3 (bottom row)
    ("L_outer_bottom", 18),
    ("L_pinky_bottom", 19),
    ("L_ring_bottom",  20),
    ("L_middle_bottom", 21),
    ("L_index_bottom", 22),
    ("L_inner_bottom", 23),

    // ── Left thumb cluster ──────────────────────────────────────────────────
    ("L_thumb_inner",  24),
    ("L_thumb_outer",  25),

    // ── Right half (mirrored: inner is leftmost, outer is rightmost) ────────
    // row 0
    ("R_inner_num",    26),
    ("R_index_num",    27),
    ("R_middle_num",   28),
    ("R_ring_num",     29),
    ("R_pinky_num",    30),
    ("R_outer_num",    31),

    // row 1
    ("R_inner_top",    32),
    ("R_index_top",    33),
    ("R_middle_top",   34),
    ("R_ring_top",     35),
    ("R_pinky_top",    36),
    ("R_outer_top",    37),

    // row 2 (home row)
    ("R_inner_home",   38),
    ("R_index_home",   39),
    ("R_middle_home",  40),
    ("R_ring_home",    41),
    ("R_pinky_home",   42),
    ("R_outer_home",   43),

    // row 3
    ("R_inner_bottom", 44),
    ("R_index_bottom", 45),
    ("R_middle_bottom", 46),
    ("R_ring_bottom",  47),
    ("R_pinky_bottom", 48),
    ("R_outer_bottom", 49),

    // ── Right thumb cluster ─────────────────────────────────────────────────
    ("R_thumb_inner",  50),
    ("R_thumb_outer",  51),
];

// =============================================================================
// Electrical matrix → canonical index
// =============================================================================
//
// Verbatim transcription of `keyboards/zsa/voyager/keyboard.json`
// `layouts.LAYOUT.layout[]`: each entry's `matrix: [row, col]` pairs
// with its `label: "k##"` where `##` is the canonical Oryx `keys[]`
// index. The firmware raw HID `KEYDOWN` / `KEYUP` events carry the
// same (row, col) pair that QMK stores in `keyrecord_t.event.key`, so
// this table is the lookup every UI / stats consumer goes through.
//
// The Voyager's scan matrix is 12 rows × 7 cols; only the entries below
// are populated (the rest are matrix holes — ROW2COL diodes do not
// bridge a key there).
#[rustfmt::skip]
const MATRIX_TABLE: &[(u8, u8, usize)] = &[
    // ── Left half rows ──────────────────────────────────────────────
    (0, 1,  0), (0, 2,  1), (0, 3,  2), (0, 4,  3), (0, 5,  4), (0, 6,  5),
    (1, 1,  6), (1, 2,  7), (1, 3,  8), (1, 4,  9), (1, 5, 10), (1, 6, 11),
    (2, 1, 12), (2, 2, 13), (2, 3, 14), (2, 4, 15), (2, 5, 16), (2, 6, 17),
    (3, 1, 18), (3, 2, 19), (3, 3, 20), (3, 4, 21), (3, 5, 22),
    (4, 4, 23),

    // ── Left thumb cluster ──────────────────────────────────────────
    (5, 0, 24), (5, 1, 25),

    // ── Right half rows ─────────────────────────────────────────────
    (6, 0, 26), (6, 1, 27), (6, 2, 28), (6, 3, 29), (6, 4, 30), (6, 5, 31),
    (7, 0, 32), (7, 1, 33), (7, 2, 34), (7, 3, 35), (7, 4, 36), (7, 5, 37),
    (8, 0, 38), (8, 1, 39), (8, 2, 40), (8, 3, 41), (8, 4, 42), (8, 5, 43),
    (10, 2, 44),
    (9, 1, 45), (9, 2, 46), (9, 3, 47), (9, 4, 48), (9, 5, 49),

    // ── Right thumb cluster ─────────────────────────────────────────
    (11, 5, 50), (11, 6, 51),
];

// =============================================================================
// QMK LAYOUT_voyager argument order
// =============================================================================
//
// Maps each QMK LAYOUT positional argument index (0..52) to the
// corresponding Oryx canonical-layout key index. Derived from the
// `keyboard.json` for `keyboards/zsa/voyager` in the firmware24 ZSA
// QMK fork (verified against `layouts.LAYOUT.layout[]`).
//
// Pattern: QMK interleaves left/right per row, ending with both thumbs:
//   QMK   0..6   = L row 0       (canonical 0..6)
//   QMK   6..12  = R row 0       (canonical 26..32)
//   QMK  12..18  = L row 1       (canonical 6..12)
//   QMK  18..24  = R row 1       (canonical 32..38)
//   QMK  24..30  = L row 2       (canonical 12..18)
//   QMK  30..36  = R row 2       (canonical 38..44)
//   QMK  36..42  = L row 3       (canonical 18..24)
//   QMK  42..48  = R row 3       (canonical 44..50)
//   QMK  48..50  = L thumb       (canonical 24..26)
//   QMK  50..52  = R thumb       (canonical 50..52)
//
// Without this permutation, the codegen would emit `keymap.c` with
// every right-side key at the wrong physical position, bricking the
// user's layout the moment they flash.
#[rustfmt::skip]
const QMK_ARG_ORDER: [usize; 52] = [
    // L row 0
    0, 1, 2, 3, 4, 5,
    // R row 0
    26, 27, 28, 29, 30, 31,
    // L row 1
    6, 7, 8, 9, 10, 11,
    // R row 1
    32, 33, 34, 35, 36, 37,
    // L row 2
    12, 13, 14, 15, 16, 17,
    // R row 2
    38, 39, 40, 41, 42, 43,
    // L row 3
    18, 19, 20, 21, 22, 23,
    // R row 3
    44, 45, 46, 47, 48, 49,
    // L thumb
    24, 25,
    // R thumb
    50, 51,
];

// =============================================================================
// ASCII grid
// =============================================================================

#[rustfmt::skip]
const ROW0_L: &[Option<usize>] = &[Some(0), Some(1), Some(2), Some(3), Some(4), Some(5)];
#[rustfmt::skip]
const ROW0_R: &[Option<usize>] = &[Some(26), Some(27), Some(28), Some(29), Some(30), Some(31)];

#[rustfmt::skip]
const ROW1_L: &[Option<usize>] = &[Some(6), Some(7), Some(8), Some(9), Some(10), Some(11)];
#[rustfmt::skip]
const ROW1_R: &[Option<usize>] = &[Some(32), Some(33), Some(34), Some(35), Some(36), Some(37)];

#[rustfmt::skip]
const ROW2_L: &[Option<usize>] = &[Some(12), Some(13), Some(14), Some(15), Some(16), Some(17)];
#[rustfmt::skip]
const ROW2_R: &[Option<usize>] = &[Some(38), Some(39), Some(40), Some(41), Some(42), Some(43)];

#[rustfmt::skip]
const ROW3_L: &[Option<usize>] = &[Some(18), Some(19), Some(20), Some(21), Some(22), Some(23)];
#[rustfmt::skip]
const ROW3_R: &[Option<usize>] = &[Some(44), Some(45), Some(46), Some(47), Some(48), Some(49)];

const ROWS: &[GridRow] = &[
    GridRow {
        left: ROW0_L,
        right: ROW0_R,
    },
    GridRow {
        left: ROW1_L,
        right: ROW1_R,
    },
    GridRow {
        left: ROW2_L,
        right: ROW2_R,
    },
    GridRow {
        left: ROW3_L,
        right: ROW3_R,
    },
];

const LEFT_THUMB: &[ThumbKey] = &[
    ThumbKey {
        index: 24,
        width: ThumbKeyWidth::Standard,
    },
    ThumbKey {
        index: 25,
        width: ThumbKeyWidth::Wide,
    },
];
const RIGHT_THUMB: &[ThumbKey] = &[
    ThumbKey {
        index: 50,
        width: ThumbKeyWidth::Standard,
    },
    ThumbKey {
        index: 51,
        width: ThumbKeyWidth::Wide,
    },
];

const THUMBS: &[ThumbCluster] = &[
    ThumbCluster {
        hand: Hand::Left,
        keys: LEFT_THUMB,
    },
    ThumbCluster {
        hand: Hand::Right,
        keys: RIGHT_THUMB,
    },
];

const GRID: GridLayout = GridLayout {
    halves: 2,
    rows: ROWS,
    thumb_clusters: THUMBS,
};

// =============================================================================
// Physical layout (pixel-accurate GUI)
// =============================================================================
//
// Per-key (x, y) top-left corners are transcribed verbatim from
// `keyboards/zsa/voyager/keyboard.json` `layouts.LAYOUT.layout[]` —
// the same array we used to build `MATRIX_TABLE` above. The JSON does
// **not** include thumb-cluster rotation (it's a cosmetic Oryx/web
// convention, not a matrix fact), so we add it here: each thumb pair
// rotates outward around its inner corner, matching the angle every
// ZSA-authored render of the Voyager uses.
//
// Widths default to 1u; the real thumb caps are physically ~1.25u, but
// using 1u here keeps the rendered halves symmetric against the grid
// and matches the keyboard.json footprint exactly.

const THUMB_ROT_DEG: f32 = 20.0;

#[rustfmt::skip]
const PHYSICAL_KEYS: &[PhysicalKey] = &[
    // ── Left half, rows 0..3 ────────────────────────────────────────
    pk(0,  0.0, 0.50), pk(1,  1.0, 0.50), pk(2,  2.0, 0.25), pk(3,  3.0, 0.00), pk(4,  4.0, 0.25), pk(5,  5.0, 0.50),
    pk(6,  0.0, 1.50), pk(7,  1.0, 1.50), pk(8,  2.0, 1.25), pk(9,  3.0, 1.00), pk(10, 4.0, 1.25), pk(11, 5.0, 1.50),
    pk(12, 0.0, 2.50), pk(13, 1.0, 2.50), pk(14, 2.0, 2.25), pk(15, 3.0, 2.00), pk(16, 4.0, 2.25), pk(17, 5.0, 2.50),
    pk(18, 0.0, 3.50), pk(19, 1.0, 3.50), pk(20, 2.0, 3.25), pk(21, 3.0, 3.00), pk(22, 4.0, 3.25), pk(23, 5.0, 3.50),

    // ── Left thumb cluster (rotated outward around (5, 4.5)) ────────
    pk_rot(24, 5.0, 4.50,  THUMB_ROT_DEG, 5.0, 4.5),
    pk_rot(25, 6.0, 4.75,  THUMB_ROT_DEG, 5.0, 4.5),

    // ── Right half, rows 0..3 ───────────────────────────────────────
    pk(26, 10.0, 0.50), pk(27, 11.0, 0.25), pk(28, 12.0, 0.00), pk(29, 13.0, 0.25), pk(30, 14.0, 0.50), pk(31, 15.0, 0.50),
    pk(32, 10.0, 1.50), pk(33, 11.0, 1.25), pk(34, 12.0, 1.00), pk(35, 13.0, 1.25), pk(36, 14.0, 1.50), pk(37, 15.0, 1.50),
    pk(38, 10.0, 2.50), pk(39, 11.0, 2.25), pk(40, 12.0, 2.00), pk(41, 13.0, 2.25), pk(42, 14.0, 2.50), pk(43, 15.0, 2.50),
    pk(44, 10.0, 3.50), pk(45, 11.0, 3.25), pk(46, 12.0, 3.00), pk(47, 13.0, 3.25), pk(48, 14.0, 3.50), pk(49, 15.0, 3.50),

    // ── Right thumb cluster (rotated outward around (11, 4.5)) ──────
    pk_rot(50,  9.0, 4.75, -THUMB_ROT_DEG, 11.0, 4.5),
    pk_rot(51, 10.0, 4.50, -THUMB_ROT_DEG, 11.0, 4.5),
];

const fn pk(index: usize, x: f32, y: f32) -> PhysicalKey {
    PhysicalKey {
        index,
        x,
        y,
        w: 1.0,
        h: 1.0,
        rot_deg: 0.0,
        rot_origin_x: 0.0,
        rot_origin_y: 0.0,
    }
}

const fn pk_rot(index: usize, x: f32, y: f32, rot_deg: f32, rx: f32, ry: f32) -> PhysicalKey {
    PhysicalKey {
        index,
        x,
        y,
        w: 1.0,
        h: 1.0,
        rot_deg,
        rot_origin_x: rx,
        rot_origin_y: ry,
    }
}

// The bbox is generous by ~1u below the thumbs so the rotated thumb
// caps don't clip the viewport — a rotation of 20° around (5, 4.5)
// pushes the outer corner of (6, 4.75) down to ≈y 5.9.
const PHYSICAL: PhysicalLayout = PhysicalLayout {
    keys: PHYSICAL_KEYS,
    width: 16.0,
    height: 6.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_52_matrix_keys() {
        assert_eq!(Voyager.matrix_key_count(), 52);
    }

    #[test]
    fn every_position_in_range() {
        for (name, idx) in POSITION_TABLE.iter() {
            assert!(
                *idx < Voyager.matrix_key_count(),
                "position {name} has out-of-range index {idx}"
            );
        }
    }

    #[test]
    fn position_name_round_trip() {
        let samples = [
            "L_pinky_home",
            "R_thumb_outer",
            "L_thumb_inner",
            "R_pinky_num",
        ];
        for name in samples {
            let idx = Voyager.position_to_index(name).expect(name);
            let back = Voyager.index_to_position(idx).expect("idx in table");
            assert_eq!(back, name);
        }
    }

    #[test]
    fn hand_classification() {
        // Left half rows
        assert_eq!(Voyager.hand(0), Some(Hand::Left));
        assert_eq!(Voyager.hand(23), Some(Hand::Left));
        // Left thumb cluster
        assert_eq!(Voyager.hand(24), Some(Hand::Left));
        assert_eq!(Voyager.hand(25), Some(Hand::Left));
        // Right half rows
        assert_eq!(Voyager.hand(26), Some(Hand::Right));
        assert_eq!(Voyager.hand(49), Some(Hand::Right));
        // Right thumb cluster
        assert_eq!(Voyager.hand(50), Some(Hand::Right));
        assert_eq!(Voyager.hand(51), Some(Hand::Right));
    }

    #[test]
    fn qmk_arg_order_is_a_complete_permutation() {
        // Every canonical index 0..52 must appear exactly once in the
        // QMK arg order, otherwise the codegen permutation drops or
        // duplicates a key.
        let mut seen = [false; 52];
        for &idx in QMK_ARG_ORDER.iter() {
            assert!(idx < 52, "qmk arg index {idx} out of range");
            assert!(!seen[idx], "canonical index {idx} appears twice");
            seen[idx] = true;
        }
        assert!(
            seen.iter().all(|&b| b),
            "qmk arg order is missing some canonical index"
        );
        assert_eq!(QMK_ARG_ORDER.len(), 52);
        assert_eq!(QMK_ARG_ORDER.len(), Voyager.matrix_key_count());
    }

    #[test]
    fn qmk_arg_order_pins_known_positions() {
        // L row 0 starts the array (canonical 0..6 → QMK 0..6).
        assert_eq!(QMK_ARG_ORDER[0], 0);
        assert_eq!(QMK_ARG_ORDER[5], 5);
        // R row 0 follows immediately (QMK 6..12 = canonical 26..32).
        assert_eq!(QMK_ARG_ORDER[6], 26);
        assert_eq!(QMK_ARG_ORDER[11], 31);
        // L row 1 (QMK 12..18 = canonical 6..12).
        assert_eq!(QMK_ARG_ORDER[12], 6);
        // L thumb (QMK 48..50 = canonical 24..26).
        assert_eq!(QMK_ARG_ORDER[48], 24);
        assert_eq!(QMK_ARG_ORDER[49], 25);
        // R thumb (QMK 50..52 = canonical 50..52, identity).
        assert_eq!(QMK_ARG_ORDER[50], 50);
        assert_eq!(QMK_ARG_ORDER[51], 51);
    }

    #[test]
    fn matrix_table_covers_every_canonical_index() {
        // The firmware HID KEYDOWN path depends on this being a total
        // map: every canonical index 0..52 must be reachable from
        // exactly one (row, col) pair, otherwise a press on the
        // unmapped key silently goes un-highlighted.
        let mut seen = [false; 52];
        for (_, _, idx) in MATRIX_TABLE.iter() {
            assert!(*idx < 52, "matrix entry has out-of-range index {idx}");
            assert!(!seen[*idx], "canonical index {idx} appears twice");
            seen[*idx] = true;
        }
        assert!(
            seen.iter().all(|&b| b),
            "matrix table missing some canonical index"
        );
        assert_eq!(MATRIX_TABLE.len(), Voyager.matrix_key_count());
    }

    #[test]
    fn matrix_to_index_pins_known_coords() {
        // keyboard.json fixture: top-left on left half.
        assert_eq!(Voyager.matrix_to_index(0, 1), Some(0));
        // Thumb cluster — verified against keyboard.json.
        assert_eq!(Voyager.matrix_to_index(5, 0), Some(24));
        assert_eq!(Voyager.matrix_to_index(5, 1), Some(25));
        assert_eq!(Voyager.matrix_to_index(11, 5), Some(50));
        assert_eq!(Voyager.matrix_to_index(11, 6), Some(51));
        // Matrix hole — not wired.
        assert_eq!(Voyager.matrix_to_index(0, 0), None);
        // Out-of-matrix coordinates.
        assert_eq!(Voyager.matrix_to_index(12, 0), None);
        assert_eq!(Voyager.matrix_to_index(0, 7), None);
    }

    /// Pin the position table against the real Oryx fixture so a future
    /// off-by-2 doesn't sneak past the snapshot tests.
    #[test]
    fn matches_oryx_serialization_order_in_fixture() {
        // From examples/voyager-dvorak/pulled/revision.json Main layer:
        // idx 24=KC_SPACE (left thumb), 25=KC_CAPS (left thumb),
        // idx 50=KC_ENTER (right thumb inner — has hold=KC_LALT),
        // idx 51=KC_BSPC (right thumb outer — has hold=MO).
        assert_eq!(Voyager.index_to_position(24), Some("L_thumb_inner"));
        assert_eq!(Voyager.index_to_position(25), Some("L_thumb_outer"));
        assert_eq!(Voyager.index_to_position(26), Some("R_inner_num"));
        assert_eq!(Voyager.index_to_position(31), Some("R_outer_num"));
        assert_eq!(Voyager.index_to_position(50), Some("R_thumb_inner"));
        assert_eq!(Voyager.index_to_position(51), Some("R_thumb_outer"));
    }
}
