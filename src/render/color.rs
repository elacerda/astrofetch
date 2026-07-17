/// Internal color palette abstraction for procedural art.
///
/// This module provides the color-palette foundation for Patch 3C-A.
/// The `Nebula` palette represents the current AstroFetch ANSI color behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorPalette {
    /// The standard AstroFetch color scheme - preserves current ANSI behavior.
    Nebula,
    /// Color-vision-friendly xterm-256 ramp inspired by Cividis.
    Cividis,
    /// Warm monochromatic-style ramp.
    Amber,
    /// xterm-256 grayscale ramp.
    Mono,
}

pub(super) const RESET: &str = "\x1b[0m";

/// Map a density value to an intensity level index (0-6).
///
/// The thresholds define 7 intensity bands:
/// - level 0: value < 0.16
/// - level 1: value < 0.30
/// - level 2: value < 0.44
/// - level 3: value < 0.58
/// - level 4: value < 0.72
/// - level 5: value < 0.88
/// - level 6: value >= 0.88
fn value_to_level(value: f64) -> usize {
    if value < 0.16 {
        0
    } else if value < 0.30 {
        1
    } else if value < 0.44 {
        2
    } else if value < 0.58 {
        3
    } else if value < 0.72 {
        4
    } else if value < 0.88 {
        5
    } else {
        6
    }
}

/// Galaxies use foreground color based on density value.
///
/// For `ColorPalette::Nebula`, the thresholds and ANSI sequences preserve
/// the current behavior exactly. For other palettes, different xterm-256
/// color indices are used for each intensity level.
pub(super) fn galaxy_foreground_ansi(palette: ColorPalette, value: f64) -> &'static str {
    let level = value_to_level(value);
    match palette {
        ColorPalette::Nebula => galaxy_foreground_nebula(level),
        ColorPalette::Cividis => galaxy_foreground_cividis(level),
        ColorPalette::Amber => galaxy_foreground_amber(level),
        ColorPalette::Mono => galaxy_foreground_mono(level),
    }
}

/// Galaxies use background color based on density value.
///
/// For `ColorPalette::Nebula`, uses the same indices as foreground but with
/// background ANSI sequence (`48;5;<index>m`) without dim attribute.
/// For other palettes, corresponding background indices are used.
pub(super) fn galaxy_background_ansi(palette: ColorPalette, value: f64) -> &'static str {
    let level = value_to_level(value);
    match palette {
        ColorPalette::Nebula => galaxy_background_nebula(level),
        ColorPalette::Cividis => galaxy_background_cividis(level),
        ColorPalette::Amber => galaxy_background_amber(level),
        ColorPalette::Mono => galaxy_background_mono(level),
    }
}

