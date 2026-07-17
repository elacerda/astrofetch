/// Internal color palette abstraction for procedural art.
///
/// This module provides the color-palette foundation for Patch 3C-A.
/// The `Nebula` palette represents the current AstroFetch ANSI color behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorPalette {
    /// The standard AstroFetch color scheme - preserves current ANSI behavior.
    Nebula,
}

pub(super) const RESET: &str = "\x1b[0m";

/// Galaxies use foreground color based on density value.
///
/// For `ColorPalette::Nebula`, the thresholds and ANSI sequences preservation
/// the current behavior exactly.
pub(super) fn galaxy_foreground_ansi(palette: ColorPalette, value: f64) -> &'static str {
    match palette {
        ColorPalette::Nebula => {
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
    }
}

/// Galaxies use background color based on density value.
///
/// For `ColorPalette::Nebula`, uses the same indices as foreground but with
/// background ANSI sequence (`48;5;<index>m`) without dim attribute.
pub(super) fn galaxy_background_ansi(palette: ColorPalette, value: f64) -> &'static str {
    match palette {
        ColorPalette::Nebula => {
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
    }
}

/// Starfield uses foreground color based on density value and deterministic hue.
///
/// The hue is calculated deterministically from the cell coordinates and a fixed seed.
/// For `ColorPalette::Nebula`, the thresholds and ANSI sequences preserve the current behavior exactly.
pub(super) fn starfield_foreground_ansi(
    palette: ColorPalette,
    value: f64,
    hue: f64,
) -> &'static str {
    match palette {
        ColorPalette::Nebula => {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Constant for default palette in tests
    const DEFAULT_PALETTE: ColorPalette = ColorPalette::Nebula;

    // ===== ColorPalette enum tests =====

    #[test]
    fn test_color_palette_is_copy() {
        let palette = DEFAULT_PALETTE;
        let cloned = palette;
        assert_eq!(palette, cloned);
    }

    #[test]
    fn test_color_palette_is_eq() {
        assert_eq!(DEFAULT_PALETTE, ColorPalette::Nebula);
    }

    // ===== Galaxy foreground color tests =====
    // Test exact output at representative boundaries
    // value < 0.16  → \x1b[2;38;5;17m
    // value < 0.30  → \x1b[2;38;5;24m
    // value < 0.44  → \x1b[38;5;30m
    // value < 0.58  → \x1b[38;5;65m
    // value < 0.72  → \x1b[38;5;136m
    // value < 0.88  → \x1b[38;5;130m
    // otherwise     → \x1b[38;5;255m

    #[test]
    fn test_galaxy_foreground_ansi_nebula_at_boundaries() {
        let tests = [
            (0.00, "\x1b[2;38;5;17m"),  // < 0.16
            (0.159, "\x1b[2;38;5;17m"), // < 0.16
            (0.16, "\x1b[2;38;5;24m"),  // < 0.30
            (0.299, "\x1b[2;38;5;24m"), // < 0.30
            (0.30, "\x1b[38;5;30m"),    // < 0.44
            (0.439, "\x1b[38;5;30m"),   // < 0.44
            (0.44, "\x1b[38;5;65m"),    // < 0.58
            (0.579, "\x1b[38;5;65m"),   // < 0.58
            (0.58, "\x1b[38;5;136m"),   // < 0.72
            (0.719, "\x1b[38;5;136m"),  // < 0.72
            (0.72, "\x1b[38;5;130m"),   // < 0.88
            (0.879, "\x1b[38;5;130m"),  // < 0.88
            (0.88, "\x1b[38;5;255m"),   // otherwise
            (1.00, "\x1b[38;5;255m"),   // otherwise
        ];

        for (value, expected) in tests {
            let actual = galaxy_foreground_ansi(DEFAULT_PALETTE, value);
            assert_eq!(
                actual, expected,
                "galaxy_foreground_ansi({}) should be {}, got {}",
                value, expected, actual
            );
        }
    }

    #[test]
    fn test_galaxy_foreground_ansi_nebula_faint_value() {
        let result = galaxy_foreground_ansi(DEFAULT_PALETTE, 0.0);
        assert_eq!(result, "\x1b[2;38;5;17m");
    }

    #[test]
    fn test_galaxy_foreground_ansi_nebula_deep_blue() {
        let result = galaxy_foreground_ansi(DEFAULT_PALETTE, 0.15);
        assert_eq!(result, "\x1b[2;38;5;17m");
    }

    #[test]
    fn test_galaxy_foreground_ansi_nebula_muted_green() {
        let result = galaxy_foreground_ansi(DEFAULT_PALETTE, 0.5);
        assert_eq!(result, "\x1b[38;5;65m");
    }

    #[test]
    fn test_galaxy_foreground_ansi_nebula_soft_white_core() {
        let result = galaxy_foreground_ansi(DEFAULT_PALETTE, 0.95);
        assert_eq!(result, "\x1b[38;5;255m");
    }

    // ===== Galaxy background color tests =====
    // Same thresholds, different ANSI sequences (background uses 48;5;<index>m)
    // 17, 24, 30, 65, 136, 130, 255

    #[test]
    fn test_galaxy_background_ansi_nebula_at_boundaries() {
        let tests = [
            (0.00, "\x1b[48;5;17m"),   // < 0.16
            (0.159, "\x1b[48;5;17m"),  // < 0.16
            (0.16, "\x1b[48;5;24m"),   // < 0.30
            (0.299, "\x1b[48;5;24m"),  // < 0.30
            (0.30, "\x1b[48;5;30m"),   // < 0.44
            (0.439, "\x1b[48;5;30m"),  // < 0.44
            (0.44, "\x1b[48;5;65m"),   // < 0.58
            (0.579, "\x1b[48;5;65m"),  // < 0.58
            (0.58, "\x1b[48;5;136m"),  // < 0.72
            (0.719, "\x1b[48;5;136m"), // < 0.72
            (0.72, "\x1b[48;5;130m"),  // < 0.88
            (0.879, "\x1b[48;5;130m"), // < 0.88
            (0.88, "\x1b[48;5;255m"),  // otherwise
            (1.00, "\x1b[48;5;255m"),  // otherwise
        ];

        for (value, expected) in tests {
            let actual = galaxy_background_ansi(DEFAULT_PALETTE, value);
            assert_eq!(
                actual, expected,
                "galaxy_background_ansi({}) should be {}, got {}",
                value, expected, actual
            );
        }
    }

    #[test]
    fn test_galaxy_background_ansi_nebula_faint_value() {
        let result = galaxy_background_ansi(DEFAULT_PALETTE, 0.0);
        assert_eq!(result, "\x1b[48;5;17m");
    }

    #[test]
    fn test_galaxy_background_ansi_nebula_deep_blue() {
        let result = galaxy_background_ansi(DEFAULT_PALETTE, 0.15);
        assert_eq!(result, "\x1b[48;5;17m");
    }

    #[test]
    fn test_galaxy_background_ansi_nebula_muted_green() {
        let result = galaxy_background_ansi(DEFAULT_PALETTE, 0.5);
        assert_eq!(result, "\x1b[48;5;65m");
    }

    #[test]
    fn test_galaxy_background_ansi_nebula_soft_white() {
        let result = galaxy_background_ansi(DEFAULT_PALETTE, 0.95);
        assert_eq!(result, "\x1b[48;5;255m");
    }

    // ===== Starfield foreground color tests =====
    // value < 0.085              → \x1b[2;37m
    // value < 0.150, hue < 0.50 → \x1b[38;5;67m
    // value < 0.150, otherwise  → \x1b[38;5;109m
    // bright, hue < 0.58        → \x1b[38;5;250m
    // bright, hue < 0.84        → \x1b[38;5;180m
    // bright, otherwise         → \x1b[38;5;167m

    #[test]
    fn test_starfield_foreground_ansi_nebula_faint() {
        // value < 0.085 → faint gray
        let result = starfield_foreground_ansi(DEFAULT_PALETTE, 0.05, 0.0);
        assert_eq!(result, "\x1b[2;37m");
    }

    #[test]
    fn test_starfield_foreground_ansi_nebula_medium_low_hue() {
        // value < 0.150, hue < 0.50 → muted pale blue
        let result = starfield_foreground_ansi(DEFAULT_PALETTE, 0.10, 0.25);
        assert_eq!(result, "\x1b[38;5;67m");
    }

    #[test]
    fn test_starfield_foreground_ansi_nebula_medium_high_hue() {
        // value < 0.150, hue >= 0.50 → muted pale cyan
        let result = starfield_foreground_ansi(DEFAULT_PALETTE, 0.10, 0.75);
        assert_eq!(result, "\x1b[38;5;109m");
    }

    #[test]
    fn test_starfield_foreground_ansi_nebula_bright_low_hue() {
        // bright, hue < 0.58 → soft white
        let result = starfield_foreground_ansi(DEFAULT_PALETTE, 0.20, 0.30);
        assert_eq!(result, "\x1b[38;5;250m");
    }

    #[test]
    fn test_starfield_foreground_ansi_nebula_bright_middle_hue() {
        // bright, hue >= 0.58 and < 0.84 → soft warm star
        let result = starfield_foreground_ansi(DEFAULT_PALETTE, 0.20, 0.70);
        assert_eq!(result, "\x1b[38;5;180m");
    }

    #[test]
    fn test_starfield_foreground_ansi_nebula_bright_high_hue() {
        // bright, hue >= 0.84 → rare muted red star
        let result = starfield_foreground_ansi(DEFAULT_PALETTE, 0.20, 0.95);
        assert_eq!(result, "\x1b[38;5;167m");
    }

    #[test]
    fn test_starfield_foreground_ansi_nebula_boundary_medium_hue() {
        // hue exactly at 0.50 boundary (uses high hue path)
        let result = starfield_foreground_ansi(DEFAULT_PALETTE, 0.10, 0.50);
        assert_eq!(result, "\x1b[38;5;109m");
    }

    #[test]
    fn test_starfield_foreground_ansi_nebula_boundary_bright_low() {
        // hue exactly at 0.58 boundary (uses middle hue path)
        let result = starfield_foreground_ansi(DEFAULT_PALETTE, 0.20, 0.58);
        assert_eq!(result, "\x1b[38;5;180m");
    }

    #[test]
    fn test_starfield_foreground_ansi_nebula_boundary_bright_middle() {
        // hue exactly at 0.84 boundary (uses high hue path)
        let result = starfield_foreground_ansi(DEFAULT_PALETTE, 0.20, 0.84);
        assert_eq!(result, "\x1b[38;5;167m");
    }
}
