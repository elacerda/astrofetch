use super::{
    color::{starfield_foreground_ansi, ColorPalette, RESET},
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
        let mut line = String::with_capacity(width);

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
                line.push_str(color);
                line.push(ch);
                line.push_str(RESET);
            } else {
                line.push(ch);
            }
        }

        lines.push(line);
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
