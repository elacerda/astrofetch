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

    // Lower than the structural threshold because the glyph palette below
    // can represent diffuse light without filling everything with solid blocks.
    let threshold = (adaptive * 0.55).clamp(0.05, 0.24);

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

            let ch = glyph_for_density_pair(top, bottom, threshold);

            if colors_enabled && ch != ' ' {
                let color = intensity_to_ansi(top.max(bottom));
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

fn glyph_for_density_pair(top: f64, bottom: f64, threshold: f64) -> char {
    let top_on = top >= threshold;
    let bottom_on = bottom >= threshold;

    match (top_on, bottom_on) {
        (false, false) => ' ',
        (true, true) => shade_for_intensity((top + bottom) * 0.5, threshold),
        (true, false) => half_or_diffuse('▀', top, threshold),
        (false, true) => half_or_diffuse('▄', bottom, threshold),
    }
}

fn half_or_diffuse(half_block: char, value: f64, threshold: f64) -> char {
    let normalized = ((value - threshold) / (1.0 - threshold)).clamp(0.0, 1.0);

    // Very faint one-sided pixels should look like diffuse glow, not hard strokes.
    if normalized < 0.22 {
        '░'
    } else {
        half_block
    }
}

fn shade_for_intensity(value: f64, threshold: f64) -> char {
    let normalized = ((value - threshold) / (1.0 - threshold)).clamp(0.0, 1.0);

    if normalized < 0.18 {
        '░'
    } else if normalized < 0.42 {
        '▒'
    } else if normalized < 0.68 {
        '▓'
    } else {
        '█'
    }
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

    values[idx].clamp(0.10, 0.42)
}

/// Converte intensidade para cor ANSI.
fn intensity_to_ansi(value: f64) -> &'static str {
    if value < 0.20 {
        "\x1b[90m" // Preto suave (dim)
    } else if value < 0.38 {
        "\x1b[34m" // Azul
    } else if value < 0.58 {
        "\x1b[36m" // Ciano
    } else if value < 0.78 {
        "\x1b[33m" // Amarelo
    } else {
        "\x1b[97m" // Branco brilhante
    }
}

/// Aplica stretch (contraste) no valor usando gamma.
/// Gamma < 1 aumenta contraste em valores baixos.
pub fn apply_gamma_stretch(value: f64, gamma: f64) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }
    if value >= 1.0 {
        return 1.0;
    }
    value.powf(gamma)
}

/// Aplica stretch logarítmico.
pub fn apply_log_stretch(value: f64, base: f64) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }
    if value >= 1.0 {
        return 1.0;
    }
    (value * base + 1.0).log(base + 1.0)
}

/// Aplica stretch asinh (arco-seno-hiperbólico).
pub fn apply_asinh_stretch(value: f64, scale: f64) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }
    if value >= 1.0 {
        return 1.0;
    }
    (value * scale).asinh() / scale.asinh()
}

/// Normaliza valores de um canvas para o intervalo [0, 1].
pub fn normalize_canvas(canvas: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let mut min_val = f64::INFINITY;
    let mut max_val = f64::NEG_INFINITY;

    for row in canvas.iter() {
        for &val in row.iter() {
            if val < min_val {
                min_val = val;
            }
            if val > max_val {
                max_val = val;
            }
        }
    }

    if (max_val - min_val).abs() < f64::EPSILON {
        return canvas
            .iter()
            .map(|row| row.iter().map(|_| 0.0).collect())
            .collect();
    }

    let range = max_val - min_val;
    canvas
        .iter()
        .map(|row| row.iter().map(|&v| (v - min_val) / range).collect())
        .collect()
}

/// Normaliza e aplica stretch ao canvas.
pub fn normalize_with_stretch(canvas: &[Vec<f64>], stretch: StretchType) -> Vec<Vec<f64>> {
    let normalized = normalize_canvas(canvas);
    apply_stretch(&normalized, stretch)
}

/// Tipos de stretch disponíveis.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum StretchType {
    /// Sem stretch (linear)
    None,
    /// Gamma stretch
    Gamma(f64),
    /// Log stretch
    Log(f64),
    /// Asinh stretch
    Asinh(f64),
}

impl Default for StretchType {
    fn default() -> Self {
        StretchType::Gamma(0.6)
    }
}

/// Aplica stretch a um canvas normalizado.
pub fn apply_stretch(canvas: &[Vec<f64>], stretch: StretchType) -> Vec<Vec<f64>> {
    match stretch {
        StretchType::None => canvas.to_vec(),
        StretchType::Gamma(gamma) => canvas
            .iter()
            .map(|row| row.iter().map(|&v| apply_gamma_stretch(v, gamma)).collect())
            .collect(),
        StretchType::Log(base) => canvas
            .iter()
            .map(|row| row.iter().map(|&v| apply_log_stretch(v, base)).collect())
            .collect(),
        StretchType::Asinh(scale) => canvas
            .iter()
            .map(|row| row.iter().map(|&v| apply_asinh_stretch(v, scale)).collect())
            .collect(),
    }
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
    fn test_render_galaxy_half_blocks() {
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let canvas = vec![vec![1.0, 0.0, 1.0], vec![0.0, 1.0, 1.0]];

        let result = render_galaxy(&canvas, false, &terminal);

        assert_eq!(result, vec!["▀▄█"]);
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
    fn test_colored_contains_ansi() {
        let terminal = crate::terminal::Terminal::with_colors(true, true);
        let canvas = vec![vec![0.5]];
        let palette = Palette::DEFAULT;

        let result = render_colored_ascii(&canvas, &palette, &terminal);
        let line = &result[0];

        assert!(line.contains('\x1b'));
    }
}
