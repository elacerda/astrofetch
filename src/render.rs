mod ascii;
mod color;
mod hash;
mod profile;
mod starfield;
mod stretch;

#[allow(unused_imports)]
pub use ascii::render_ascii;
pub use profile::{prepare_density, PreparedDensity, RenderProfile};
pub use starfield::render_starfield;

use color::{intensity_to_ansi, intensity_to_background_ansi, RESET};
use hash::{hash_cell, hash_to_unit};

/// Renderiza o mapa de densidade usando caracteres de bloco Unicode meio a meio.
///
/// This is the true half-block renderer for galaxy-like models. It consumes pairs of density
/// rows and converts them to one terminal row using half-block characters.
///
/// The renderer:
/// - Takes two vertical density samples per terminal row
/// - Calculates independent visibility for top and bottom halves
/// - Uses ▀ for top-only, ▄ for bottom-only, █ for both, space for neither
/// - In color mode: uses foreground for top, background for bottom with ▀
///
/// Each terminal cell represents two vertical density samples:
/// ```text
/// top density row    -> upper half of the glyph (top visible)
/// bottom density row -> lower half of the glyph (bottom visible)
/// ```
pub fn render_half_blocks(
    canvas: &[Vec<f64>],
    threshold: f64,
    colors_enabled: bool,
) -> Vec<String> {
    let star_seed = star_field_seed(canvas);

    let width = canvas.first().map_or(0, Vec::len);
    let mut lines = Vec::with_capacity(canvas.len().div_ceil(2));

    for y in (0..canvas.len()).step_by(2) {
        let mut line = String::with_capacity(width);

        for x in 0..width {
            let top = canvas[y].get(x).copied().unwrap_or(0.0);
            let bottom = canvas
                .get(y + 1)
                .and_then(|row| row.get(x))
                .copied()
                .unwrap_or(0.0);

            // Calculate independent visibility for each half
            let top_visible = top.is_finite() && top > 0.0 && top >= threshold;
            let bottom_visible = bottom.is_finite() && bottom > 0.0 && bottom >= threshold;

            // Determine the glyph based on visibility
            let galaxy_ch = glyph_for_half_block(top_visible, bottom_visible);

            if galaxy_ch == ' ' {
                // Only inject background star if neither half is visible
                if let Some(star_ch) =
                    star_glyph_for_cell(x, y / 2, top, bottom, threshold, star_seed)
                {
                    // Keep stars uncolored for portability and to avoid turning the
                    // background into ANSI pixel-art noise.
                    line.push(star_ch);
                    continue;
                }
            }

            if colors_enabled && galaxy_ch != ' ' {
                // In color mode, we use foreground for top and background for bottom
                // with the ▀ glyph to preserve both samples independently
                if top_visible && bottom_visible {
                    // Both visible: foreground for top, background for bottom, ▀ glyph
                    line.push_str(intensity_to_ansi(top));
                    line.push_str(intensity_to_background_ansi(bottom));
                    line.push('▀');
                    line.push_str(RESET);
                } else if top_visible {
                    // Top only: foreground for top, ▀ glyph
                    line.push_str(intensity_to_ansi(top));
                    line.push('▀');
                    line.push_str(RESET);
                } else {
                    // Bottom only: foreground for bottom, ▄ glyph
                    line.push_str(intensity_to_ansi(bottom));
                    line.push('▄');
                    line.push_str(RESET);
                }
            } else {
                // No-color mode: use the appropriate half-block glyph
                line.push(galaxy_ch);
            }
        }

        lines.push(line);
    }

    lines
}

/// Returns the appropriate glyph for half-block rendering based on visibility.
fn glyph_for_half_block(top_visible: bool, bottom_visible: bool) -> char {
    match (top_visible, bottom_visible) {
        (true, true) => '█',
        (true, false) => '▀',
        (false, true) => '▄',
        (false, false) => ' ',
    }
}

