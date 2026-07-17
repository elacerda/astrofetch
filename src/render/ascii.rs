use super::ansi::AnsiForegroundLine;
use super::color::{galaxy_foreground_ansi, ColorPalette};
use super::{scale_visible, star_field_seed, star_glyph_for_cell};

/// ASCII glyphs ordered from lowest to highest intensity.
const ASCII_GLYPHS: &[char] = &['.', ':', '-', '=', '+', '*', '#', '%', '@'];

/// Renders a density canvas as ASCII art.
///
/// The renderer consumes pairs of density rows and converts them to one terminal
/// row using ASCII glyphs. It supports optional ANSI color output.
///
/// Each terminal cell represents the maximum of two vertical density samples:
/// ```text
/// top density row    -> upper half ( contributes to cell value )
/// bottom density row -> lower half ( contributes to cell value )
/// ```
pub fn render_ascii(
    canvas: &[Vec<f64>],
    threshold: f64,
    colors_enabled: bool,
    palette: ColorPalette,
) -> Vec<String> {
    let star_seed = star_field_seed(canvas);
    let mut lines = Vec::with_capacity(canvas.len().div_ceil(2));

    for y in (0..canvas.len()).step_by(2) {
        let mut line = AnsiForegroundLine::with_capacity(64);

        // Determine the max width of this terminal row (pair of canvas rows)
        let row_width = canvas
            .get(y)
            .map_or(0, |r| r.len())
            .max(canvas.get(y + 1).map_or(0, |r| r.len()));

        for x in 0..row_width {
            // Get vertical pair maximum
            let top = canvas.get(y).and_then(|r| r.get(x)).copied().unwrap_or(0.0);
            let bottom = canvas
                .get(y + 1)
                .and_then(|r| r.get(x))
                .copied()
                .unwrap_or(0.0);
            let value = top.max(bottom);

            // Check if the galaxy cell is visible
            if let Some(scaled) = scale_visible(value, threshold) {
                let ch = glyph_for_value(scaled);

                if colors_enabled {
                    // Color mode: use the original maximum value for color mapping
                    // The builder handles style grouping and final RESET
                    line.push_styled(ch, galaxy_foreground_ansi(palette, value));
                } else {
                    // No-color mode: pure ASCII only
                    line.push_plain(ch);
                }
            } else {
                // No visible galaxy - fall back to background star or space
                if let Some(star_ch) =
                    star_glyph_for_cell(x, y / 2, top, bottom, threshold, star_seed)
                {
                    line.push_plain(star_ch);
                } else {
                    line.push_plain(' ');
                }
            }
        }

        lines.push(line.finish());
    }

    lines
}

