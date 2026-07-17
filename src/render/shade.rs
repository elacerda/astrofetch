use super::color::{galaxy_foreground_ansi, ColorPalette, RESET};
use super::{scale_visible, star_field_seed, star_glyph_for_cell};

/// Shade glyphs ordered from lowest to highest intensity.
const SHADE_GLYPHS: &[char] = &['░', '▒', '▓', '█'];

/// Renderiza o mapa de densidade usando caracteres Shade Unicode.
///
/// O renderizador consome pares de linhas de densidade e os converte em uma linha
/// de terminal usando caracteres Shade. Suporta saída ANSI colorida.
///
/// Cada célula terminal representa o máximo de duas amostras de densidade vertical:
/// ```text
/// linha superior de densidade    -> metade superior (contribui para o valor da célula)
/// linha inferior de densidade   -> metade inferior (contribui para o valor da célula)
/// ```
pub fn render_shades(
    canvas: &[Vec<f64>],
    threshold: f64,
    colors_enabled: bool,
    palette: ColorPalette,
) -> Vec<String> {
    let star_seed = star_field_seed(canvas);
    let mut lines = Vec::with_capacity(canvas.len().div_ceil(2));

    for y in (0..canvas.len()).step_by(2) {
        let mut line = String::with_capacity(64);

        // Determina a largura máxima desta linha terminal (par de linhas de canvas)
        let top_row_width = canvas.get(y).map_or(0, |r| r.len());
        let bottom_row_width = canvas.get(y + 1).map_or(0, |r| r.len());
        let row_width = top_row_width.max(bottom_row_width);

        for x in 0..row_width {
            // Obtém o máximo do par vertical
            let top = canvas.get(y).and_then(|r| r.get(x)).copied().unwrap_or(0.0);
            let bottom = canvas
                .get(y + 1)
                .and_then(|r| r.get(x))
                .copied()
                .unwrap_or(0.0);
            let value = top.max(bottom);

            // Verifica se a célula da galáxia évisível
            if let Some(scaled) = scale_visible(value, threshold) {
                let ch = glyph_for_value(scaled);

                if colors_enabled {
                    // Modo colorido: usa o valor original do máximo para mapeamento de cor
                    // e anexa RESET após o glifo
                    let color = galaxy_foreground_ansi(palette, value);
                    line.push_str(color);
                    line.push(ch);
                    line.push_str(RESET);
                } else {
                    // Modo sem cor: apenas caracteres Shade
                    line.push(ch);
                }
            } else {
                // Nenhuma galáxia visível - usa estrela de fundo ou espaço
                if let Some(star_ch) =
                    star_glyph_for_cell(x, y / 2, top, bottom, threshold, star_seed)
                {
                    line.push(star_ch);
                } else {
                    line.push(' ');
                }
            }
        }

        lines.push(line);
    }

    lines
}