/// Starfield uses foreground color based on density value and deterministic hue.
///
/// The hue is calculated deterministically from the cell coordinates and a fixed seed.
/// Different palettes use different ANSI sequences for each brightness category.
pub(super) fn starfield_foreground_ansi(
    palette: ColorPalette,
    value: f64,
    hue: f64,
) -> &'static str {
    // Brightness thresholds for starfield
    let is_faint = value < 0.085;
    let is_medium = (0.085..0.150).contains(&value);

    match (is_faint, is_medium, palette) {
        // Faint stars (value < 0.085) - always gray
        (true, _, _) => starfield_faint_ansi(palette),

        // Medium stars (0.085 <= value < 0.150) - hue-dependent
        (false, true, ColorPalette::Nebula) => {
            if hue < 0.50 {
                "\x1b[38;5;67m" // muted pale blue
            } else {
                "\x1b[38;5;109m" // muted pale cyan
            }
        }
        (false, true, ColorPalette::Cividis) => {
            if hue < 0.50 {
                "\x1b[38;5;24m" // dark blue
            } else {
                "\x1b[38;5;66m" // dim green-cyan
            }
        }
        (false, true, ColorPalette::Amber) => {
            if hue < 0.50 {
                "\x1b[38;5;94m" // medium orange (Amber medium low)
            } else {
                "\x1b[38;5;130m" // medium orange/high
            }
        }
        (false, true, ColorPalette::Mono) => {
            if hue < 0.50 {
                "\x1b[38;5;245m" // medium-light gray (Mono medium low)
            } else {
                "\x1b[38;5;248m" // light gray
            }
        }

        // Bright stars (value >= 0.150) - hue-dependent
        (false, false, ColorPalette::Nebula) => {
            if hue < 0.58 {
                "\x1b[38;5;250m" // soft white
            } else if hue < 0.84 {
                "\x1b[38;5;180m" // soft warm star
            } else {
                "\x1b[38;5;167m" // rare muted red star
            }
        }
        (false, false, ColorPalette::Cividis) => {
            if hue < 0.58 {
                "\x1b[38;5;252m" // bright white
            } else if hue < 0.84 {
                "\x1b[38;5;186m" // warm yellow
            } else {
                "\x1b[38;5;228m" // light yellow-white
            }
        }
        (false, false, ColorPalette::Amber) => {
            if hue < 0.58 {
                "\x1b[38;5;214m" // bright orange
            } else if hue < 0.84 {
                "\x1b[38;5;220m" // yellow-orange
            } else {
                "\x1b[38;5;230m" // light yellow
            }
        }
        (false, false, ColorPalette::Mono) => {
            if hue < 0.58 {
                "\x1b[38;5;250m" // bright gray
            } else if hue < 0.84 {
                "\x1b[38;5;253m" // very light gray
            } else {
                "\x1b[38;5;255m" // near white
            }
        }
    }
}

// ===== Nebula palette ANSI sequences =====

fn galaxy_foreground_nebula(level: usize) -> &'static str {
    match level {
        0 => "\x1b[2;38;5;17m", // dim deep blue
        1 => "\x1b[2;38;5;24m", // dim blue
        2 => "\x1b[38;5;30m",   // muted cyan/teal
        3 => "\x1b[38;5;65m",   // muted green
        4 => "\x1b[38;5;136m",  // muted amber
        5 => "\x1b[38;5;130m",  // muted orange/red
        6 => "\x1b[38;5;255m",  // soft white core
        _ => "\x1b[38;5;255m",  // fallback to white
    }
}

fn galaxy_background_nebula(level: usize) -> &'static str {
    match level {
        0 => "\x1b[48;5;17m",  // deep blue background
        1 => "\x1b[48;5;24m",  // blue background
        2 => "\x1b[48;5;30m",  // muted cyan/teal background
        3 => "\x1b[48;5;65m",  // muted green background
        4 => "\x1b[48;5;136m", // muted amber background
        5 => "\x1b[48;5;130m", // muted orange/red background
        6 => "\x1b[48;5;255m", // soft white background
        _ => "\x1b[48;5;255m", // fallback to white
    }
}

fn starfield_faint_nebula() -> &'static str {
    "\x1b[2;37m" // faint gray
}

// ===== Cividis palette ANSI sequences =====

fn galaxy_foreground_cividis(level: usize) -> &'static str {
    match level {
        0 => "\x1b[2;38;5;17m", // dim deep blue
        1 => "\x1b[2;38;5;24m", // dim blue
        2 => "\x1b[38;5;60m",   // blue-cyan
        3 => "\x1b[38;5;66m",   // green-cyan
        4 => "\x1b[38;5;101m",  // cyan-blue
        5 => "\x1b[38;5;143m",  // green-yellow
        6 => "\x1b[38;5;228m",  // light yellow
        _ => "\x1b[38;5;228m",  // fallback
    }
}

fn galaxy_background_cividis(level: usize) -> &'static str {
    match level {
        0 => "\x1b[48;5;17m",  // deep blue background
        1 => "\x1b[48;5;24m",  // blue background
        2 => "\x1b[48;5;60m",  // blue-cyan background
        3 => "\x1b[48;5;66m",  // green-cyan background
        4 => "\x1b[48;5;101m", // cyan-blue background
        5 => "\x1b[48;5;143m", // green-yellow background
        6 => "\x1b[48;5;228m", // light yellow background
        _ => "\x1b[48;5;228m", // fallback
    }
}

