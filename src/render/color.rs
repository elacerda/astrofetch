pub(super) const RESET: &str = "\x1b[0m";

/// Converte intensidade para cor ANSI.
pub(super) fn intensity_to_ansi(value: f64) -> &'static str {
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
