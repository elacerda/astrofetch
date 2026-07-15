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

/// Converte intensidade para cor ANSI de fundo.
///
/// Usa os mesmos índices da paleta que `intensity_to_ansi`, mas emite
/// sequências de fundo usando `48;5;<index>m`.
///
/// Para entradas da paleta que incluem o atributo dim no formato de primeiro plano,
/// o helper de fundo usa `48;5;<index>m` sem o atributo dim global,
/// pois eles também podem afetar a renderização do primeiro plano.
pub(super) fn intensity_to_background_ansi(value: f64) -> &'static str {
    if value < 0.16 {
        "\x1b[48;5;17m" // deep blue background
    } else if value < 0.30 {
        "\x1b[48;5;24m" // blue background
    } else if value < 0.44 {
        "\x1b[48;5;30m" // muted cyan/teal background
    } else if value < 0.58 {
        "\x1b[48;5;65m" // muted green background
    } else if value < 0.72 {
        "\x1b[48;5;136m" // muted amber background
    } else if value < 0.88 {
        "\x1b[48;5;130m" // muted orange/red background
    } else {
        "\x1b[48;5;255m" // soft white background
    }
}
