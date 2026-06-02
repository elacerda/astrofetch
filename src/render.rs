use crate::terminal::Terminal;

const RESET: &str = "\x1b[0m";

/// Paleta de caracteres ASCII para renderização.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub chars: &'static [char],
}

impl Palette {
    /// Paleta padrão (do mais claro para mais escuro).
    /// O espaço ' ' é o nível mais baixo de intensidade para reduzir ruído visual.
    #[allow(dead_code)]
    pub const DEFAULT: Palette = Palette {
        chars: &[' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'],
    };

    /// Paleta alternativa mais detalhada.
    #[allow(dead_code)]
    pub const DETAILED: Palette = Palette {
        chars: &[
            ' ', '.', ',', ':', ';', 'i', 'r', 's', 'X', 'A', '2', '5', '3', 'h', 'M', 'H', 'G',
            'S', '#', '9', 'B', '&', '@',
        ],
    };

    /// Retorna o caractere correspondente ao nível de brilho (0.0 a 1.0).
    pub fn get_char(&self, value: f64) -> char {
        if value <= 0.0 {
            return self.chars[0];
        }
        if value >= 1.0 {
            return self.chars[self.chars.len() - 1];
        }

        let idx = (value * (self.chars.len() - 1) as f64).floor() as usize;
        self.chars[idx]
    }
}

/// Renderiza uma matriz de luminosidade em ASCII.
#[allow(dead_code)]
pub fn render_ascii(canvas: &[Vec<f64>], palette: &Palette) -> Vec<String> {
    canvas
        .iter()
        .map(|row| row.iter().map(|&v| palette.get_char(v)).collect::<String>())
        .collect()
}

/// Renderiza ASCII com cores ANSI.
#[allow(dead_code)]
pub fn render_colored_ascii(
    canvas: &[Vec<f64>],
    palette: &Palette,
    terminal: &Terminal,
) -> Vec<String> {
    canvas
        .iter()
        .map(|row| {
            row.iter()
                .map(|&v| {
                    let c = palette.get_char(v);
                    if terminal.colors_enabled() {
                        let color = intensity_to_ansi(v);
                        format!("{}{}{}", color, c, RESET)
                    } else {
                        c.to_string()
                    }
                })
                .collect::<String>()
        })
        .collect()
}

/// Renderiza o mapa de densidade usando half-block Unicode.
///
/// The input canvas is expected to have twice the requested terminal height.
/// Two density rows are collapsed into one terminal glyph row using:
/// - top only:    ▀
/// - bottom only: ▄
/// - both:        █
/// - none:        space
pub fn render_galaxy(
    canvas: &[Vec<f64>],
    colors_enabled: bool,
    terminal: &Terminal,
) -> Vec<String> {
    render_half_blocks(canvas, colors_enabled && terminal.colors_enabled())
}

fn render_half_blocks(canvas: &[Vec<f64>], colors_enabled: bool) -> Vec<String> {
    let adaptive = adaptive_threshold(canvas);

    // Shade-first renderer: favor diffuse luminosity over hard spiral strokes.
    let threshold = (adaptive * 0.42).clamp(0.035, 0.18);
    let star_seed = star_field_seed(canvas);

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

            let galaxy_ch = glyph_for_density_pair(top, bottom, threshold);

            if galaxy_ch == ' ' {
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
                let color = intensity_to_ansi((top + bottom) * 0.5);
                line.push_str(color);
                line.push(galaxy_ch);
                line.push_str(RESET);
            } else {
                line.push(galaxy_ch);
            }
        }

        lines.push(line);
    }

    lines
}

fn glyph_for_density_pair(top: f64, bottom: f64, threshold: f64) -> char {
    let maxv = top.max(bottom);
    if maxv < threshold {
        return ' ';
    }

    // A small max contribution preserves thin arms, while the average keeps
    // the result visually diffuse instead of line-like.
    let value = ((top + bottom) * 0.5).max(maxv * 0.62);
    shade_for_intensity(value, threshold)
}

fn shade_for_intensity(value: f64, threshold: f64) -> char {
    let normalized = ((value - threshold) / (1.0 - threshold)).clamp(0.0, 1.0);

    if normalized < 0.10 {
        '░'
    } else if normalized < 0.26 {
        '▒'
    } else if normalized < 0.48 {
        '▓'
    } else {
        '█'
    }
}

