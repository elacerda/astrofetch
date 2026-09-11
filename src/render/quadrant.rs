//! Pure 2×2 quadrant renderer (Phase 5B.2).
//!
//! Converts a prepared logical density canvas of `2W × 2H` samples into
//! `W` terminal cells × `H` terminal rows of Unicode quadrant glyphs.
//!
//! Each terminal cell owns four logical subcells in the row-major layout
//!
//! ```text
//! TL TR
//! BL BR
//! ```
//!
//! The subcell→logical mapping is taken from the Phase 4 topology
//! abstraction (`CellSamplingShape::QUADRANT` / `Quadrant`), not from
//! ad-hoc coordinate arithmetic.
//!
//! This checkpoint is intentionally pure:
//! - no ANSI/color behavior (Phase 5B.3 decides the integration boundary),
//! - no background-star injection (Phase 5B.3 / Phase 6),
//! - deterministic from `canvas + threshold` alone (no hidden seed).
//!
//! Visibility uses the same per-subcell rule as the half-block renderer:
//! a subcell is visible iff its value is finite, strictly positive, and
//! greater than or equal to the threshold.

use crate::render::topology::{CellSamplingShape, Quadrant};

/// Quadrant glyph table indexed by the 4-bit visibility mask.
///
/// Bit assignment (exact contract):
/// - bit 0 = TL (top-left)
/// - bit 1 = TR (top-right)
/// - bit 2 = BL (bottom-left)
/// - bit 3 = BR (bottom-right)
///
/// The table is the authoritative mask→glyph mapping; entries are pinned
/// by `test_glyph_table_pins_all_16_masks` (character and code point).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const QUADRANT_GLYPHS: [char; 16] = [
    ' ', // 0b0000 empty
    '▘', // 0b0001 U+2598 QUADRANT UPPER LEFT
    '▝', // 0b0010 U+259D QUADRANT UPPER RIGHT
    '▀', // 0b0011 U+2580 UPPER HALF BLOCK
    '▖', // 0b0100 U+2596 QUADRANT LOWER LEFT
    '▌', // 0b0101 U+258C LEFT HALF BLOCK
    '▞', // 0b0110 U+259E QUADRANT UPPER RIGHT AND LOWER LEFT
    '▛', // 0b0111 U+259B QUADRANT UPPER LEFT AND UPPER RIGHT AND LOWER LEFT
    '▗', // 0b1000 U+2597 QUADRANT LOWER RIGHT
    '▚', // 0b1001 U+259A QUADRANT UPPER LEFT AND LOWER RIGHT
    '▐', // 0b1010 U+2590 RIGHT HALF BLOCK
    '▜', // 0b1011 U+259C QUADRANT UPPER LEFT AND UPPER RIGHT AND LOWER RIGHT
    '▄', // 0b1100 U+2584 LOWER HALF BLOCK
    '▙', // 0b1101 U+2599 QUADRANT UPPER LEFT AND LOWER LEFT AND LOWER RIGHT
    '▟', // 0b1110 U+259F QUADRANT UPPER RIGHT AND LOWER LEFT AND LOWER RIGHT
    '█', // 0b1111 U+2588 FULL BLOCK
];

/// Per-subcell visibility rule, identical to the half-block renderer:
/// visible iff finite, strictly positive, and >= threshold.
fn subcell_visible(value: f64, threshold: f64) -> bool {
    value.is_finite() && value > 0.0 && value >= threshold
}

/// Builds the 4-bit visibility mask for one terminal cell.
///
/// Bit 0 = TL, bit 1 = TR, bit 2 = BL, bit 3 = BR (row-major
/// `Quadrant::ALL` order).
fn visibility_mask(tl: f64, tr: f64, bl: f64, br: f64, threshold: f64) -> u8 {
    let values = [tl, tr, bl, br];
    let mut mask = 0u8;
    for (bit, value) in values.iter().enumerate() {
        if subcell_visible(*value, threshold) {
            mask |= 1 << bit;
        }
    }
    mask
}