fn starfield_faint_cividis() -> &'static str {
    "\x1b[2;38;5;240m" // dim gray
}

// ===== Amber palette ANSI sequences =====

fn galaxy_foreground_amber(level: usize) -> &'static str {
    match level {
        0 => "\x1b[2;38;5;52m", // dim red-orange
        1 => "\x1b[2;38;5;88m", // medium orange
        2 => "\x1b[38;5;130m",  // orange-red
        3 => "\x1b[38;5;166m",  // bright orange
        4 => "\x1b[38;5;202m",  // vivid orange
        5 => "\x1b[38;5;214m",  // bright yellow-orange
        6 => "\x1b[38;5;230m",  // light yellow
        _ => "\x1b[38;5;230m",  // fallback
    }
}

fn galaxy_background_amber(level: usize) -> &'static str {
    match level {
        0 => "\x1b[48;5;52m",  // dim red-orange background
        1 => "\x1b[48;5;88m",  // medium orange background
        2 => "\x1b[48;5;130m", // orange-red background
        3 => "\x1b[48;5;166m", // bright orange background
        4 => "\x1b[48;5;202m", // vivid orange background
        5 => "\x1b[48;5;214m", // bright yellow-orange background
        6 => "\x1b[48;5;230m", // light yellow background
        _ => "\x1b[48;5;230m", // fallback
    }
}

fn starfield_faint_amber() -> &'static str {
    "\x1b[2;38;5;239m" // dim gray
}

// ===== Mono (grayscale) palette ANSI sequences =====

fn galaxy_foreground_mono(level: usize) -> &'static str {
    match level {
        0 => "\x1b[2;38;5;236m", // very dark gray
        1 => "\x1b[2;38;5;239m", // dark gray
        2 => "\x1b[38;5;242m",   // medium-dark gray
        3 => "\x1b[38;5;245m",   // medium-light gray
        4 => "\x1b[38;5;248m",   // light gray
        5 => "\x1b[38;5;252m",   // very light gray
        6 => "\x1b[38;5;255m",   // near white
        _ => "\x1b[38;5;255m",   // fallback
    }
}

fn galaxy_background_mono(level: usize) -> &'static str {
    match level {
        0 => "\x1b[48;5;236m", // very dark gray background
        1 => "\x1b[48;5;239m", // dark gray background
        2 => "\x1b[48;5;242m", // medium-dark gray background
        3 => "\x1b[48;5;245m", // medium-light gray background
        4 => "\x1b[48;5;248m", // light gray background
        5 => "\x1b[48;5;252m", // very light gray background
        6 => "\x1b[48;5;255m", // near white background
        _ => "\x1b[48;5;255m", // fallback
    }
}

fn starfield_faint_mono() -> &'static str {
    "\x1b[2;38;5;240m" // dim gray
}

// ===== Helper for starfield faint color =====

