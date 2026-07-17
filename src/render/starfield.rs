use super::{
    ansi::AnsiForegroundLine,
    color::{starfield_foreground_ansi, ColorPalette},
    hash::{hash_cell, hash_to_unit},
};
use crate::terminal::Terminal;

pub fn render_starfield(
    canvas: &[Vec<f64>],
    colors_enabled: bool,
    terminal: &Terminal,
    palette: ColorPalette,
) -> Vec<String> {
    let width = canvas.first().map_or(0, Vec::len);
    let mut lines = Vec::with_capacity(canvas.len().div_ceil(2));

    for y in (0..canvas.len()).step_by(2) {
        let mut line = AnsiForegroundLine::with_capacity(width);

        for x in 0..width {
            let top = canvas[y].get(x).copied().unwrap_or(0.0);
            let bottom = canvas
                .get(y + 1)
                .and_then(|row| row.get(x))
                .copied()
                .unwrap_or(0.0);

            let value = top.max(bottom);
            let ch = starfield_glyph(value);

            if colors_enabled && terminal.colors_enabled() && ch != ' ' {
                let hue = hash_to_unit(hash_cell(x, y / 2, 0x51a7_f17e_d00d_cafe));
                let color = starfield_foreground_ansi(palette, value, hue);
                line.push_styled(ch, color);
            } else {
                line.push_plain(ch);
            }
        }

        lines.push(line.finish());
    }

    lines
}

fn starfield_glyph(value: f64) -> char {
    if value < 0.030 {
        ' '
    } else if value < 0.085 {
        '.'
    } else if value < 0.150 {
        '*'
    } else {
        '+'
    }
}

#[cfg(test)]
mod tests {
    use super::{render_starfield, starfield_foreground_ansi, starfield_glyph};
    use crate::render::color::{ColorPalette, RESET};
    use crate::render::hash::{hash_cell, hash_to_unit};
    use crate::terminal::Terminal;

    // ===== Starfield ANSI grouping tests =====

    #[test]
    fn test_starfield_same_style_run_uses_single_ansi_sequence() {
        let seed = 0x51a7_f17e_d00d_cafe;
        let hue_a = hash_to_unit(hash_cell(0, 0, seed));
        let hue_b = hash_to_unit(hash_cell(1, 0, seed));

        let value_a = 0.05;
        let value_b = 0.05;

        let glyph_a = starfield_glyph(value_a);
        let glyph_b = starfield_glyph(value_b);

        let style_a = starfield_foreground_ansi(ColorPalette::Nebula, value_a, hue_a);
        let style_b = starfield_foreground_ansi(ColorPalette::Nebula, value_b, hue_b);

        assert_eq!(glyph_a, glyph_b);
        assert_eq!(style_a, style_b);

        let canvas = vec![vec![value_a, value_b, value_a, value_b], vec![0.0; 4]];

        let terminal = Terminal::with_colors(true, true);
        let result = render_starfield(&canvas, true, &terminal, ColorPalette::Nebula);

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
    fn test_starfield_style_transition_pushes_reset() {
        let seed = 0x51a7_f17e_d00d_cafe;

        let hue_a = hash_to_unit(hash_cell(0, 0, seed));
        let hue_b = hash_to_unit(hash_cell(1, 0, seed));

        let value_a = 0.10;
        let value_b = 0.20;

        let glyph_a = starfield_glyph(value_a);
        let glyph_b = starfield_glyph(value_b);

        let style_a = starfield_foreground_ansi(ColorPalette::Nebula, value_a, hue_a);
        let style_b = starfield_foreground_ansi(ColorPalette::Nebula, value_b, hue_b);

        assert_ne!(style_a, style_b);

        let canvas = vec![vec![value_a, value_b], vec![0.0; 2]];

        let terminal = Terminal::with_colors(true, true);
        let result = render_starfield(&canvas, true, &terminal, ColorPalette::Nebula);

        assert_eq!(
            result[0],
            format!("{style_a}{glyph_a}{RESET}{style_b}{glyph_b}{RESET}")
        );
    }

    #[test]
    fn test_starfield_colored_to_plain() {
        let value = 0.05;
        let glyph = starfield_glyph(value);

        let seed = 0x51a7_f17e_d00d_cafe;
        let hue = hash_to_unit(hash_cell(0, 0, seed));
        let style = starfield_foreground_ansi(ColorPalette::Nebula, value, hue);

        let canvas = vec![vec![value, 0.0], vec![0.0; 2]];

        let terminal = Terminal::with_colors(true, true);
        let result = render_starfield(&canvas, true, &terminal, ColorPalette::Nebula);

        assert_eq!(result[0], format!("{}{}{} ", style, glyph, RESET));
    }
}