/// Renders a `2W × 2H` logical density canvas into `W × H` terminal cells
/// of quadrant glyphs.
///
/// For each terminal cell the four subcells are read through
/// `CellSamplingShape::QUADRANT.logical_index`, each subcell is tested
/// with the shared visibility rule, and the resulting 4-bit mask selects
/// exactly one glyph from `QUADRANT_GLYPHS`.
///
/// Edge behavior (consistent with the existing renderers):
/// - empty canvas → no lines;
/// - missing subcells (odd width/height, ragged rows) read as `0.0`,
///   which is invisible, so partial edge cells simply lose the missing
///   quadrants;
/// - terminal width is `ceil(first_row_len / 2)` and terminal height is
///   `ceil(row_count / 2)`.
///
/// The output is deterministic: the same canvas and threshold always
/// produce identical lines, with no RNG, stars, or ANSI sequences.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn render_quadrant(canvas: &[Vec<f64>], threshold: f64) -> Vec<String> {
    let shape = CellSamplingShape::QUADRANT;
    let terminal_width = canvas.first().map_or(0, Vec::len).div_ceil(shape.columns());
    let terminal_height = canvas.len().div_ceil(shape.rows());
    let mut lines = Vec::with_capacity(terminal_height);

    for cell_y in 0..terminal_height {
        let mut line = String::with_capacity(terminal_width);
        for cell_x in 0..terminal_width {
            let mut values = [0.0f64; 4];
            for (bit, quadrant) in Quadrant::ALL.iter().enumerate() {
                let (sub_x, sub_y) = quadrant.offset();
                let (logical_x, logical_y) = shape.logical_index(cell_x, cell_y, sub_x, sub_y);
                values[bit] = canvas
                    .get(logical_y)
                    .and_then(|row| row.get(logical_x))
                    .copied()
                    .unwrap_or(0.0);
            }
            let mask = visibility_mask(values[0], values[1], values[2], values[3], threshold);
            line.push(QUADRANT_GLYPHS[mask as usize]);
        }
        lines.push(line);
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins all 16 mask entries exactly: character and code point.
    #[test]
    fn test_glyph_table_pins_all_16_masks() {
        // (mask, expected char, expected code point)
        let pinned: [(u8, char, u32); 16] = [
            (0b0000, ' ', 0x0020),
            (0b0001, '▘', 0x2598),
            (0b0010, '▝', 0x259D),
            (0b0011, '▀', 0x2580),
            (0b0100, '▖', 0x2596),
            (0b0101, '▌', 0x258C),
            (0b0110, '▞', 0x259E),
            (0b0111, '▛', 0x259B),
            (0b1000, '▗', 0x2597),
            (0b1001, '▚', 0x259A),
            (0b1010, '▐', 0x2590),
            (0b1011, '▜', 0x259C),
            (0b1100, '▄', 0x2584),
            (0b1101, '▙', 0x2599),
            (0b1110, '▟', 0x259F),
            (0b1111, '█', 0x2588),
        ];

        assert_eq!(QUADRANT_GLYPHS.len(), 16);
        for (mask, expected_char, expected_codepoint) in pinned {
            assert_eq!(
                QUADRANT_GLYPHS[mask as usize], expected_char,
                "mask {mask:04b} glyph mismatch"
            );
            assert_eq!(
                QUADRANT_GLYPHS[mask as usize] as u32, expected_codepoint,
                "mask {mask:04b} code point mismatch"
            );
        }
    }

    /// Proves the exact bit assignment: TL→bit 0, TR→bit 1, BL→bit 2, BR→bit 3.
    #[test]
    fn test_visibility_mask_bit_assignment() {
        let threshold = 0.1;
        let visible = 0.5;

        assert_eq!(
            visibility_mask(visible, 0.0, 0.0, 0.0, threshold),
            0b0001,
            "TL must set bit 0"
        );
        assert_eq!(
            visibility_mask(0.0, visible, 0.0, 0.0, threshold),
            0b0010,
            "TR must set bit 1"
        );
        assert_eq!(
            visibility_mask(0.0, 0.0, visible, 0.0, threshold),
            0b0100,
            "BL must set bit 2"
        );
        assert_eq!(
            visibility_mask(0.0, 0.0, 0.0, visible, threshold),
            0b1000,
            "BR must set bit 3"
        );

        // Combined bits must be independent.
        assert_eq!(
            visibility_mask(visible, visible, 0.0, 0.0, threshold),
            0b0011
        );
        assert_eq!(
            visibility_mask(visible, 0.0, visible, visible, threshold),
            0b1101
        );
        assert_eq!(
            visibility_mask(visible, visible, visible, visible, threshold),
            0b1111
        );
        assert_eq!(visibility_mask(0.0, 0.0, 0.0, 0.0, threshold), 0b0000);
    }

    /// Visibility contract per subcell: below threshold, equal threshold,
    /// zero, negative, NaN, and infinities.
    #[test]
    fn test_visibility_predicate_contract() {
        let threshold = 0.1;

        // Below threshold: invisible.
        assert_eq!(visibility_mask(0.05, 0.05, 0.05, 0.05, threshold), 0b0000);
        // Exactly at threshold: visible.
        assert_eq!(
            visibility_mask(threshold, threshold, threshold, threshold, threshold),
            0b1111
        );
        // Zero: always invisible, even with threshold 0.0.
        assert_eq!(visibility_mask(0.0, 0.0, 0.0, 0.0, 0.0), 0b0000);
        // Negative: always invisible.
        assert_eq!(visibility_mask(-0.5, -0.1, -1.0, -0.01, 0.0), 0b0000);
        // NaN: invisible.
        assert_eq!(
            visibility_mask(f64::NAN, f64::NAN, f64::NAN, f64::NAN, threshold),
            0b0000
        );
        // Infinities: invisible.
        assert_eq!(
            visibility_mask(
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                threshold
            ),
            0b0000
        );
        // Non-finite subcells must not affect the other bits.
        assert_eq!(
            visibility_mask(f64::NAN, 0.5, f64::INFINITY, 0.5, threshold),
            0b1010
        );
    }

    /// A 4×4 canvas (2×2 terminal cells) with known masks.
    ///
    /// Terminal cell values (threshold 0.1; 1.0 = visible, 0.0 = invisible):
    /// ```text
    /// cell (0,0): TL=1 TR=1 BL=0 BR=0  -> 0b0011 '▀'
    /// cell (1,0): TL=1 TR=0 BL=1 BR=0  -> 0b0101 '▌'
    /// cell (0,1): TL=0 TR=1 BL=1 BR=1  -> 0b1110 '▟'
    /// cell (1,1): TL=1 TR=1 BL=1 BR=1  -> 0b1111 '█'
    /// ```
    #[test]
    fn test_synthetic_cells_exact_glyphs() {
        let canvas = vec![
            vec![1.0, 1.0, 1.0, 0.0], // y=0: TL/TR of row-0 cells
            vec![0.0, 0.0, 1.0, 0.0], // y=1: BL/BR of row-0 cells
            vec![0.0, 1.0, 1.0, 1.0], // y=2: TL/TR of row-1 cells
            vec![1.0, 1.0, 1.0, 1.0], // y=3: BL/BR of row-1 cells
        ];

        let result = render_quadrant(&canvas, 0.1);
        assert_eq!(result, vec!["▀▌", "▟█"]);
    }

    /// Full 16-mask coverage: an 8×8 canvas (4×4 terminal cells) where each
    /// terminal cell realizes exactly one mask, in row-major mask order.
    #[test]
    fn test_all_16_masks_rendered() {
        let visible = 1.0;
        let hidden = 0.0;
        let threshold = 0.1;

        // Build one 2×2 block per mask, row-major mask order 0..16.
        let mut canvas: Vec<Vec<f64>> = Vec::new();
        for mask_row in 0..4 {
            for y in 0..2 {
                let mut row = Vec::new();
                for mask_col in 0..4 {
                    let mask = (mask_row * 4 + mask_col) as u8;
                    for x in 0..2 {
                        let bit = if y == 0 { x } else { x + 2 };
                        let value = if mask & (1 << bit) != 0 {
                            visible
                        } else {
                            hidden
                        };
                        row.push(value);
                    }
                }
                canvas.push(row);
            }
        }

        let result = render_quadrant(&canvas, threshold);
        assert_eq!(result.len(), 4);
        for (row, line) in result.iter().enumerate() {
            let expected: String = (0..4).map(|col| QUADRANT_GLYPHS[row * 4 + col]).collect();
            assert_eq!(line, &expected, "mask row {row} mismatch");
        }
    }

    /// A valid 2W×2H canvas produces exactly H lines of exactly W chars.
    #[test]
    fn test_output_dimensions() {
        let (w, h) = (5usize, 3usize);
        let canvas: Vec<Vec<f64>> = (0..2 * h).map(|_| vec![0.5; 2 * w]).collect();

        let result = render_quadrant(&canvas, 0.1);
        assert_eq!(result.len(), h);
        for line in &result {
            assert_eq!(line.chars().count(), w);
        }
    }

    /// Same canvas + threshold → identical output bytes. No RNG involved.
    #[test]
    fn test_deterministic_output() {
        let canvas = vec![
            vec![0.0, 0.5, 0.9, 0.1],
            vec![0.5, 0.0, 0.1, 0.9],
            vec![0.9, 0.1, 0.0, 0.5],
            vec![0.1, 0.9, 0.5, 0.0],
        ];

        let result1 = render_quadrant(&canvas, 0.2);
        let result2 = render_quadrant(&canvas, 0.2);
        assert_eq!(result1, result2);
        assert_eq!(result1.join("\n").as_bytes(), result2.join("\n").as_bytes());
    }

    /// Empty canvas → no lines (consistent with the other renderers).
    #[test]
    fn test_empty_canvas() {
        let canvas: Vec<Vec<f64>> = vec![];
        assert_eq!(render_quadrant(&canvas, 0.1), Vec::<String>::new());
    }

    /// All-zero canvas renders as spaces, never as glyphs.
    #[test]
    fn test_zero_canvas_is_spaces() {
        // 4 logical columns -> 2 terminal cells.
        let canvas = vec![vec![0.0, 0.0, 0.0, 0.0], vec![0.0, 0.0, 0.0, 0.0]];
        let result = render_quadrant(&canvas, 0.0);
        assert_eq!(result, vec!["  "]);
    }

    /// Odd height: the last terminal row's missing bottom subcells read as
    /// 0.0 (invisible). 3 rows → 2 terminal rows.
    #[test]
    fn test_odd_height_missing_bottom_is_invisible() {
        // Rows 0-1 form a full cell row; row 2 has no partner row.
        let canvas = vec![vec![1.0, 1.0], vec![1.0, 1.0], vec![1.0, 1.0]];
        let result = render_quadrant(&canvas, 0.1);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "█");
        // Row 2: TL/TR visible, BL/BR missing → 0b0011 '▀'
        assert_eq!(result[1], "▀");
    }

    /// Odd width: the last terminal column's missing right subcells read as
    /// 0.0 (invisible). 5 columns → 3 terminal columns.
    #[test]
    fn test_odd_width_missing_right_is_invisible() {
        let canvas = vec![vec![1.0, 1.0, 1.0, 1.0, 1.0], vec![1.0, 1.0, 1.0, 1.0, 1.0]];
        let result = render_quadrant(&canvas, 0.1);
        assert_eq!(result.len(), 1);
        // Cells 0-1 full ('█'), cell 2 has only the left subcells → 0b0101 '▌'
        assert_eq!(result[0], "██▌");
        assert_eq!(result[0].chars().count(), 3);
    }

    /// Ragged rows: missing subcells read as 0.0 (invisible).
    #[test]
    fn test_ragged_rows() {
        // Second row shorter: BR of the only cell missing.
        // Mask 0b0111 → '▛'
        let canvas = vec![vec![1.0, 1.0], vec![1.0]];
        let result = render_quadrant(&canvas, 0.1);
        assert_eq!(result, vec!["▛"]);

        // First row shorter: TR of the only cell missing.
        // Mask 0b1101 → '▙'
        let canvas = vec![vec![1.0], vec![1.0, 1.0]];
        let result = render_quadrant(&canvas, 0.1);
        assert_eq!(result, vec!["▙"]);
    }

    /// Output is pure glyphs: no ANSI escape sequences.
    #[test]
    fn test_output_is_ansi_free() {
        let canvas = vec![vec![0.5, 0.0], vec![0.0, 0.5]];
        let result = render_quadrant(&canvas, 0.1);
        for line in &result {
            assert!(!line.contains('\x1b'));
        }
    }
}
