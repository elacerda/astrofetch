use super::{
    hash::{hash_cell, hash_to_unit},
    RESET,
};
use crate::terminal::Terminal;

pub fn render_starfield(
    canvas: &[Vec<f64>],
    colors_enabled: bool,
    terminal: &Terminal,
) -> Vec<String> {
    let width = canvas.first().map_or(0, Vec::len);
    let mut lines = Vec::with_capacity((canvas.len() + 1) / 2);

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
                let color = starfield_to_ansi(value, x, y / 2);
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

fn starfield_to_ansi(value: f64, x: usize, y: usize) -> &'static str {
    let hue = hash_to_unit(hash_cell(x, y, 0x51a7_f17e_d00d_cafe));

    if value < 0.085 {
        "\x1b[2;37m" // faint gray
    } else if value < 0.150 {
        if hue < 0.50 {
            "\x1b[38;5;67m" // muted pale blue
        } else {
            "\x1b[38;5;109m" // muted pale cyan
        }
    } else if hue < 0.58 {
        "\x1b[38;5;250m" // soft white
    } else if hue < 0.84 {
        "\x1b[38;5;180m" // soft warm star
    } else {
        "\x1b[38;5;167m" // rare muted red star
    }
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
