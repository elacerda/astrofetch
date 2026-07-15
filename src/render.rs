use crate::terminal::Terminal;

mod color;
mod hash;
mod profile;
mod starfield;
mod stretch;

pub use profile::{prepare_density, PreparedDensity, RenderProfile};
pub use starfield::render_starfield;

use color::{intensity_to_ansi, RESET};
use hash::{hash_cell, hash_to_unit};

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

/// Renderiza o mapa de densidade usando sombreamento Unicode.
///
/// This is the main renderer for galaxy-like models. It consumes pairs of density
/// rows and converts them to one terminal row using shaded block characters.
///
/// The renderer:
/// - Takes two vertical density samples per terminal row
/// - Uses their maximum for visibility threshold
/// - Uses their average for intensity
/// - Emits ░▒▓█ glyphs based on intensity
///
/// This is NOT a true half-block renderer. True ▀/▄ rendering belongs to Patch 3.
pub fn render_shades(canvas: &[Vec<f64>], threshold: f64, colors_enabled: bool) -> Vec<String> {
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
    if !maxv.is_finite() || maxv <= 0.0 || maxv < threshold {
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
    fn test_deterministic_render() {
        let model = crate::engine::ArtModel::Starfield;
        let canvas1 = model.generate_scene(10, 5, Some(42)).density.into_rows();
        let canvas2 = model.generate_scene(10, 5, Some(42)).density.into_rows();

        assert_eq!(canvas1, canvas2);
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

    #[test]
    fn test_render_shades_with_precomputed_threshold() {
        let canvas = vec![vec![1.0, 0.0, 1.0], vec![0.0, 1.0, 1.0]];

        // Use a low threshold so all cells are visible
        let result = render_shades(&canvas, 0.0, false);

        assert_eq!(result, vec!["███"]);
    }

    #[test]
    fn test_render_shades_zero_map_is_empty() {
        // Create a canvas with at least two density rows, all zeros
        let canvas = vec![vec![0.0, 0.0, 0.0, 0.0], vec![0.0, 0.0, 0.0, 0.0]];

        // With any threshold, zero-density cells should render as spaces
        let result = render_shades(&canvas, 0.0, false);

        // Each terminal row should contain only spaces (no galaxy glyphs)
        assert_eq!(result.len(), 1);
        assert!(result[0].chars().all(|c| c == ' '));
    }
}
