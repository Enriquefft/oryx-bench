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

use super::{Geometry, GridLayout, GridRow, Hand, ThumbCluster};

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

    fn ascii_layout(&self) -> &'static GridLayout {
        &GRID
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

const LEFT_THUMB: &[usize] = &[24, 25];
const RIGHT_THUMB: &[usize] = &[50, 51];

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
