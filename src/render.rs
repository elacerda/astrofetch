use crate::terminal::Terminal;

/// Paleta de caracteres ASCII para renderização.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub chars: &'static [char],
}

impl Palette {
    /// Paleta padrão (do mais claro para mais escuro).
    pub const DEFAULT: Palette = Palette {
        chars: &['.', ':', '-', '=', '+', '*', '#', '%', '@'],
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
#[allow(dead_code)]
fn intensity_to_ansi(value: f64) -> &'static str {
    if value < 0.2 {
        "\x1b[90m" // Preto suave
    } else if value < 0.4 {
        "\x1b[34m" // Azul
    } else if value < 0.6 {
        "\x1b[36m" // Ciano
    } else if value < 0.8 {
        "\x1b[32m" // Verde
    } else if value < 0.9 {
        "\x1b[33m" // Amarelo
    } else {
        "\x1b[37m" // Branco
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ArtModel;

    #[test]
    fn test_palette_get_char() {
        let palette = Palette::DEFAULT;
        assert_eq!(palette.get_char(0.0), '.');
        assert_eq!(palette.get_char(1.0), '@');
    }

    #[test]
    fn test_render_ascii() {
        let canvas = vec![vec![0.0, 0.5, 1.0], vec![1.0, 0.5, 0.0]];
        let palette = Palette::DEFAULT;
        let result = render_ascii(&canvas, &palette);

        // Palette: ['.', ':', '-', '=', '+', '*', '#', '%', '@']
        // 0.0 -> '.' (index 0)
        // 0.5 -> (0.5 * 8).floor() = 4 -> '+'
        // 1.0 -> '@' (index 8)
        assert_eq!(result[0], ".+@");
        assert_eq!(result[1], "@+.");
    }

    #[test]
    fn test_deterministic_render() {
        // Mesma seed deve produzir mesma saída
        let model = ArtModel::Starfield;
        let canvas1 = model.generate(10, 5, Some(42));
        let canvas2 = model.generate(10, 5, Some(42));

        assert_eq!(canvas1, canvas2);
    }
}