/// Maps a scaled value (0.0 to 1.0) to an ASCII glyph.
///
/// 0.0 → '.'
/// 1.0 → '@'
fn glyph_for_value(value: f64) -> char {
    if value <= 0.0 {
        return ASCII_GLYPHS[0];
    }
    if value >= 1.0 {
        return ASCII_GLYPHS[ASCII_GLYPHS.len() - 1];
    }

    let idx = (value * (ASCII_GLYPHS.len() - 1) as f64).floor() as usize;
    ASCII_GLYPHS[idx]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::color::RESET;

    #[test]
    fn test_empty_canvas() {
        let canvas: Vec<Vec<f64>> = vec![];
        let result: Vec<String> = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        let expected: Vec<String> = vec![];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_two_rows_become_one_line() {
        let canvas = vec![vec![0.5], vec![0.5]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        // value 0.5, threshold 0.1:
        // scaled = (0.5-0.1)/(1-0.1) = 0.444...
        // idx = floor(0.444... * 8) = 3
        // ASCII_GLYPHS[3] = '='
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "=");
    }

    #[test]
    fn test_odd_row_count() {
        // 3 rows → ceil(3/2) = 2 terminal rows
        // All values are 0.5, threshold is 0.1
        // scaled = (0.5-0.1)/(1-0.1) = 0.444..., idx = 3, glyph = '='
        let canvas = vec![vec![0.5], vec![0.5], vec![0.5]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "=");
        assert_eq!(result[1], "=");
    }

    #[test]
    fn test_one_row() {
        // value 0.5, threshold 0.1: scaled = 0.444..., idx = 3, glyph = '='
        let canvas = vec![vec![0.5]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "=");
    }

    #[test]
    fn test_one_column() {
        // 3 rows → 2 terminal rows
        // Row 0: max(0.1, 0.2) = 0.2, scaled = (0.2-0.1)/0.9 = 0.111..., idx = 0.888..., glyph = '.'
        // Row 1: 0.3, scaled = (0.3-0.1)/0.9 = 0.222..., idx = 1.777..., glyph = ':'
        let canvas = vec![vec![0.1], vec![0.2], vec![0.3]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ".");
        assert_eq!(result[1], ":");
    }

    #[test]
    fn test_one_by_one() {
        let canvas = vec![vec![1.0]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result, vec!["@"]);
    }

    #[test]
    fn test_ragged_rows() {
        // First row shorter than second
        // value 0.5 with threshold 0.1 gives '='
        let canvas = vec![vec![0.5], vec![0.5, 0.5, 0.5]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "===");

        // Second row shorter than first
        let canvas = vec![vec![0.5, 0.5, 0.5], vec![0.5]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "===");
    }

    #[test]
    fn test_below_threshold_invisible() {
        let canvas = vec![vec![0.05]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        // Below threshold - should be space (no star at x=0, y=0 with threshold 0.1)
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_exactly_threshold_visible_as_dot() {
        let canvas = vec![vec![0.1]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        // Exactly at threshold - should be visible as '.'
        assert_eq!(result, vec!["."]);
    }

    #[test]
    fn test_maximum_visible_as_at() {
        let canvas = vec![vec![1.0]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result, vec!["@"]);
    }

    #[test]
    fn test_monotonic_glyph_mapping() {
        // Test that increasing values produce increasing glyphs
        // Canvas has 10 rows, which gives 5 terminal rows (pairs processed)
        let canvas = vec![
            vec![0.1], // row 0
            vec![0.2], // row 1
            vec![0.3], // row 2
            vec![0.4], // row 3
            vec![0.5], // row 4
            vec![0.6], // row 5
            vec![0.7], // row 6
            vec![0.8], // row 7
            vec![0.9], // row 8
            vec![1.0], // row 9
        ];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);

        // With threshold 0.1, paired values (max) produce:
        // result[0]: max(0.1, 0.2) = 0.2, scaled = 0.111..., idx = 0.888... -> '.'
        // result[1]: max(0.3, 0.4) = 0.4, scaled = 0.333..., idx = 2.666... -> '-'
        // result[2]: max(0.5, 0.6) = 0.6, scaled = 0.555..., idx = 4.444... -> '+'
        // result[3]: max(0.7, 0.8) = 0.8, scaled = 0.777..., idx = 6.222... -> '#'
        // result[4]: max(0.9, 1.0) = 1.0, scaled = 1.0, idx = 8 -> '@'
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], ".");
        assert_eq!(result[1], "-");
        assert_eq!(result[2], "+");
        assert_eq!(result[3], "#");
        assert_eq!(result[4], "@");
    }

    #[test]
    fn test_nan_invisible() {
        let canvas = vec![vec![f64::NAN]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_infinity_invisible() {
        let canvas = vec![vec![f64::INFINITY]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_negative_invisible() {
        let canvas = vec![vec![-0.5]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_zero_invisible() {
        let canvas = vec![vec![0.0]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_no_color_output_only_ascii() {
        let canvas = vec![vec![0.5]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        // Should only contain ASCII bytes
        assert!(result[0].is_ascii());
        // Should be '=' for value 0.5 (scaled = 0.444..., idx = 3)
        assert_eq!(result[0], "=");
    }

    #[test]
    fn test_no_color_no_ansi() {
        let canvas = vec![vec![0.5]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert!(!result[0].contains('\x1b'));
        assert_eq!(result[0], "=");
    }

    #[test]
    fn test_colored_contains_ansi() {
        let canvas = vec![vec![0.5]];
        let result = render_ascii(&canvas, 0.1, true, ColorPalette::Nebula);
        // Should contain ANSI escape sequence and end with RESET
        assert!(result[0].contains('\x1b'));
        // The glyph for 0.5 is '=' (not '+')
        assert!(result[0].contains('='));
        assert!(result[0].ends_with(RESET));
    }

    #[test]
    fn test_deterministic_output() {
        let canvas = vec![vec![0.5, 0.0], vec![0.0, 0.5]];
        let result1 = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        let result2 = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_background_stars_do_not_overwrite_visible_cells() {
        // A visible cell should never be replaced by a star
        let canvas = vec![vec![0.5], vec![0.0]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        // The top is visible (0.5 >= 0.1), so no star should appear
        // value 0.5 with threshold 0.1 gives '=' (not '+')
        assert_eq!(result, vec!["="]);
    }

    #[test]
    fn test_expected_output_dimensions() {
        // 10 rows -> 5 terminal rows
        let canvas = vec![vec![0.5; 20]; 10];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result.len(), 5);
        for line in &result {
            assert_eq!(line.len(), 20);
        }
    }

    #[test]
    fn test_ascii_glyphs_const_is_correct() {
        assert_eq!(ASCII_GLYPHS.len(), 9);
        assert_eq!(ASCII_GLYPHS[0], '.');
        assert_eq!(ASCII_GLYPHS[8], '@');
    }

    #[test]
    fn test_scale_visible_below_threshold() {
        assert!(scale_visible(0.05, 0.1).is_none());
        assert!(scale_visible(0.0, 0.1).is_none());
        assert!(scale_visible(-0.1, 0.1).is_none());
    }

    #[test]
    fn test_scale_visible_at_threshold() {
        let scaled = scale_visible(0.1, 0.1);
        assert_eq!(scaled, Some(0.0));
    }

    #[test]
    fn test_scale_visible_at_one() {
        let scaled = scale_visible(1.0, 0.1);
        assert_eq!(scaled, Some(1.0));
    }

    #[test]
    fn test_scale_visible_non_finite() {
        assert!(scale_visible(f64::NAN, 0.1).is_none());
        assert!(scale_visible(f64::INFINITY, 0.1).is_none());
        assert!(scale_visible(f64::NEG_INFINITY, 0.1).is_none());
    }

    #[test]
    fn test_scale_visible_threshold_at_one() {
        // When threshold >= 1.0, any value >= threshold should map to 1.0
        let scaled = scale_visible(1.0, 1.0);
        assert_eq!(scaled, Some(1.0));
    }

    #[test]
    fn test_glyph_for_value_edge_cases() {
        assert_eq!(glyph_for_value(0.0), '.');
        assert_eq!(glyph_for_value(1.0), '@');
        assert_eq!(glyph_for_value(-0.1), '.');
        assert_eq!(glyph_for_value(1.5), '@');
    }

    #[test]
    fn test_star_glyph_for_cell_deterministic() {
        let seed = 42u64;
        let x = 5;
        let y = 3;

        let glyph1 = star_glyph_for_cell(x, y, 0.0, 0.0, 0.5, seed);
        let glyph2 = star_glyph_for_cell(x, y, 0.0, 0.0, 0.5, seed);

        assert_eq!(glyph1, glyph2);
    }

    #[test]
    fn test_star_glyph_for_cell_visible_galaxy_prevents_star() {
        // High local density should prevent star
        let seed = 42u64;
        let result = star_glyph_for_cell(0, 0, 0.5, 0.5, 0.1, seed);
        assert!(result.is_none());
    }

    #[test]
    fn test_star_field_seed_is_deterministic() {
        let canvas = vec![vec![0.5, 0.3], vec![0.7, 0.1]];
        let seed1 = star_field_seed(&canvas);
        let seed2 = star_field_seed(&canvas);
        assert_eq!(seed1, seed2);
    }

    #[test]
    fn test_empty_background_uses_only_star_or_space_glyphs() {
        // Create a mostly-empty canvas to test star injection
        let canvas = vec![vec![0.0; 10]; 2];
        let result = render_ascii(&canvas, 0.5, false, ColorPalette::Nebula);

        assert_eq!(result.len(), 1);
        // With no visible galaxy, stars should appear occasionally
        // The key test is that stars don't appear where galaxy would be visible
        assert!(result[0]
            .chars()
            .all(|c| c == ' ' || c == '.' || c == '*' || c == '+'));
    }

    // ===== ANSI grouping regression tests =====

    #[test]
    fn test_same_color_run_uses_single_ansi_sequence() {
        let threshold = 0.1;
        let value_a = 0.16;
        let value_b = 0.29;

        let scaled_a = scale_visible(value_a, threshold).unwrap();
        let scaled_b = scale_visible(value_b, threshold).unwrap();

        let glyph_a = glyph_for_value(scaled_a);
        let glyph_b = glyph_for_value(scaled_b);

        assert_ne!(glyph_a, glyph_b);

        let style_a = galaxy_foreground_ansi(ColorPalette::Nebula, value_a);
        let style_b = galaxy_foreground_ansi(ColorPalette::Nebula, value_b);

        assert_eq!(style_a, style_b);

        let canvas = vec![vec![value_a, value_b, value_a, value_b], vec![0.0; 4]];

        let result = render_ascii(&canvas, 0.1, true, ColorPalette::Nebula);

        assert_eq!(
            result[0],
            format!("{style_a}{glyph_a}{glyph_b}{glyph_a}{glyph_b}{RESET}")
        );

        let style_count = result[0].matches(style_a).count();
        let reset_count = result[0].matches(RESET).count();

        assert_eq!(
            style_count, 1,
            "Same-style run should use single style sequence"
        );
        assert_eq!(reset_count, 1, "Same-style run should use single reset");
    }

    #[test]
    fn test_style_transition_pushes_reset_before_new_style() {
        let threshold = 0.1;
        let value_a = 0.15;
        let value_b = 0.20;

        let scaled_a = scale_visible(value_a, threshold).unwrap();
        let scaled_b = scale_visible(value_b, threshold).unwrap();

        let glyph_a = glyph_for_value(scaled_a);
        let glyph_b = glyph_for_value(scaled_b);

        let style_a = galaxy_foreground_ansi(ColorPalette::Nebula, value_a);
        let style_b = galaxy_foreground_ansi(ColorPalette::Nebula, value_b);

        assert_ne!(style_a, style_b);

        let canvas = vec![vec![value_a, value_b], vec![0.0; 2]];

        let result = render_ascii(&canvas, 0.1, true, ColorPalette::Nebula);

        assert_eq!(
            result[0],
            format!("{style_a}{glyph_a}{RESET}{style_b}{glyph_b}{RESET}")
        );
    }

    #[test]
    fn test_colored_to_plain_transition() {
        let threshold = 0.1;
        let value = 0.5;
        let glyph = glyph_for_value(scale_visible(value, threshold).unwrap());
        let style = galaxy_foreground_ansi(ColorPalette::Nebula, value);

        let canvas = vec![vec![value, 0.0], vec![0.0; 2]];

        let result = render_ascii(&canvas, 0.1, true, ColorPalette::Nebula);

        assert_eq!(result[0], format!("{}{}{} ", style, glyph, RESET));
    }

    #[test]
    fn test_no_color_regression_unchanged() {
        // No-color mode should produce identical output to original
        let canvas = vec![vec![0.5, 0.5, 0.5], vec![0.0; 3]];
        let result = render_ascii(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result, vec!["==="]);
    }
}
