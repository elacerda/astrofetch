use crossterm::{queue, style::Print};
use is_terminal::IsTerminal;
use std::env;
use std::io::{self, Write};

const RESET: &str = "\x1b[0m";

/// Estrutura para gerenciar configurações do terminal.
#[derive(Debug, Clone)]
pub struct Terminal {
    /// Se a saída é um TTY
    pub is_tty: bool,
    /// Se cores ANSI estão habilitadas
    pub colors_enabled: bool,
}

impl Terminal {
    /// Cria uma nova instância do Terminal detectando o ambiente.
    pub fn new() -> Self {
        let is_tty = std::io::stdout().is_terminal();
        let no_color = env::var("NO_COLOR").is_ok();
        let colors_enabled = is_tty && !no_color;

        Self {
            is_tty,
            colors_enabled,
        }
    }

    /// Cria uma nova instância com configuração explícita de cores.
    #[allow(dead_code)]
    pub fn with_colors(is_tty: bool, colors_enabled: bool) -> Self {
        Self {
            is_tty,
            colors_enabled,
        }
    }

    /// Retorna true se a saída é um TTY.
    #[allow(dead_code)]
    pub fn is_tty(&self) -> bool {
        self.is_tty
    }

    /// Retorna true se cores ANSI estão habilitadas.
    pub fn colors_enabled(&self) -> bool {
        self.colors_enabled
    }

    /// Prints lines using crossterm's cross-platform command queue.
    pub fn print_lines(&self, lines: &[String]) -> io::Result<()> {
        let mut stdout = io::stdout();

        for line in lines {
            queue!(stdout, Print(line), Print("\n"))?;
        }

        stdout.flush()
    }

    /// Aplica cor ANSI se estiver habilitada, caso contrário retorna a string original.
    #[allow(dead_code)]
    pub fn colorize<S: AsRef<str>>(&self, text: S, color: &str) -> String {
        if self.colors_enabled {
            format!("{}{}{}", color, text.as_ref(), RESET)
        } else {
            text.as_ref().to_string()
        }
    }
}

/// Calcula a largura visual de uma string, ignorando códigos ANSI.
///
/// Usa `unicode_width` para caracteres Unicode e remove códigos ANSI.
pub fn visible_width(s: &str) -> usize {
    let without_ansi = remove_ansi_codes(s);
    unicode_width::UnicodeWidthStr::width(without_ansi.as_str())
}

/// Remove códigos ANSI de uma string.
fn remove_ansi_codes(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            while let Some(&next) = chars.peek() {
                if next.is_ascii_uppercase() || next.is_ascii_lowercase() || next == 'm' {
                    chars.next();
                    break;
                }
                if next.is_ascii_digit() || next == ';' || next == '[' {
                    chars.next();
                    continue;
                }
                break;
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visible_width_ascii() {
        assert_eq!(visible_width("hello"), 5);
        assert_eq!(visible_width(""), 0);
        assert_eq!(visible_width("a b c"), 5);
    }

    #[test]
    fn test_visible_width_ansi() {
        let red = "\x1b[31m";
        let reset = "\x1b[0m";

        assert_eq!(visible_width(&format!("{}hello{}", red, reset)), 5);
        assert_eq!(visible_width(&format!("{}red{}", red, reset)), 3);
    }

    #[test]
    fn test_visible_width_unicode() {
        assert_eq!(visible_width("hello"), 5);
        assert_eq!(visible_width("▀▄█"), 3);
    }

    #[test]
    fn test_remove_ansi_codes() {
        let input = "\x1b[31mred\x1b[0m and \x1b[1mbold\x1b[0m";
        let expected = "red and bold";
        assert_eq!(remove_ansi_codes(input), expected);
    }

    #[test]
    fn test_visible_width_complex_ansi() {
        let input = "\x1b[1;31mred\x1b[0m and \x1b[2;32mgreen\x1b[0m";
        assert_eq!(visible_width(input), 13);
    }

    #[test]
    fn test_visible_width_empty() {
        assert_eq!(visible_width(""), 0);
        assert_eq!(visible_width("\x1b[0m"), 0);
    }
}