fn star_glyph_for_cell(
    x: usize,
    y: usize,
    top: f64,
    bottom: f64,
    threshold: f64,
    seed: u64,
) -> Option<char> {
    let local_density = top.max(bottom);

    // Do not draw background stars over visible galaxy structure.
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

fn star_field_seed(canvas: &[Vec<f64>]) -> u64 {
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

fn hash_cell(x: usize, y: usize, seed: u64) -> u64 {
    let mut value = seed;
    value ^= (x as u64).wrapping_mul(0x9e3779b97f4a7c15);
    value ^= (y as u64).wrapping_mul(0xbf58476d1ce4e5b9);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58476d1ce4e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn hash_to_unit(hash: u64) -> f64 {
    const SCALE: f64 = 1.0 / ((1_u64 << 53) as f64);
    ((hash >> 11) as f64) * SCALE
}

fn adaptive_threshold(canvas: &[Vec<f64>]) -> f64 {
    let mut values: Vec<f64> = canvas
        .iter()
        .flat_map(|row| row.iter().copied())
        .filter(|value| *value > 1.0e-6)
        .collect();

    if values.is_empty() {
        return 1.0;
    }

    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Keep only the brighter structure instead of filling the whole disk.
    // The clamp avoids the two bad extremes we observed:
    // - too low: solid ellipse
    // - too high: only the nucleus
    let percentile = 0.58;
    let idx = ((values.len().saturating_sub(1)) as f64 * percentile).round() as usize;

    values[idx].clamp(0.06, 0.28)
}

mod starfield;

pub use starfield::render_starfield;

/// Converte intensidade para cor ANSI.
fn intensity_to_ansi(value: f64) -> &'static str {
    if value < 0.16 {
        "\x1b[2;38;5;17m" // dim deep blue
    } else if value < 0.30 {
        "\x1b[2;38;5;24m" // dim blue
    } else if value < 0.44 {
        "\x1b[38;5;30m" // muted cyan/teal
    } else if value < 0.58 {
        "\x1b[38;5;65m" // muted green
    } else if value < 0.72 {
        "\x1b[38;5;136m" // muted amber
    } else if value < 0.88 {
        "\x1b[38;5;130m" // muted orange/red
    } else {
        "\x1b[38;5;255m" // soft white core
    }
}

/// Aplica stretch (contraste) no valor usando gamma.
/// Gamma < 1 aumenta contraste em valores baixos.
mod stretch;

pub use stretch::{
    apply_asinh_stretch, apply_gamma_stretch, apply_log_stretch, normalize_canvas,
    normalize_with_stretch, StretchType,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_get_char() {
        let palette = Palette::DEFAULT;
        assert_eq!(palette.get_char(0.0), ' ');
        assert_eq!(palette.get_char(1.0), '@');
    }

    #[test]
    fn test_palette_maps_low_to_space() {
        let palette = Palette::DEFAULT;
        assert_eq!(palette.chars[0], ' ');
    }

    #[test]
    fn test_render_ascii() {
        let canvas = vec![vec![0.0, 0.5, 1.0], vec![1.0, 0.5, 0.0]];
        let palette = Palette::DEFAULT;
        let result = render_ascii(&canvas, &palette);

        assert_eq!(result[0], " =@");
        assert_eq!(result[1], "@= ");
    }

    #[test]
    fn test_render_galaxy_shade_only_blocks() {
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let canvas = vec![vec![1.0, 0.0, 1.0], vec![0.0, 1.0, 1.0]];

        let result = render_galaxy(&canvas, false, &terminal);

        assert_eq!(result, vec!["███"]);
    }

    #[test]
    fn test_deterministic_render() {
        let model = crate::engine::ArtModel::Starfield;
        let canvas1 = model.generate(10, 5, Some(42));
        let canvas2 = model.generate(10, 5, Some(42));

        assert_eq!(canvas1, canvas2);
    }

    #[test]
    fn test_gamma_stretch() {
        assert!(apply_gamma_stretch(0.5, 0.6) > 0.5);
        assert_eq!(apply_gamma_stretch(0.0, 0.6), 0.0);
        assert_eq!(apply_gamma_stretch(1.0, 0.6), 1.0);
    }

    #[test]
    fn test_log_stretch() {
        assert!(apply_log_stretch(0.5, 10.0) > 0.5);
        assert_eq!(apply_log_stretch(0.0, 10.0), 0.0);
        assert_eq!(apply_log_stretch(1.0, 10.0), 1.0);
    }

    #[test]
    fn test_asinh_stretch() {
        assert!(apply_asinh_stretch(0.5, 2.0) > 0.5);
        assert_eq!(apply_asinh_stretch(0.0, 2.0), 0.0);
        assert_eq!(apply_asinh_stretch(1.0, 2.0), 1.0);
    }

    #[test]
    fn test_normalize_canvas() {
        let canvas = vec![vec![0.0, 0.5, 1.0], vec![1.0, 0.5, 0.0]];
        let normalized = normalize_canvas(&canvas);

        for row in &normalized {
            for &val in row {
                assert!((0.0..=1.0).contains(&val));
            }
        }
    }

    #[test]
    fn test_no_color_ansi_free() {
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let canvas = vec![vec![0.5]];
        let palette = Palette::DEFAULT;

        let result = render_colored_ascii(&canvas, &palette, &terminal);
        let line = &result[0];

        assert!(!line.contains('\x1b'));
        assert_eq!(line, "=");
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
    fn test_colored_contains_ansi() {
        let terminal = crate::terminal::Terminal::with_colors(true, true);
        let canvas = vec![vec![0.5]];
        let palette = Palette::DEFAULT;

        let result = render_colored_ascii(&canvas, &palette, &terminal);
        let line = &result[0];

        assert!(line.contains('\x1b'));
    }
}
