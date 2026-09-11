//! 2×2 quadrant renderer (Phases 5B.2-6B).
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
//! This module provides three renderers:
//! - `render_quadrant` (Phase 5B.2): pure no-color primitive — glyphs
//!   only, no ANSI, no stars;
//! - `render_quadrant_colored` (Phase 6A): pure colored primitive — the
//!   same geometry with a foreground-only color channel, no stars;
//! - `render_quadrant_with_stars` (Phase 6B): the application-level
//!   star-aware renderer used by the `--renderer quadrant` path.
//!
//! The pure primitives intentionally remain star-free reference and
//! compatibility functions. The star-aware renderer adds deterministic
//! sparse background stars only to completely empty cells; stars are
//! uncolored, and foreground galaxy intensity keeps the Phase 6A
//! max-visible-subcell rule. All renderers are deterministic from their
//! inputs (no hidden seed).
//!
//! Visibility uses the same per-subcell rule as the half-block renderer:
//! a subcell is visible iff its value is finite, strictly positive, and
//! greater than or equal to the threshold.

use crate::render::ansi::AnsiForegroundLine;
use crate::render::color::{galaxy_foreground_ansi, ColorPalette};
use crate::render::topology::{CellSamplingShape, Quadrant};
use crate::render::{star_field_seed, star_glyph_for_local_density};

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
///
/// Phase 6B note: the application-level Quadrant path now routes through
/// `render_quadrant_with_stars`. This pure primitive is retained as the
/// frozen Phase-5B reference (star-free) and is exercised by tests only.
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

/// Maximum density among the VISIBLE subcells of one terminal cell.
///
/// Visibility uses the exact per-subcell rule of `subcell_visible`
/// (finite && value > 0 && value >= threshold), so non-finite values
/// (+inf, -inf, NaN) and below-threshold values are invisible by contract
/// and never influence the selected foreground color.
///
/// Returns `None` when no subcell is visible (empty mask).
fn max_visible_density(values: [f64; 4], threshold: f64) -> Option<f64> {
    values
        .iter()
        .filter(|&&value| subcell_visible(value, threshold))
        .copied()
        .max_by(f64::total_cmp)
}

/// Renders a `2W × 2H` logical density canvas into `W × H` terminal cells
/// of quadrant glyphs with a foreground-only color channel (Phase 6A).
///
/// Geometry is identical to `render_quadrant`: each cell's glyph is chosen
/// exclusively from the 4-bit visibility mask via `QUADRANT_GLYPHS`.
/// Color is an additional semantic channel:
/// - empty mask (0b0000) → plain space, no foreground style;
/// - non-empty mask → the glyph is styled with
///   `galaxy_foreground_ansi(palette, max_visible_density)`, where
///   `max_visible_density` is the maximum density among VISIBLE subcells
///   only (finite && > 0 && >= threshold).
///
/// No ANSI background-color sequence is ever emitted: a background would
/// paint subcells whose mask bit is zero and falsify the spatial geometry.
///
/// Edge behavior (empty canvas, missing subcells, terminal dimensions)
/// matches `render_quadrant`. The output is deterministic: the same
/// canvas, threshold, and palette always produce identical lines.
///
/// Phase 6B note: the application-level Quadrant path now routes through
/// `render_quadrant_with_stars`. This pure primitive is retained as the
/// frozen Phase-6A reference (star-free) and is exercised by tests only.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn render_quadrant_colored(
    canvas: &[Vec<f64>],
    threshold: f64,
    palette: ColorPalette,
) -> Vec<String> {
    let shape = CellSamplingShape::QUADRANT;
    let terminal_width = canvas.first().map_or(0, Vec::len).div_ceil(shape.columns());
    let terminal_height = canvas.len().div_ceil(shape.rows());
    let mut lines = Vec::with_capacity(terminal_height);

    for cell_y in 0..terminal_height {
        let mut line = AnsiForegroundLine::with_capacity(terminal_width * 8);
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
            let glyph = QUADRANT_GLYPHS[mask as usize];
            match max_visible_density(values, threshold) {
                Some(max_visible) => {
                    line.push_styled(glyph, galaxy_foreground_ansi(palette, max_visible));
                }
                None => line.push_plain(glyph),
            }
        }
        lines.push(line.finish());
    }

    lines
}