/// Returns the visibility scale for a density value above threshold.
///
/// Returns None for:
/// - Non-finite values (NaN, infinity, negative infinity)
/// - Zero or negative values
/// - Values below threshold
///
/// Returns Some(scaled) for visible values, where scaled is a monotonic
/// mapping from [threshold, 1.0] to [0.0, 1.0].
#[allow(dead_code)]
pub(super) fn scale_visible(value: f64, threshold: f64) -> Option<f64> {
    // Reject non-finite values
    if !value.is_finite() {
        return None;
    }

    // Reject zero and negative
    if value <= 0.0 {
        return None;
    }

    // Reject below threshold (value exactly equal to threshold is visible)
    if value < threshold {
        return None;
    }

    // Map to [0.0, 1.0] where threshold -> 0.0 and 1.0 -> 1.0
    if threshold >= 1.0 {
        Some(1.0)
    } else {
        let scaled = ((value - threshold) / (1.0 - threshold)).clamp(0.0, 1.0);
        Some(scaled)
    }
}

pub(super) fn star_glyph_for_cell(
    x: usize,
    y: usize,
    top: f64,
    bottom: f64,
    threshold: f64,
    seed: u64,
) -> Option<char> {
    let local_density = top.max(bottom);

    // Do not draw background stars over visible galaxy structure.
    // A star should never replace either visible galaxy half.
    if local_density > threshold * 0.35 {
        return None;
    }

    let r = hash_to_unit(hash_cell(x, y, seed));

    // Sparse background:
    // - "." = faint common stars
    // - "*" = medium rare stars
    // - "+" = bright very rare stars
    if r < 0.0004 {
        Some('+')
    } else if r < 0.0022 {
        Some('*')
    } else if r < 0.0132 {
        Some('.')
    } else {
        None
    }
}