/// Mapeia um valor escalado (0.0 a 1.0) para um glifo Shade.
///
/// 0.0 → ░
/// 1.0 → █
fn glyph_for_value(value: f64) -> char {
    if value <= 0.0 {
        return SHADE_GLYPHS[0];
    }
    if value >= 1.0 {
        return SHADE_GLYPHS[SHADE_GLYPHS.len() - 1];
    }

    let idx = (value * (SHADE_GLYPHS.len() - 1) as f64).floor() as usize;
    SHADE_GLYPHS[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_canvas() {
        let canvas: Vec<Vec<f64>> = vec![];
        let result: Vec<String> = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        let expected: Vec<String> = vec![];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_two_rows_become_one_line() {
        let canvas = vec![vec![0.5], vec![0.5]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        // value 0.5, threshold 0.1:
        // scaled = (0.5-0.1)/(1-0.1) = 0.444...
        // idx = floor(0.444... * 3) = 1
        // SHADE_GLYPHS[1] = '▒'
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "▒");
    }

    #[test]
    fn test_odd_row_count() {
        // 3 rows → ceil(3/2) = 2 terminal rows
        // All values are 0.5, threshold is 0.1
        // scaled = (0.5-0.1)/(1-0.1) = 0.444..., idx = 1, glyph = '▒'
        let canvas = vec![vec![0.5], vec![0.5], vec![0.5]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "▒");
        assert_eq!(result[1], "▒");
    }

    #[test]
    fn test_one_row() {
        // value 0.5, threshold 0.1: scaled = 0.444..., idx = 1, glyph = '▒'
        let canvas = vec![vec![0.5]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "▒");
    }

    #[test]
    fn test_one_column() {
        // 3 rows → 2 terminal rows
        // Row 0: max(0.1, 0.2) = 0.2, scaled = (0.2-0.1)/0.9 = 0.111..., idx = floor(0.111... * 3) = 0 -> '░'
        // Row 1: 0.3, scaled = (0.3-0.1)/0.9 = 0.222..., idx = floor(0.222... * 3) = 0 -> '░'
        let canvas = vec![vec![0.1], vec![0.2], vec![0.3]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "░");
        assert_eq!(result[1], "░");
    }

    #[test]
    fn test_one_by_one() {
        let canvas = vec![vec![1.0]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result, vec!["█"]);
    }

    #[test]
    fn test_ragged_rows() {
        // First row shorter than second
        // value 0.5 with threshold 0.1 gives '▒'
        let canvas = vec![vec![0.5], vec![0.5, 0.5, 0.5]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "▒▒▒");

        // Second row shorter than first
        let canvas = vec![vec![0.5, 0.5, 0.5], vec![0.5]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "▒▒▒");
    }

    #[test]
    fn test_below_threshold_invisible() {
        let canvas = vec![vec![0.05]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        // Below threshold - should be space (no star at x=0, y=0 with threshold 0.1)
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_exactly_threshold_visible_as_pane() {
        let canvas = vec![vec![0.1]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        // Exactly at threshold - should be visible as '░'
        assert_eq!(result, vec!["░"]);
    }

    #[test]
    fn test_maximum_visible_as_block() {
        let canvas = vec![vec![1.0]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result, vec!["█"]);
    }

    #[test]
    fn test_monotonic_glyph_mapping() {
        // Test that increasing values produce increasing glyphs
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
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);

        // With threshold 0.1, paired values (max) produce:
        // result[0]: max(0.1, 0.2) = 0.2, scaled = 0.111..., idx = floor(0.111... * 3) = 0 -> '░'
        // result[1]: max(0.3, 0.4) = 0.4, scaled = 0.333..., idx = floor(0.333... * 3) = 1 -> '▒'
        // result[2]: max(0.5, 0.6) = 0.6, scaled = 0.555..., idx = floor(0.555... * 3) = 1 -> '▒'
        // result[3]: max(0.7, 0.8) = 0.8, scaled = 0.777..., idx = floor(0.777... * 3) = 2 -> '▓'
        // result[4]: max(0.9, 1.0) = 1.0, scaled = 1.0, idx = floor(1.0 * 3) = 3 -> '█'
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], "░");
        assert_eq!(result[1], "▒");
        assert_eq!(result[2], "▒");
        assert_eq!(result[3], "▓");
        assert_eq!(result[4], "█");
    }

    #[test]
    fn test_nan_invisible() {
        let canvas = vec![vec![f64::NAN]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_infinity_invisible() {
        let canvas = vec![vec![f64::INFINITY]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_negative_invisible() {
        let canvas = vec![vec![-0.5]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_zero_invisible() {
        let canvas = vec![vec![0.0]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result, vec![" "]);
    }

    #[test]
    fn test_no_color_output_only_shade_and_star_glyphs() {
        let canvas = vec![vec![0.5]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        // Should only contain shade glyphs
        assert!(result[0].chars().all(|c| "░▒▓█".contains(c)));
        // Should be '▒' for value 0.5 (scaled = 0.444..., idx = 1)
        assert_eq!(result[0], "▒");
    }

    #[test]
    fn test_no_color_no_ansi() {
        let canvas = vec![vec![0.5]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert!(!result[0].contains('\x1b'));
        assert_eq!(result[0], "▒");
    }

    #[test]
    fn test_colored_contains_ansi() {
        let canvas = vec![vec![0.5]];
        let result = render_shades(&canvas, 0.1, true, ColorPalette::Nebula);
        // Should contain ANSI escape sequence and end with RESET
        assert!(result[0].contains('\x1b'));
        // The glyph for 0.5 is '▒' (not '+')
        assert!(result[0].contains('▒'));
        assert!(result[0].ends_with(RESET));
    }

    #[test]
    fn test_deterministic_output() {
        let canvas = vec![vec![0.5, 0.0], vec![0.0, 0.5]];
        let result1 = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        let result2 = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_background_stars_do_not_overwrite_visible_cells() {
        // A visible cell should never be replaced by a star
        let canvas = vec![vec![0.5], vec![0.0]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        // The top is visible (0.5 >= 0.1), so no star should appear
        assert_eq!(result, vec!["▒"]);
    }

    #[test]
    fn test_expected_output_dimensions() {
        // 10 rows -> 5 terminal rows
        let canvas = vec![vec![0.5; 20]; 10];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result.len(), 5);
        for line in &result {
            assert_eq!(line.chars().count(), 20);
        }
    }

    #[test]
    fn test_shade_glyphs_const_is_correct() {
        assert_eq!(SHADE_GLYPHS.len(), 4);
        assert_eq!(SHADE_GLYPHS[0], '░');
        assert_eq!(SHADE_GLYPHS[1], '▒');
        assert_eq!(SHADE_GLYPHS[2], '▓');
        assert_eq!(SHADE_GLYPHS[3], '█');
    }

    #[test]
    fn test_glyph_for_value_edge_cases() {
        assert_eq!(glyph_for_value(0.0), '░');
        assert_eq!(glyph_for_value(1.0), '█');
        assert_eq!(glyph_for_value(-0.1), '░');
        assert_eq!(glyph_for_value(1.5), '█');
    }

    #[test]
    fn test_background_stars_use_only_space_dot_star_plus() {
        // Create a mostly-empty canvas to test star injection
        let canvas = vec![vec![0.0; 10]; 2];
        let result = render_shades(&canvas, 0.5, false, ColorPalette::Nebula);

        assert_eq!(result.len(), 1);
        // With no visible galaxy, stars should appear occasionally
        // The key test is that stars don't appear where galaxy would be visible
        assert!(result[0].chars().all(|c| " ░▒▓█.*+".contains(c)));
    }

    #[test]
    fn test_colored_output_ends_with_reset() {
        // Every visible cell should end with reset
        let canvas = vec![vec![0.5, 0.5], vec![0.5, 0.5]];
        let result = render_shades(&canvas, 0.1, true, ColorPalette::Nebula);

        for line in &result {
            assert!(
                line.ends_with(RESET),
                "Line should end with reset: {}",
                line
            );
        }
    }

    #[test]
    fn test_colored_output_contains_ansi_sequences() {
        let canvas = vec![vec![0.5]];
        let result = render_shades(&canvas, 0.1, true, ColorPalette::Nebula);
        let line = &result[0];

        // Should contain ANSI escape sequence
        assert!(line.contains('\x1b'), "Line should contain ANSI: {}", line);
        // Should contain the visible glyph
        assert!(line.contains('▒'), "Line should contain '▒': {}", line);
        // Should end with reset
        assert!(
            line.ends_with(RESET),
            "Line should end with reset: {}",
            line
        );
    }

    #[test]
    fn test_repeated_calls_produce_identical_output() {
        let canvas = vec![vec![0.0, 0.1, 0.5, 1.0], vec![0.2, 0.3, 0.6, 0.0]];
        let result1 = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        let result2 = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        let result3 = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
        assert_eq!(result1, result2);
        assert_eq!(result2, result3);
    }

    #[test]
    fn test_background_stars_never_overwrite_visible_cells_comprehensive() {
        // Test at various coordinates where galaxy is visible
        let canvas = vec![vec![0.5, 0.0, 0.5], vec![0.0, 0.5, 0.0]];
        let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);

        // Every position with galaxy should be a shade glyph, never a star
        for line in &result {
            for ch in line.chars() {
                if ch != ' ' && ch != '.' && ch != '*' && ch != '+' {
                    // Must be a shade glyph
                    assert!("░▒▓█".contains(ch), "Unexpected character: '{}'", ch);
                }
            }
        }
    }

    #[test]
    fn test_output_dimensions_always_correct() {
        // Test with various heights to verify ceil(height/2)
        for height in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
            let canvas = vec![vec![0.5; 5]; height];
            let result = render_shades(&canvas, 0.1, false, ColorPalette::Nebula);
            let expected_lines = height.div_ceil(2);
            assert_eq!(
                result.len(),
                expected_lines,
                "Height {} should produce {} lines, got {}",
                height,
                expected_lines,
                result.len()
            );
        }
    }
}