/// Renders a `2W × 2H` logical density canvas into `W × H` terminal cells of
/// quadrant glyphs with deterministic sparse background stars (Phase 6B).
///
/// Geometry is identical to `render_quadrant` / `render_quadrant_colored`:
/// each terminal cell's glyph is chosen exclusively from the 4-bit visibility
/// mask via `QUADRANT_GLYPHS`. The only addition is a background-star overlay
/// on visually-empty cells (mask == 0), using the exact same deterministic
/// star decision as the existing galaxy renderers
/// (`star_glyph_for_local_density`). No separate Quadrant-specific probability
/// or glyph distribution is introduced.
///
/// For each terminal cell:
/// - mask != 0: render exactly `QUADRANT_GLYPHS[mask]`. When `colors_enabled`,
///   the glyph is styled with `galaxy_foreground_ansi(palette, max_visible)`
///   where `max_visible` is the maximum density among VISIBLE subcells only
///   (the exact Phase-6A semantic). When colors are disabled, the glyph is
///   emitted plainly. A star is NEVER rendered for a non-empty cell.
/// - mask == 0: compute one local density as the direct four-subcell
///   generalization of the two-half star suppression calculation,
///   `tl.max(tr).max(bl).max(br)`, and query
///   `star_glyph_for_local_density(cell_x, cell_y, local_density, threshold,
///   star_seed)`. `Some(star)` is emitted plainly (uncolored); `None` emits a
///   plain space.
///
/// Stars remain uncolored even when galaxy color is enabled, and no ANSI
/// background-color sequence is ever emitted. The star seed is computed once
/// per render via `star_field_seed(canvas)`.
///
/// The output is deterministic: the same canvas, threshold, `colors_enabled`,
/// and palette always produce identical lines (no RNG).
pub(crate) fn render_quadrant_with_stars(
    canvas: &[Vec<f64>],
    threshold: f64,
    colors_enabled: bool,
    palette: ColorPalette,
) -> Vec<String> {
    let shape = CellSamplingShape::QUADRANT;
    let terminal_width = canvas.first().map_or(0, Vec::len).div_ceil(shape.columns());
    let terminal_height = canvas.len().div_ceil(shape.rows());
    let star_seed = star_field_seed(canvas);
    let mut lines = Vec::with_capacity(terminal_height);

    for cell_y in 0..terminal_height {
        let mut line = AnsiForegroundLine::with_capacity(terminal_width * 8);
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
            if mask != 0 {
                let glyph = QUADRANT_GLYPHS[mask as usize];
                if colors_enabled {
                    // Phase-6A semantic: color comes from the max of the
                    // VISIBLE subcells only. mask != 0 guarantees at least one
                    // visible subcell, so this is always Some in practice.
                    match max_visible_density(values, threshold) {
                        Some(max_visible) => {
                            line.push_styled(glyph, galaxy_foreground_ansi(palette, max_visible));
                        }
                        None => line.push_plain(glyph),
                    }
                } else {
                    line.push_plain(glyph);
                }
            } else {
                // Visually empty cell: the four-subcell generalization of the
                // two-half star suppression local density (f64::max semantics).
                let local_density = values[0].max(values[1]).max(values[2]).max(values[3]);
                match star_glyph_for_local_density(
                    cell_x,
                    cell_y,
                    local_density,
                    threshold,
                    star_seed,
                ) {
                    Some(star) => line.push_plain(star),
                    None => line.push_plain(' '),
                }
            }
        }
        lines.push(line.finish());
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

    // ===== Phase 6A: colored renderer tests =====

    /// Exact colored single-cell output: glyph, exact foreground sequence,
    /// and exactly one trailing reset.
    #[test]
    fn test_colored_single_cell_exact_output() {
        // All four subcells visible; max visible density is 0.6 (level 4).
        let canvas = vec![vec![0.5, 0.6], vec![0.2, 0.4]];
        let result = render_quadrant_colored(&canvas, 0.1, ColorPalette::Nebula);
        assert_eq!(result, vec!["\x1b[38;5;136m█\x1b[0m"]);
    }

    /// Partial glyph: color comes from the max of the visible subcells only.
    #[test]
    fn test_colored_partial_glyph_uses_max_visible_subcell() {
        // Mask 0b1001 '▚' (TL + BR); TL=0.5 (level 3), BR=0.6 (level 4).
        // The color must be the max of the two visible subcells (BR).
        let canvas = vec![vec![0.5, 0.0], vec![0.0, 0.6]];
        let result = render_quadrant_colored(&canvas, 0.1, ColorPalette::Nebula);
        assert_eq!(result, vec!["\x1b[38;5;136m▚\x1b[0m"]);
    }

    /// Non-finite values are invisible by contract and must never influence
    /// the selected foreground color.
    #[test]
    fn test_colored_non_finite_values_do_not_influence_color() {
        // TL=0.5 visible (level 3); TR=+inf and BL=NaN are invisible.
        // Mask 0b0001 '▘'; color must come from TL alone. If +inf leaked
        // into the max, the level would be 6 ("\x1b[38;5;255m").
        let canvas = vec![vec![0.5, f64::INFINITY], vec![f64::NAN, 0.0]];
        let result = render_quadrant_colored(&canvas, 0.1, ColorPalette::Nebula);
        assert_eq!(result, vec!["\x1b[38;5;65m▘\x1b[0m"]);
    }

    /// Adjacent cells with the same style are grouped: one style prefix and
    /// one final reset for the whole run.
    #[test]
    fn test_colored_same_style_cells_grouped() {
        // Two full cells, both with max visible density 0.5 (level 3).
        let canvas = vec![vec![0.5, 0.5, 0.5, 0.5], vec![0.5, 0.5, 0.5, 0.5]];
        let result = render_quadrant_colored(&canvas, 0.1, ColorPalette::Nebula);
        assert_eq!(result, vec!["\x1b[38;5;65m██\x1b[0m"]);
        assert_eq!(result[0].matches("\x1b[38;5;65m").count(), 1);
        assert_eq!(result[0].matches("\x1b[0m").count(), 1);
    }

    /// A style transition emits RESET between the two styles.
    #[test]
    fn test_colored_style_transition_resets() {
        // Cell 0 max 0.5 (level 3); cell 1 max 0.9 (level 6).
        let canvas = vec![vec![0.5, 0.5, 0.9, 0.9], vec![0.5, 0.5, 0.9, 0.9]];
        let result = render_quadrant_colored(&canvas, 0.1, ColorPalette::Nebula);
        assert_eq!(result, vec!["\x1b[38;5;65m█\x1b[0m\x1b[38;5;255m█\x1b[0m"]);
    }

    /// Styled → plain transition: RESET before the plain glyph, and no
    /// trailing reset because the line ends plain.
    #[test]
    fn test_colored_styled_to_plain_transition() {
        // Cell 0 full (level 3); cell 1 empty (plain space).
        let canvas = vec![vec![0.5, 0.5, 0.0, 0.0], vec![0.5, 0.5, 0.0, 0.0]];
        let result = render_quadrant_colored(&canvas, 0.1, ColorPalette::Nebula);
        assert_eq!(result, vec!["\x1b[38;5;65m█\x1b[0m "]);
    }

    /// Dim → non-dim transition must not leak the dim attribute: a RESET is
    /// emitted before the non-dim style.
    #[test]
    fn test_colored_dim_to_non_dim_transition() {
        // Cell 0 max 0.1 (level 0, dim); cell 1 max 0.5 (level 3, non-dim).
        let canvas = vec![vec![0.1, 0.1, 0.5, 0.5], vec![0.1, 0.1, 0.5, 0.5]];
        let result = render_quadrant_colored(&canvas, 0.05, ColorPalette::Nebula);
        assert_eq!(result, vec!["\x1b[2;38;5;17m█\x1b[0m\x1b[38;5;65m█\x1b[0m"]);
    }

    /// Line-end reset: exactly one trailing reset when the line ends
    /// styled; no reset at all when the line ends plain.
    #[test]
    fn test_colored_line_end_reset_behavior() {
        let styled_canvas = vec![vec![0.5, 0.5], vec![0.5, 0.5]];
        let styled = render_quadrant_colored(&styled_canvas, 0.1, ColorPalette::Nebula);
        assert_eq!(styled, vec!["\x1b[38;5;65m█\x1b[0m"]);
        assert_eq!(styled[0].matches("\x1b[0m").count(), 1);

        let plain_canvas = vec![vec![0.0, 0.0], vec![0.0, 0.0]];
        let plain = render_quadrant_colored(&plain_canvas, 0.1, ColorPalette::Nebula);
        assert_eq!(plain, vec![" "]);
        assert!(!plain[0].contains('\x1b'));
    }

    /// Colored Quadrant output never contains a background-color escape.
    #[test]
    fn test_colored_output_has_no_background_escape() {
        let canvas = vec![vec![0.5, 0.9, 0.1, 0.3], vec![0.7, 0.2, 0.9, 0.0]];
        let result = render_quadrant_colored(&canvas, 0.1, ColorPalette::Nebula);
        for line in &result {
            assert!(!line.contains("48;5;"), "background escape found: {line:?}");
        }
    }

    /// Repeated colored rendering is byte-deterministic.
    #[test]
    fn test_colored_deterministic_output() {
        let canvas = vec![vec![0.0, 0.5, 0.9, 0.1], vec![0.5, 0.0, 0.1, 0.9]];
        let first = render_quadrant_colored(&canvas, 0.2, ColorPalette::Nebula);
        let second = render_quadrant_colored(&canvas, 0.2, ColorPalette::Nebula);
        assert_eq!(first, second);
        assert_eq!(first.join("\n").as_bytes(), second.join("\n").as_bytes());
    }

    /// Test-local helper: removes ANSI SGR escape sequences (`ESC [ ... m`)
    /// from a line so colored output can be compared with pure glyph output.
    fn strip_ansi_sgr(line: &str) -> String {
        let mut out = String::with_capacity(line.len());
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    /// Geometry identity (Phase 6A invariant): stripping the ANSI color
    /// channel from `render_quadrant_colored` output must reproduce the
    /// pure `render_quadrant` output exactly, for the same canvas and
    /// threshold. Color adds intensity information only; it never changes
    /// the glyph geometry.
    #[test]
    fn test_colored_geometry_identity_matches_pure_renderer() {
        // 4 cells x 2 rows (8 x 4 logical canvas) exercising 8 distinct
        // visibility masks, including partial cells and one empty cell,
        // with densities spanning several intensity levels.
        let canvas = vec![
            vec![0.5, 0.0, 0.2, 0.0, 0.0, 0.0, 0.9, 0.4],
            vec![0.0, 0.0, 0.0, 0.6, 0.35, 0.31, 0.7, 0.8],
            vec![0.0, 0.45, 0.0, 0.0, 0.55, 0.5, 0.25, 0.3],
            vec![0.65, 0.0, 0.0, 0.0, 0.0, 0.95, 0.0, 0.0],
        ];
        let threshold = 0.1;

        let pure = render_quadrant(&canvas, threshold);
        let colored = render_quadrant_colored(&canvas, threshold, ColorPalette::Nebula);

        // Pin the expected geometry explicitly: masks
        // 0b0001 0b1001 0b1100 0b1111 / 0b0110 0b0000 0b1011 0b0011.
        assert_eq!(pure, vec!["▘▚▄█", "▞ ▜▀"]);

        // The colored output must actually carry a color channel, so the
        // identity below is not vacuous.
        assert!(colored.join("\n").contains('\x1b'));

        // Stripping the ANSI SGR sequences must leave exactly the pure output.
        let stripped: Vec<String> = colored.iter().map(|line| strip_ansi_sgr(line)).collect();
        assert_eq!(stripped, pure);
    }

    // ===== Phase 6B: star-aware renderer tests =====

    /// Test-local helper: parses a rendered line and returns, for each visible
    /// character, whether it was emitted in a styled (ANSI foreground active)
    /// or plain context. This lets the tests verify star color behavior (stars
    /// must always be plain) and style transitions without depending on the
    /// exact color values.
    fn char_styles(line: &str) -> Vec<(char, bool)> {
        let chars: Vec<char> = line.chars().collect();
        let mut result = Vec::new();
        let mut styled = false;
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            if ch == '\x1b' && i + 1 < chars.len() && chars[i + 1] == '[' {
                i += 2; // consume ESC [
                let mut seq = String::new();
                while i < chars.len() {
                    seq.push(chars[i]);
                    i += 1;
                    if seq.ends_with('m') {
                        break;
                    }
                }
                styled = seq != "0m";
            } else {
                result.push((ch, styled));
                i += 1;
            }
        }
        result
    }

    /// Repeated star-aware renders are byte-identical, in both color and
    /// no-color modes (no RNG).
    #[test]
    fn test_with_stars_deterministic_output() {
        let canvas = vec![
            vec![0.0, 0.5, 0.9, 0.1, 0.0, 0.0, 0.0, 0.0],
            vec![0.5, 0.0, 0.1, 0.9, 0.0, 0.0, 0.0, 0.0],
            vec![0.9, 0.1, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0],
            vec![0.1, 0.9, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0],
        ];
        let no1 = render_quadrant_with_stars(&canvas, 0.2, false, ColorPalette::Nebula);
        let no2 = render_quadrant_with_stars(&canvas, 0.2, false, ColorPalette::Nebula);
        assert_eq!(no1, no2);
        assert_eq!(no1.join("\n").as_bytes(), no2.join("\n").as_bytes());

        let col1 = render_quadrant_with_stars(&canvas, 0.2, true, ColorPalette::Nebula);
        let col2 = render_quadrant_with_stars(&canvas, 0.2, true, ColorPalette::Nebula);
        assert_eq!(col1, col2);
        assert_eq!(col1.join("\n").as_bytes(), col2.join("\n").as_bytes());
    }

    /// A non-empty cell (mask != 0) always renders its exact quadrant glyph and
    /// is never replaced by a star; an empty cell (mask == 0) renders a star or
    /// a space, never a quadrant glyph.
    #[test]
    fn test_with_stars_never_replaces_nonempty_glyph() {
        let visible = 1.0;
        let hidden = 0.0;
        let threshold = 0.1;
        // One 2x2 block per mask, row-major mask order 0..16 (4x4 cells).
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
        let result = render_quadrant_with_stars(&canvas, threshold, false, ColorPalette::Nebula);
        assert_eq!(result.len(), 4);
        for (row, line) in result.iter().enumerate() {
            assert_eq!(line.chars().count(), 4);
            for col in 0..4 {
                let mask = (row * 4 + col) as u8;
                let ch = line.chars().nth(col).unwrap();
                if mask != 0 {
                    assert_eq!(
                        ch, QUADRANT_GLYPHS[mask as usize],
                        "non-empty cell ({col},{row}) mask {mask:04b} must render its glyph"
                    );
                    assert!(
                        !matches!(ch, '.' | '*' | '+'),
                        "non-empty cell ({col},{row}) must not render a star"
                    );
                } else {
                    assert!(
                        matches!(ch, ' ' | '.' | '*' | '+'),
                        "empty cell ({col},{row}) must render a star or space, got {ch:?}"
                    );
                }
            }
        }
    }

    /// Star-aware empty cells agree exactly with `star_glyph_for_local_density`
    /// using the same x, y, local_density (0.0 for an empty cell), threshold,
    /// and star_seed.
    #[test]
    fn test_with_stars_empty_cells_match_helper() {
        let (w, h) = (12usize, 8usize);
        let canvas: Vec<Vec<f64>> = (0..2 * h).map(|_| vec![0.0f64; 2 * w]).collect();
        let threshold = 0.5;
        let seed = star_field_seed(&canvas);
        let result = render_quadrant_with_stars(&canvas, threshold, false, ColorPalette::Nebula);
        assert_eq!(result.len(), h);
        for (cy, line) in result.iter().enumerate() {
            for (cx, ch) in line.chars().enumerate() {
                let expected = star_glyph_for_local_density(cx, cy, 0.0, threshold, seed);
                match expected {
                    Some(star) => assert_eq!(ch, star, "cell ({cx},{cy})"),
                    None => assert_eq!(ch, ' ', "cell ({cx},{cy})"),
                }
            }
        }
    }

    /// Star presence is deterministic: every star the renderer emits is at a
    /// coordinate the helper also selects, and at least one star exists.
    #[test]
    fn test_with_stars_star_presence_deterministic() {
        let (w, h) = (40usize, 20usize);
        let canvas: Vec<Vec<f64>> = (0..2 * h).map(|_| vec![0.0f64; 2 * w]).collect();
        let threshold = 0.5;
        let seed = star_field_seed(&canvas);
        let result = render_quadrant_with_stars(&canvas, threshold, false, ColorPalette::Nebula);
        let mut star_count = 0;
        for (cy, line) in result.iter().enumerate() {
            for (cx, ch) in line.chars().enumerate() {
                if matches!(ch, '.' | '*' | '+') {
                    star_count += 1;
                    assert_eq!(
                        star_glyph_for_local_density(cx, cy, 0.0, threshold, seed),
                        Some(ch),
                        "star at ({cx},{cy}) not confirmed by helper"
                    );
                }
            }
        }
        assert!(star_count > 0, "expected at least one deterministic star");
    }

    /// A visually-empty cell whose local density exceeds threshold * 0.35 is
    /// suppressed to a space, even at a coordinate that would otherwise emit a
    /// star. The coordinate is found deterministically by scanning.
    #[test]
    fn test_with_stars_faint_density_suppression() {
        let threshold = 0.5;
        let faint = 0.3; // invisible (< threshold) but > threshold * 0.35
        let (w, h) = (20usize, 14usize);

        for cy in 0..h {
            for cx in 0..w {
                let mut canvas = vec![vec![0.0f64; 2 * w]; 2 * h];
                canvas[2 * cy][2 * cx] = faint;
                canvas[2 * cy][2 * cx + 1] = faint;
                canvas[2 * cy + 1][2 * cx] = faint;
                canvas[2 * cy + 1][2 * cx + 1] = faint;
                let seed = star_field_seed(&canvas);
                // Would this coordinate emit a star if it were empty?
                if star_glyph_for_local_density(cx, cy, 0.0, threshold, seed).is_some() {
                    let result =
                        render_quadrant_with_stars(&canvas, threshold, false, ColorPalette::Nebula);
                    let ch = result[cy].chars().nth(cx).unwrap();
                    assert_eq!(
                        ch, ' ',
                        "faint-density cell ({cx},{cy}) must be suppressed to a space"
                    );
                    assert_eq!(
                        star_glyph_for_local_density(cx, cy, faint, threshold, seed),
                        None,
                        "faint local density must suppress the star"
                    );
                    return;
                }
            }
        }
        panic!("expected to find a suppressible star coordinate");
    }

    /// Color behavior: stars remain plain/uncolored, styled galaxy cells are
    /// styled, both transition directions are correct, and no background ANSI
    /// is emitted.
    #[test]
    fn test_with_stars_color_behavior() {
        let (w, h) = (40usize, 20usize);
        let mut canvas = vec![vec![0.0f64; 2 * w]; 2 * h];
        // Checkerboard of full galaxy cells (density 0.5) and empty cells, so
        // stars (in empty cells) are always adjacent to styled galaxy cells.
        for cy in 0..h {
            for cx in 0..w {
                if (cx + cy) % 2 == 0 {
                    canvas[2 * cy][2 * cx] = 0.5;
                    canvas[2 * cy][2 * cx + 1] = 0.5;
                    canvas[2 * cy + 1][2 * cx] = 0.5;
                    canvas[2 * cy + 1][2 * cx + 1] = 0.5;
                }
            }
        }
        let threshold = 0.1;
        let result = render_quadrant_with_stars(&canvas, threshold, true, ColorPalette::Nebula);
        assert_eq!(result.len(), h);

        // No background ANSI.
        for line in &result {
            assert!(!line.contains("48;5;"), "background escape found: {line:?}");
        }

        let mut star_count = 0;
        let mut galaxy_count = 0;
        for (cy, line) in result.iter().enumerate() {
            let styles = char_styles(line);
            assert_eq!(styles.len(), w, "line {cy} char count mismatch");
            for (cx, (ch, is_styled)) in styles.iter().enumerate() {
                if matches!(ch, '.' | '*' | '+') {
                    star_count += 1;
                    assert!(!is_styled, "star at ({cx},{cy}) must be plain/uncolored");
                } else if *ch != ' ' {
                    galaxy_count += 1;
                    assert!(*is_styled, "galaxy cell at ({cx},{cy}) must be styled");
                }
            }
        }
        assert!(star_count > 0, "expected at least one star");
        assert!(galaxy_count > 0, "expected at least one styled galaxy cell");

        // Verify both transition directions occur and are correct.
        let mut galaxy_to_star = false;
        let mut star_to_galaxy = false;
        for line in &result {
            let styles = char_styles(line);
            for i in 0..styles.len().saturating_sub(1) {
                let (ch1, s1) = (styles[i].0, styles[i].1);
                let (ch2, s2) = (styles[i + 1].0, styles[i + 1].1);
                let star1 = matches!(ch1, '.' | '*' | '+');
                let star2 = matches!(ch2, '.' | '*' | '+');
                let gal1 = ch1 != ' ' && !star1;
                let gal2 = ch2 != ' ' && !star2;
                if gal1 && s1 && star2 && !s2 {
                    galaxy_to_star = true;
                }
                if star1 && !s1 && gal2 && s2 {
                    star_to_galaxy = true;
                }
            }
        }
        assert!(
            galaxy_to_star,
            "expected a styled-galaxy -> star transition"
        );
        assert!(
            star_to_galaxy,
            "expected a star -> styled-galaxy transition"
        );
    }

    /// Geometry/color identity: stripping the ANSI color channel from the
    /// colored star-aware output must reproduce the no-color star-aware output
    /// exactly (color adds intensity only; it never changes the glyph or the
    /// star overlay).
    #[test]
    fn test_with_stars_geometry_identity() {
        let (w, h) = (40usize, 20usize);
        let mut canvas = vec![vec![0.0f64; 2 * w]; 2 * h];
        for cy in 0..h {
            for cx in 0..w {
                if (cx * 3 + cy * 5) % 4 == 0 {
                    canvas[2 * cy][2 * cx] = 0.9;
                    canvas[2 * cy][2 * cx + 1] = 0.7;
                    canvas[2 * cy + 1][2 * cx] = 0.5;
                    canvas[2 * cy + 1][2 * cx + 1] = 0.3;
                }
            }
        }
        let threshold = 0.1;
        let no_color = render_quadrant_with_stars(&canvas, threshold, false, ColorPalette::Nebula);
        let colored = render_quadrant_with_stars(&canvas, threshold, true, ColorPalette::Nebula);
        assert!(
            colored.join("\n").contains('\x1b'),
            "colored output must carry ANSI"
        );
        let stripped: Vec<String> = colored.iter().map(|line| strip_ansi_sgr(line)).collect();
        assert_eq!(stripped, no_color);
    }

    /// Pure primitive isolation: the frozen Phase-5B/6A reference renderers
    /// remain star-free. An empty canvas produces spaces only, even where the
    /// star-aware renderer would emit stars.
    #[test]
    fn test_pure_primitives_remain_star_free() {
        let (w, h) = (40usize, 20usize);
        let canvas: Vec<Vec<f64>> = (0..2 * h).map(|_| vec![0.0f64; 2 * w]).collect();
        let pure = render_quadrant(&canvas, 0.5);
        assert_eq!(pure.len(), h);
        for line in &pure {
            assert_eq!(line.chars().count(), w);
            assert!(
                line.chars().all(|c| c == ' '),
                "pure renderer must be star-free: {line:?}"
            );
        }
        let pure_colored = render_quadrant_colored(&canvas, 0.5, ColorPalette::Nebula);
        for line in &pure_colored {
            assert!(
                line.chars().all(|c| c == ' '),
                "pure colored renderer must be star-free: {line:?}"
            );
        }
    }
}