pub(super) fn star_field_seed(canvas: &[Vec<f64>]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;

    for (i, value) in canvas.iter().flatten().enumerate() {
        // Sample all values but quantize them. This keeps the star field
        // deterministic and makes it vary with the galaxy seed without passing
        // the CLI seed into the renderer API.
        let quantized = (value.clamp(0.0, 1.0) * 4096.0).round() as u64;
        hash ^= quantized.wrapping_add((i as u64).wrapping_mul(0x9e3779b97f4a7c15));
        hash = hash.wrapping_mul(0x100000001b3);
    }

    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_render() {
        let model = crate::engine::ArtModel::Starfield;
        let canvas1 = model.generate_scene(10, 5, Some(42)).density.into_rows();
        let canvas2 = model.generate_scene(10, 5, Some(42)).density.into_rows();

        assert_eq!(canvas1, canvas2);
    }

    #[test]
    fn test_render_starfield_collapses_density_rows() {
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let canvas = vec![
            vec![0.0, 0.04, 0.10, 0.20],
            vec![0.0, 0.0, 0.0, 0.0],
            vec![0.02, 0.08, 0.15, 0.18],
            vec![0.0, 0.0, 0.0, 0.0],
        ];

        let result = render_starfield(&canvas, false, &terminal);

        assert_eq!(result, vec![" .*+", " .++"]);
    }

    #[test]
    fn test_render_starfield_no_color_is_ansi_free() {
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let canvas = vec![vec![0.20]];

        let result = render_starfield(&canvas, false, &terminal);

        assert_eq!(result, vec!["+"]);
        assert!(!result[0].contains('\x1b'));
    }

    #[test]
    fn test_render_starfield_colored_contains_ansi() {
        let terminal = crate::terminal::Terminal::with_colors(true, true);
        let canvas = vec![vec![0.20]];

        let result = render_starfield(&canvas, true, &terminal);

        assert!(result[0].contains('\x1b'));
        assert!(result[0].contains('+'));
    }

    #[test]
    fn test_render_half_blocks_with_precomputed_threshold() {
        let canvas = vec![vec![1.0, 0.0, 1.0], vec![0.0, 1.0, 1.0]];

        // Use a low threshold so all cells are visible
        let result = render_half_blocks(&canvas, 0.0, false);

        // Cell 0: top=1.0 (visible), bottom=0.0 (not visible) → '▀'
        // Cell 1: top=0.0 (not visible), bottom=1.0 (visible) → '▄'
        // Cell 2: top=1.0 (visible), bottom=1.0 (visible) → '█'
        assert_eq!(result, vec!["▀▄█"]);
    }

    #[test]
    fn test_render_half_blocks_zero_map_is_empty() {
        // Create a canvas with at least two density rows, all zeros
        let canvas = vec![vec![0.0, 0.0, 0.0, 0.0], vec![0.0, 0.0, 0.0, 0.0]];

        // With any threshold, zero-density cells should render as spaces
        let result = render_half_blocks(&canvas, 0.0, false);

        // Each terminal row should contain only spaces (no galaxy glyphs)
        assert_eq!(result.len(), 1);
        assert!(result[0].chars().all(|c| c == ' '));
    }

    // ===== No-color half-block tests =====

    #[test]
    fn test_half_blocks_top_only() {
        // Top visible, bottom invisible → '▀'
        let canvas = vec![vec![0.5], vec![0.0]];
        let result = render_half_blocks(&canvas, 0.1, false);
        assert_eq!(result, vec!["▀"]);
    }

    #[test]
    fn test_half_blocks_bottom_only() {
        // Top invisible, bottom visible → '▄'
        let canvas = vec![vec![0.0], vec![0.5]];
        let result = render_half_blocks(&canvas, 0.1, false);
        assert_eq!(result, vec!["▄"]);
    }

    #[test]
    fn test_half_blocks_both_visible() {
        // Both visible → '█'
        let canvas = vec![vec![0.5], vec![0.5]];
        let result = render_half_blocks(&canvas, 0.1, false);
        assert_eq!(result, vec!["█"]);
    }

    #[test]
    fn test_half_blocks_neither_visible() {
        // Neither visible → ' '
        let canvas = vec![vec![0.0], vec![0.0]];
        let result = render_half_blocks(&canvas, 0.1, false);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_half_blocks_zero_values_are_invisible() {
        // Zero values should be invisible regardless of threshold
        let canvas = vec![vec![0.0], vec![0.0]];
        let result = render_half_blocks(&canvas, 0.0, false);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_half_blocks_negative_values_are_invisible() {
        // Negative values should be invisible
        let canvas = vec![vec![-0.5], vec![-0.5]];
        let result = render_half_blocks(&canvas, 0.0, false);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_half_blocks_non_finite_values_are_invisible() {
        // Non-finite values should be invisible
        let canvas = vec![vec![f64::NAN], vec![f64::INFINITY]];
        let result = render_half_blocks(&canvas, 0.0, false);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_half_blocks_odd_height() {
        // Test with odd number of rows (last row has no bottom)
        // 3 rows → ceil(3/2) = 2 terminal rows
        let canvas = vec![vec![0.5, 0.0], vec![0.0, 0.5], vec![0.5, 0.0]];
        let result = render_half_blocks(&canvas, 0.1, false);
        // Terminal row 0 (canvas rows 0-1):
        //   Cell 0: top=0.5 (visible), bottom=0.0 (not visible) → '▀'
        //   Cell 1: top=0.0 (not visible), bottom=0.5 (visible) → '▄'
        //   Result: "▀▄"
        // Terminal row 1 (canvas rows 2-3):
        //   Cell 0: top=0.5 (visible), bottom=0.0 (missing, invisible) → '▀'
        //   Cell 1: top=0.0 (not visible), bottom=0.0 (missing, invisible) → ' '
        //   Result: "▀ "
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "▀▄");
        assert_eq!(result[1], "▀ ");
    }

    #[test]
    fn test_half_blocks_preserve_terminal_width() {
        // Width should be preserved
        let canvas = vec![vec![0.5, 0.0, 0.5, 0.0], vec![0.0, 0.5, 0.0, 0.5]];
        let result = render_half_blocks(&canvas, 0.1, false);
        // 2 rows → 1 terminal row with 4 characters
        // Note: Unicode half-block characters are 3 bytes each in UTF-8
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].chars().count(), 4);
        // Check the actual content
        assert_eq!(result[0], "▀▄▀▄");
    }

    #[test]
    fn test_half_blocks_no_color_is_ansi_free() {
        // No-color mode should not contain ANSI sequences
        let canvas = vec![vec![0.5], vec![0.5]];
        let result = render_half_blocks(&canvas, 0.1, false);
        assert!(!result[0].contains('\x1b'));
        assert_eq!(result, vec!["█"]);
    }

    #[test]
    fn test_half_blocks_deterministic() {
        // Same input should produce same output
        let canvas = vec![vec![0.5, 0.0], vec![0.0, 0.5]];
        let result1 = render_half_blocks(&canvas, 0.1, false);
        let result2 = render_half_blocks(&canvas, 0.1, false);
        assert_eq!(result1, result2);
    }

    // ===== Colored half-block tests =====

    #[test]
    fn test_half_blocks_colored_top_only() {
        // Top only: foreground sequence + ▀ + reset
        let canvas = vec![vec![0.5], vec![0.0]];
        let result = render_half_blocks(&canvas, 0.1, true);

        let line = &result[0];
        // Top intensity 0.5 maps to index 65 (muted green) with sequence \x1b[38;5;65m
        // Expected: \x1b[38;5;65m▀\x1b[0m
        assert_eq!(line, "\x1b[38;5;65m▀\x1b[0m");
    }

    #[test]
    fn test_half_blocks_colored_bottom_only() {
        // Bottom only: foreground sequence + ▄ + reset
        let canvas = vec![vec![0.0], vec![0.5]];
        let result = render_half_blocks(&canvas, 0.1, true);

        let line = &result[0];
        // Bottom intensity 0.5 maps to index 65 (muted green) with sequence \x1b[38;5;65m
        // Expected: \x1b[38;5;65m▄\x1b[0m
        assert_eq!(line, "\x1b[38;5;65m▄\x1b[0m");
    }

    #[test]
    fn test_half_blocks_colored_both_visible() {
        // Both visible: foreground sequence + background sequence + ▀ + reset
        // Use different intensities so palette indexes differ
        // top = 0.50 -> index 65 (muted green) -> \x1b[38;5;65m
        // bottom = 0.80 -> index 130 (muted orange/red) -> \x1b[48;5;130m
        let canvas = vec![vec![0.50], vec![0.80]];
        let result = render_half_blocks(&canvas, 0.1, true);

        let line = &result[0];
        // Expected: \x1b[38;5;65m\x1b[48;5;130m▀\x1b[0m
        // 38;5;65 for foreground (top), 48;5;130 for background (bottom)
        assert_eq!(line, "\x1b[38;5;65m\x1b[48;5;130m▀\x1b[0m");
    }

    #[test]
    fn test_half_blocks_colored_ends_with_reset() {
        // Every visible cell should end with reset
        let canvas = vec![vec![0.5, 0.5], vec![0.5, 0.5]];
        let result = render_half_blocks(&canvas, 0.1, true);

        for line in &result {
            assert!(
                line.ends_with(RESET),
                "Line should end with reset: {}",
                line
            );
        }
    }

    #[test]
    fn test_half_blocks_no_ansi_leakage() {
        // ANSI state should not leak into later cells
        // Each visible cell should have its own color sequences followed by RESET
        // Cell 0: top visible (fg) + reset
        // Cell 1: bottom visible (fg) + reset
        // Cell 2: both visible (fg + bg) + reset
        // The key is that RESET occurs immediately after each colored glyph
        let canvas = vec![vec![0.5, 0.0, 0.5], vec![0.0, 0.5, 0.5]];
        let result = render_half_blocks(&canvas, 0.1, true);

        let line = &result[0];
        // Cell 0: top visible -> \x1b[38;5;65m + ▀ + \x1b[0m
        // Cell 1: bottom visible -> \x1b[38;5;65m + ▄ + \x1b[0m
        // Cell 2: both visible -> \x1b[38;5;65m + \x1b[48;5;65m + ▀ + \x1b[0m
        // Expected: \x1b[38;5;65m▀\x1b[0m\x1b[38;5;65m▄\x1b[0m\x1b[38;5;65m\x1b[48;5;65m▀\x1b[0m
        let expected =
            "\x1b[38;5;65m▀\x1b[0m\x1b[38;5;65m▄\x1b[0m\x1b[38;5;65m\x1b[48;5;65m▀\x1b[0m";
        assert_eq!(line, expected);
    }

    #[test]
    fn test_half_blocks_background_star_never_overwrites_visible() {
        // Background stars should never overwrite visible galaxy halves
        let canvas = vec![vec![0.5], vec![0.0]];
        let result = render_half_blocks(&canvas, 0.1, false);

        // Top is visible, so no star should appear
        assert_eq!(result, vec!["▀"]);
    }

    /// Tests the deterministic background-star helper directly.
    ///
    /// This test finds a coordinate where a star is emitted and verifies:
    /// 1. Such a coordinate exists
    /// 2. Repeated calls return the exact same glyph
    /// 3. The glyph is one of the supported background-star glyphs
    #[test]
    fn test_star_glyph_for_cell_deterministic() {
        // Create a canvas that will produce a deterministic star field seed
        let canvas = vec![vec![0.0, 0.0], vec![0.0, 0.0]];

        // Get the seed used by render_half_blocks
        let star_seed = star_field_seed(&canvas);

        // Threshold high enough that no galaxy structure is visible
        let threshold = 0.5;

        // Search bounded range for a coordinate that produces a star
        let mut found_star = false;
        let mut star_glyph = ' ';
        let mut star_x = 0;
        let mut star_y = 0;

        for y in 0..50 {
            for x in 0..50 {
                let top = 0.0;
                let bottom = 0.0;
                if let Some(glyph) = star_glyph_for_cell(x, y, top, bottom, threshold, star_seed) {
                    found_star = true;
                    star_glyph = glyph;
                    star_x = x;
                    star_y = y;
                    break;
                }
            }
            if found_star {
                break;
            }
        }

        // Assert that a star-emitting coordinate exists
        assert!(
            found_star,
            "Expected to find a coordinate where star_glyph_for_cell returns a star"
        );

        // Assert that repeated calls return the exact same glyph
        for _ in 0..10 {
            let glyph = star_glyph_for_cell(star_x, star_y, 0.0, 0.0, threshold, star_seed);
            assert_eq!(
                glyph,
                Some(star_glyph),
                "Star glyph should be deterministic"
            );
        }

        // Assert the glyph is one of the supported background-star glyphs
        assert!(
            star_glyph == '.' || star_glyph == '*' || star_glyph == '+',
            "Star glyph should be one of '.', '*', or '+', got '{}'",
            star_glyph
        );
    }

    #[test]
    fn test_half_blocks_deterministic_star_repeated_calls() {
        // Repeated calls with same canvas should produce same star result
        let canvas = vec![vec![0.0], vec![0.0]];
        let result1 = render_half_blocks(&canvas, 0.5, false);
        let result2 = render_half_blocks(&canvas, 0.5, false);
        assert_eq!(result1, result2, "Star field should be deterministic");
    }

    #[test]
    fn test_half_blocks_star_prevented_by_visible_galaxy() {
        // A visible galaxy half should prevent star replacement
        // Top visible (0.5 > 0.1 threshold), so no star should appear
        let canvas = vec![vec![0.5], vec![0.0]];
        let result = render_half_blocks(&canvas, 0.1, false);
        assert_eq!(result, vec!["▀"]);
    }

    #[test]
    fn test_half_blocks_starfield_unchanged() {
        // Starfield rendering should remain unchanged
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let canvas = vec![
            vec![0.0, 0.04, 0.10, 0.20],
            vec![0.0, 0.0, 0.0, 0.0],
            vec![0.02, 0.08, 0.15, 0.18],
            vec![0.0, 0.0, 0.0, 0.0],
        ];

        let result = render_starfield(&canvas, false, &terminal);

        assert_eq!(result, vec![" .*+", " .++"]);
    }
}
