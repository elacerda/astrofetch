use crate::terminal::Terminal;

/// Paleta de caracteres ASCII para renderização.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub chars: &'static [char],
}

impl Palette {
    /// Paleta padrão (do mais claro para mais escuro).
    /// O espaço ' ' é o nível mais baixo de intensidade para reduzir ruído visual.
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
pub fn render_ascii(canvas: &[Vec<f64>], palette: &Palette) -> Vec<String> {
    canvas
        .iter()
        .map(|row| row.iter().map(|&v| palette.get_char(v)).collect::<String>())
        .collect()
}

/// Renderiza ASCII com cores ANSI.
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
                    // Aplica cor baseada na intensidade
                    if terminal.colors_enabled() {
                        let color = intensity_to_ansi(v);
                        format!("{}{}\x1b[0m", color, c)
                    } else {
                        c.to_string()
                    }
                })
                .collect::<String>()
        })
        .collect()
}

/// Converte intensidade para cor ANSI.
fn intensity_to_ansi(value: f64) -> &'static str {
    if value < 0.15 {
        "\x1b[90m" // Preto suave (dim)
    } else if value < 0.3 {
        "\x1b[34m" // Azul
    } else if value < 0.5 {
        "\x1b[36m" // Ciano
    } else if value < 0.7 {
        "\x1b[32m" // Verde
    } else if value < 0.85 {
        "\x1b[33m" // Amarelo
    } else if value < 0.95 {
        "\x1b[35m" // Magenta
    } else {
        "\x1b[37m" // Branco
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
    // log(base * value + 1) / log(base + 1)
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
    // asinh(value * scale) / asinh(scale)
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
        // Todos os valores são iguais
        return canvas
            .iter()
            .map(|row| row.iter().map(|_| 0.0).collect())
            .collect();
    }

    let range = max_val - min_val;
    canvas
        .iter()
        .map(|row| {
            row.iter()
                .map(|&v| (v - min_val) / range)
                .collect::<Vec<f64>>()
        })
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
        assert_eq!(palette.get_char(0.0), ' '); // Espaço é o primeiro caractere
        assert_eq!(palette.get_char(1.0), '@');
    }

    #[test]
    fn test_palette_maps_low_to_space() {
        let palette = Palette::DEFAULT;
        // O primeiro caractere é ' ', que deve ser usado para valores baixos
        assert_eq!(palette.chars[0], ' ');
    }

    #[test]
    fn test_render_ascii() {
        let canvas = vec![vec![0.0, 0.5, 1.0], vec![1.0, 0.5, 0.0]];
        let palette = Palette::DEFAULT;
        let result = render_ascii(&canvas, &palette);

        // Palette: [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@']
        // 0.0 -> ' ' (index 0)
        // 0.5 -> (0.5 * 9).floor() = 4 -> '='
        // 1.0 -> '@' (index 9)
        assert_eq!(result[0], " =@");
        assert_eq!(result[1], "@= ");
    }

    #[test]
    fn test_deterministic_render() {
        // Mesma seed deve produzir mesma saída
        let model = crate::engine::ArtModel::Starfield;
        let canvas1 = model.generate(10, 5, Some(42));
        let canvas2 = model.generate(10, 5, Some(42));

        assert_eq!(canvas1, canvas2);
    }

    #[test]
    fn test_gamma_stretch() {
        // Gamma < 1 aumenta contraste em valores baixos
        assert!(apply_gamma_stretch(0.5, 0.6) > 0.5);
        assert_eq!(apply_gamma_stretch(0.0, 0.6), 0.0);
        assert_eq!(apply_gamma_stretch(1.0, 0.6), 1.0);
    }

    #[test]
    fn test_log_stretch() {
        // Log stretch
        assert!(apply_log_stretch(0.5, 10.0) > 0.5);
        assert_eq!(apply_log_stretch(0.0, 10.0), 0.0);
        assert_eq!(apply_log_stretch(1.0, 10.0), 1.0);
    }

    #[test]
    fn test_asinh_stretch() {
        // Asinh stretch
        assert!(apply_asinh_stretch(0.5, 2.0) > 0.5);
        assert_eq!(apply_asinh_stretch(0.0, 2.0), 0.0);
        assert_eq!(apply_asinh_stretch(1.0, 2.0), 1.0);
    }

    #[test]
    fn test_normalize_canvas() {
        let canvas = vec![vec![0.0, 0.5, 1.0], vec![1.0, 0.5, 0.0]];
        let normalized = normalize_canvas(&canvas);

        // Valores devem estar entre 0 e 1
        for row in &normalized {
            for &val in row {
                assert!(val >= 0.0);
                assert!(val <= 1.0);
            }
        }
    }

    #[test]
    fn test_no_color_ansi_free() {
        // Quando colors_enabled é false, não deve haver códigos ANSI
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let canvas = vec![vec![0.5]];
        let palette = Palette::DEFAULT;

        let result = render_colored_ascii(&canvas, &palette, &terminal);
        let line = &result[0];

        // Não deve conter códigos ANSI
        assert!(!line.contains('\x1b'));
        assert_eq!(line, "=");
    }

    #[test]
    fn test_colored_contains_ansi() {
        // Quando colors_enabled é true, deve haver códigos ANSI
        let terminal = crate::terminal::Terminal::with_colors(true, true);
        let canvas = vec![vec![0.5]];
        let palette = Palette::DEFAULT;

        let result = render_colored_ascii(&canvas, &palette, &terminal);
        let line = &result[0];

        // Deve conter códigos ANSI
        assert!(line.contains('\x1b'));
    }
}