fn starfield_faint_ansi(palette: ColorPalette) -> &'static str {
    match palette {
        ColorPalette::Nebula => starfield_faint_nebula(),
        ColorPalette::Cividis => starfield_faint_cividis(),
        ColorPalette::Amber => starfield_faint_amber(),
        ColorPalette::Mono => starfield_faint_mono(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== ColorPalette enum tests =====

    #[test]
    fn test_color_palette_is_copy() {
        let palette = ColorPalette::Nebula;
        let cloned = palette;
        assert_eq!(palette, cloned);
    }

    #[test]
    fn test_color_palette_is_eq() {
        assert_eq!(ColorPalette::Nebula, ColorPalette::Nebula);
        assert_eq!(ColorPalette::Cividis, ColorPalette::Cividis);
        assert_eq!(ColorPalette::Amber, ColorPalette::Amber);
        assert_eq!(ColorPalette::Mono, ColorPalette::Mono);
    }

    // ===== Galaxy foreground color tests =====
    // Test exact output at representative boundaries for all palettes

    #[test]
    fn test_galaxy_foreground_ansi_nebula_at_boundaries() {
        // Nebula thresholds: 0.16, 0.30, 0.44, 0.58, 0.72, 0.88
        let tests = [
            (0.00, "\x1b[2;38;5;17m"),  // < 0.16 (level 0)
            (0.159, "\x1b[2;38;5;17m"), // < 0.16 (level 0)
            (0.16, "\x1b[2;38;5;24m"),  // < 0.30 (level 1)
            (0.299, "\x1b[2;38;5;24m"), // < 0.30 (level 1)
            (0.30, "\x1b[38;5;30m"),    // < 0.44 (level 2)
            (0.439, "\x1b[38;5;30m"),   // < 0.44 (level 2)
            (0.44, "\x1b[38;5;65m"),    // < 0.58 (level 3)
            (0.579, "\x1b[38;5;65m"),   // < 0.58 (level 3)
            (0.58, "\x1b[38;5;136m"),   // < 0.72 (level 4)
            (0.719, "\x1b[38;5;136m"),  // < 0.72 (level 4)
            (0.72, "\x1b[38;5;130m"),   // < 0.88 (level 5)
            (0.879, "\x1b[38;5;130m"),  // < 0.88 (level 5)
            (0.88, "\x1b[38;5;255m"),   // >= 0.88 (level 6)
            (1.00, "\x1b[38;5;255m"),   // >= 0.88 (level 6)
        ];

        for (value, expected) in tests {
            let actual = galaxy_foreground_ansi(ColorPalette::Nebula, value);
            assert_eq!(
                actual, expected,
                "galaxy_foreground_ansi(Nebula, {}) should be {}, got {}",
                value, expected, actual
            );
        }
    }

    #[test]
    fn test_galaxy_foreground_ansi_cividis_at_boundaries() {
        // Cividis uses same thresholds, different indices
        let tests = [
            (0.00, "\x1b[2;38;5;17m"),
            (0.16, "\x1b[2;38;5;24m"),
            (0.30, "\x1b[38;5;60m"),
            (0.44, "\x1b[38;5;66m"),
            (0.58, "\x1b[38;5;101m"),
            (0.72, "\x1b[38;5;143m"),
            (0.88, "\x1b[38;5;228m"),
        ];

        for (value, expected) in tests {
            let actual = galaxy_foreground_ansi(ColorPalette::Cividis, value);
            assert_eq!(
                actual, expected,
                "galaxy_foreground_ansi(Cividis, {}) should be {}, got {}",
                value, expected, actual
            );
        }
    }

    #[test]
    fn test_galaxy_foreground_ansi_amber_at_boundaries() {
        // Amber uses same thresholds, different indices
        let tests = [
            (0.00, "\x1b[2;38;5;52m"),
            (0.16, "\x1b[2;38;5;88m"),
            (0.30, "\x1b[38;5;130m"),
            (0.44, "\x1b[38;5;166m"),
            (0.58, "\x1b[38;5;202m"),
            (0.72, "\x1b[38;5;214m"),
            (0.88, "\x1b[38;5;230m"),
        ];

        for (value, expected) in tests {
            let actual = galaxy_foreground_ansi(ColorPalette::Amber, value);
            assert_eq!(
                actual, expected,
                "galaxy_foreground_ansi(Amber, {}) should be {}, got {}",
                value, expected, actual
            );
        }
    }

    #[test]
    fn test_galaxy_foreground_ansi_mono_at_boundaries() {
        // Mono (grayscale) uses same thresholds, different indices
        let tests = [
            (0.00, "\x1b[2;38;5;236m"),
            (0.16, "\x1b[2;38;5;239m"),
            (0.30, "\x1b[38;5;242m"),
            (0.44, "\x1b[38;5;245m"),
            (0.58, "\x1b[38;5;248m"),
            (0.72, "\x1b[38;5;252m"),
            (0.88, "\x1b[38;5;255m"),
        ];

        for (value, expected) in tests {
            let actual = galaxy_foreground_ansi(ColorPalette::Mono, value);
            assert_eq!(
                actual, expected,
                "galaxy_foreground_ansi(Mono, {}) should be {}, got {}",
                value, expected, actual
            );
        }
    }

    // ===== Galaxy background color tests =====

    #[test]
    fn test_galaxy_background_ansi_nebula_at_boundaries() {
        let tests = [
            (0.00, "\x1b[48;5;17m"),
            (0.159, "\x1b[48;5;17m"),
            (0.16, "\x1b[48;5;24m"),
            (0.299, "\x1b[48;5;24m"),
            (0.30, "\x1b[48;5;30m"),
            (0.439, "\x1b[48;5;30m"),
            (0.44, "\x1b[48;5;65m"),
            (0.579, "\x1b[48;5;65m"),
            (0.58, "\x1b[48;5;136m"),
            (0.719, "\x1b[48;5;136m"),
            (0.72, "\x1b[48;5;130m"),
            (0.879, "\x1b[48;5;130m"),
            (0.88, "\x1b[48;5;255m"),
            (1.00, "\x1b[48;5;255m"),
        ];

        for (value, expected) in tests {
            let actual = galaxy_background_ansi(ColorPalette::Nebula, value);
            assert_eq!(
                actual, expected,
                "galaxy_background_ansi(Nebula, {}) should be {}, got {}",
                value, expected, actual
            );
        }
    }

    #[test]
    fn test_galaxy_background_ansi_cividis_at_boundaries() {
        let tests = [
            (0.00, "\x1b[48;5;17m"),
            (0.16, "\x1b[48;5;24m"),
            (0.30, "\x1b[48;5;60m"),
            (0.44, "\x1b[48;5;66m"),
            (0.58, "\x1b[48;5;101m"),
            (0.72, "\x1b[48;5;143m"),
            (0.88, "\x1b[48;5;228m"),
        ];

        for (value, expected) in tests {
            let actual = galaxy_background_ansi(ColorPalette::Cividis, value);
            assert_eq!(
                actual, expected,
                "galaxy_background_ansi(Cividis, {}) should be {}, got {}",
                value, expected, actual
            );
        }
    }

    #[test]
    fn test_galaxy_background_ansi_amber_at_boundaries() {
        let tests = [
            (0.00, "\x1b[48;5;52m"),
            (0.16, "\x1b[48;5;88m"),
            (0.30, "\x1b[48;5;130m"),
            (0.44, "\x1b[48;5;166m"),
            (0.58, "\x1b[48;5;202m"),
            (0.72, "\x1b[48;5;214m"),
            (0.88, "\x1b[48;5;230m"),
        ];

        for (value, expected) in tests {
            let actual = galaxy_background_ansi(ColorPalette::Amber, value);
            assert_eq!(
                actual, expected,
                "galaxy_background_ansi(Amber, {}) should be {}, got {}",
                value, expected, actual
            );
        }
    }

    #[test]
    fn test_galaxy_background_ansi_mono_at_boundaries() {
        let tests = [
            (0.00, "\x1b[48;5;236m"),
            (0.16, "\x1b[48;5;239m"),
            (0.30, "\x1b[48;5;242m"),
            (0.44, "\x1b[48;5;245m"),
            (0.58, "\x1b[48;5;248m"),
            (0.72, "\x1b[48;5;252m"),
            (0.88, "\x1b[48;5;255m"),
        ];

        for (value, expected) in tests {
            let actual = galaxy_background_ansi(ColorPalette::Mono, value);
            assert_eq!(
                actual, expected,
                "galaxy_background_ansi(Mono, {}) should be {}, got {}",
                value, expected, actual
            );
        }
    }

    // ===== Starfield foreground color tests =====

    /// Table-driven test for starfield colors across all palettes
    #[test]
    fn test_starfield_foreground_ansi_table_driven() {
        // Test cases: (palette, value, hue, expected_ansicolor)
        let tests = [
            // Faint stars (value < 0.085) - always gray
            (ColorPalette::Nebula, 0.05, 0.0, "\x1b[2;37m"),
            (ColorPalette::Cividis, 0.05, 0.0, "\x1b[2;38;5;240m"),
            (ColorPalette::Amber, 0.05, 0.0, "\x1b[2;38;5;239m"),
            (ColorPalette::Mono, 0.05, 0.0, "\x1b[2;38;5;240m"),
            // Medium stars (0.085 <= value < 0.150), low hue
            (ColorPalette::Nebula, 0.10, 0.25, "\x1b[38;5;67m"),
            (ColorPalette::Cividis, 0.10, 0.25, "\x1b[38;5;24m"),
            (ColorPalette::Amber, 0.10, 0.25, "\x1b[38;5;94m"),
            (ColorPalette::Mono, 0.10, 0.25, "\x1b[38;5;245m"),
            // Medium stars (0.085 <= value < 0.150), high hue
            (ColorPalette::Nebula, 0.10, 0.75, "\x1b[38;5;109m"),
            (ColorPalette::Cividis, 0.10, 0.75, "\x1b[38;5;66m"),
            (ColorPalette::Amber, 0.10, 0.75, "\x1b[38;5;130m"),
            (ColorPalette::Mono, 0.10, 0.75, "\x1b[38;5;248m"),
            // Bright stars (value >= 0.150), low hue
            (ColorPalette::Nebula, 0.20, 0.30, "\x1b[38;5;250m"),
            (ColorPalette::Cividis, 0.20, 0.30, "\x1b[38;5;252m"),
            (ColorPalette::Amber, 0.20, 0.30, "\x1b[38;5;214m"),
            (ColorPalette::Mono, 0.20, 0.30, "\x1b[38;5;250m"),
            // Bright stars (value >= 0.150), middle hue
            (ColorPalette::Nebula, 0.20, 0.70, "\x1b[38;5;180m"),
            (ColorPalette::Cividis, 0.20, 0.70, "\x1b[38;5;186m"),
            (ColorPalette::Amber, 0.20, 0.70, "\x1b[38;5;220m"),
            (ColorPalette::Mono, 0.20, 0.70, "\x1b[38;5;253m"),
            // Bright stars (value >= 0.150), high hue
            (ColorPalette::Nebula, 0.20, 0.95, "\x1b[38;5;167m"),
            (ColorPalette::Cividis, 0.20, 0.95, "\x1b[38;5;228m"),
            (ColorPalette::Amber, 0.20, 0.95, "\x1b[38;5;230m"),
            (ColorPalette::Mono, 0.20, 0.95, "\x1b[38;5;255m"),
        ];

        for (palette, value, hue, expected) in tests {
            let actual = starfield_foreground_ansi(palette, value, hue);
            assert_eq!(
                actual, expected,
                "starfield_foreground_ansi({:?}, {}, {}) should be {}, got {}",
                palette, value, hue, expected, actual
            );
        }
    }

    /// Hue boundary tests for Nebula palette
    #[test]
    fn test_starfield_nebula_hue_boundaries() {
        // Medium stars: 0.085 <= value < 0.150, hue boundary at 0.50
        assert_eq!(
            starfield_foreground_ansi(ColorPalette::Nebula, 0.10, 0.50),
            "\x1b[38;5;109m"
        );
        // Bright stars: value >= 0.150, hue boundaries at 0.58 and 0.84
        assert_eq!(
            starfield_foreground_ansi(ColorPalette::Nebula, 0.20, 0.58),
            "\x1b[38;5;180m"
        );
        assert_eq!(
            starfield_foreground_ansi(ColorPalette::Nebula, 0.20, 0.84),
            "\x1b[38;5;167m"
        );
    }
}
