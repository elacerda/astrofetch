use crate::render::color::RESET;

/// Internal builder for rendering ANSI foreground sequences.
///
/// Groups consecutive characters using the same style to minimize ANSI escape sequences.
/// The builder tracks the current active style and only emits:
/// - The style sequence when first used or when changing styles
/// - RESET before a plain character if a style is active
/// - Exactly one final RESET when finishing if a style is active
pub(super) struct AnsiForegroundLine {
    output: String,
    current_style: Option<&'static str>,
}

impl AnsiForegroundLine {
    /// Creates a new line builder with the specified capacity.
    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            output: String::with_capacity(capacity),
            current_style: None,
        }
    }

    /// Pushes a styled character with the given style.
    ///
    /// If the style differs from the current style, emits RESET then the new style.
    /// If the style matches the current style, emits only the character.
    pub(super) fn push_styled(&mut self, ch: char, style: &'static str) {
        match self.current_style {
            None => {
                // No active style: emit style then character
                self.output.push_str(style);
                self.output.push(ch);
                self.current_style = Some(style);
            }
            Some(current) => {
                if current == style {
                    // Same style: emit only character
                    self.output.push(ch);
                } else {
                    // Different style: emit RESET, then new style, then character
                    self.output.push_str(RESET);
                    self.output.push_str(style);
                    self.output.push(ch);
                    self.current_style = Some(style);
                }
            }
        }
    }

    /// Pushes a plain (unstyled) character.
    ///
    /// If a style is active, emits RESET before the character.
    pub(super) fn push_plain(&mut self, ch: char) {
        if self.current_style.is_some() {
            self.output.push_str(RESET);
            self.current_style = None;
        }
        self.output.push(ch);
    }

    /// Finishes the line and returns the ANSI-encoded string.
    ///
    /// Appends exactly one RESET if a style is active (i.e., the line ends with styled content).
    /// No RESET is appended if the line ends with plain content.
    pub(super) fn finish(self) -> String {
        if self.current_style.is_some() {
            self.output + RESET
        } else {
            self.output
        }
    }
}

/// Internal builder for rendering ANSI half-block cells with independent foreground
/// and background colors.
///
/// The HalfBlock renderer combines foreground and background colors independently,
/// so this builder tracks both independently and uses a conservative transition strategy:
///
/// 1. If the complete next style is identical to the current style, append only the glyph.
/// 2. If the style changes and any style is currently active:
///    - emit RESET;
///    - emit the new foreground, if present;
///    - emit the new background, if present;
///    - append the glyph.
/// 3. If the current style is plain and the next cell is styled:
///    - emit the new foreground, if present;
///    - emit the new background, if present;
///    - append the glyph.
/// 4. If a styled cell is followed by a plain star or space:
///    - emit RESET;
///    - append the plain glyph.
/// 5. At the end of the line:
///    - emit exactly one final RESET if a style is active;
///    - emit no reset if the line ends plain.
///
/// This conservative strategy (full RESET on any change) ensures:
/// - Removal of old background when moving to foreground-only
/// - Removal of old foreground when necessary
/// - Prevents dim from leaking into non-dim foreground
/// - Prevents backgrounds from leaking into stars and spaces
pub(super) struct AnsiHalfBlockLine {
    output: String,
    current_foreground: Option<&'static str>,
    current_background: Option<&'static str>,
}

impl AnsiHalfBlockLine {
    /// Creates a new line builder with the specified capacity.
    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            output: String::with_capacity(capacity),
            current_foreground: None,
            current_background: None,
        }
    }

    /// Pushes a cell with the given foreground and background.
    ///
    /// The `ch` is the half-block character (▀, ▄, █, or space).
    /// If both `foreground` and `background` are None, the cell is plain.
    pub(super) fn push_cell(
        &mut self,
        ch: char,
        foreground: Option<&'static str>,
        background: Option<&'static str>,
    ) {
        // Check if the complete next style is identical to current
        if foreground == self.current_foreground && background == self.current_background {
            // Same complete style: emit only the glyph
            self.output.push(ch);
        } else {
            // Style changed: emit RESET first if any style was active
            if self.current_foreground.is_some() || self.current_background.is_some() {
                self.output.push_str(RESET);
            }

            // Emit new foreground if present
            if let Some(fg) = foreground {
                self.output.push_str(fg);
            }

            // Emit new background if present
            if let Some(bg) = background {
                self.output.push_str(bg);
            }

            // Append the glyph
            self.output.push(ch);

            // Update current state
            self.current_foreground = foreground;
            self.current_background = background;
        }
    }

    /// Finishes the line and returns the ANSI-encoded string.
    ///
    /// Appends exactly one RESET if a style is active (i.e., the line ends with styled content).
    /// No RESET is appended if the line ends with plain content.
    pub(super) fn finish(self) -> String {
        if self.current_foreground.is_some() || self.current_background.is_some() {
            self.output + RESET
        } else {
            self.output
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== AnsiForegroundLine tests (unchanged) =====

    #[test]
    fn test_plain_only_line() {
        // Plain-only line should contain no ANSI sequences
        let mut line = AnsiForegroundLine::with_capacity(64);
        line.push_plain('a');
        line.push_plain('b');
        line.push_plain('c');
        let result = line.finish();
        assert_eq!(result, "abc");
        assert!(!result.contains('\x1b'));
    }

    #[test]
    fn test_same_style_run() {
        // Same style for multiple characters: one style prefix, one reset at end
        let mut line = AnsiForegroundLine::with_capacity(64);
        line.push_styled('a', "\x1b[38;5;17m");
        line.push_styled('b', "\x1b[38;5;17m");
        let result = line.finish();
        // Style appears once, reset appears once
        assert_eq!(result, "\x1b[38;5;17mab\x1b[0m");
        // Count occurrences
        assert_eq!(result.matches("\x1b[38;5;17m").count(), 1);
        assert_eq!(result.matches("\x1b[0m").count(), 1);
    }

    #[test]
    fn test_style_change() {
        // Style change: RESET between styles
        let mut line = AnsiForegroundLine::with_capacity(64);
        line.push_styled('a', "\x1b[38;5;17m");
        line.push_styled('b', "\x1b[38;5;30m");
        let result = line.finish();
        // Expected: STYLE_A + "a" + RESET + STYLE_B + "b" + RESET
        assert_eq!(result, "\x1b[38;5;17ma\x1b[0m\x1b[38;5;30mb\x1b[0m");
    }

    #[test]
    fn test_dim_to_non_dim_transition() {
        // Dim (2;) to non-dim must emit RESET before new style to prevent dim leakage
        let dim_style = "\x1b[2;38;5;17m"; // dim blue
        let non_dim_style = "\x1b[38;5;30m"; // non-dim cyan
        let mut line = AnsiForegroundLine::with_capacity(64);
        line.push_styled('a', dim_style);
        line.push_styled('b', non_dim_style);
        let result = line.finish();
        // Reset must occur before the new style (not just appended)
        assert_eq!(result, "\x1b[2;38;5;17ma\x1b[0m\x1b[38;5;30mb\x1b[0m");
    }

    #[test]
    fn test_styled_to_plain_transition() {
        // Styled to plain: RESET before plain character
        let mut line = AnsiForegroundLine::with_capacity(64);
        line.push_styled('a', "\x1b[38;5;17m");
        line.push_plain(' ');
        let result = line.finish();
        assert_eq!(result, "\x1b[38;5;17ma\x1b[0m ");
    }

    #[test]
    fn test_plain_to_styled_transition() {
        // Plain to styled: style before styled character
        let mut line = AnsiForegroundLine::with_capacity(64);
        line.push_plain(' ');
        line.push_styled('a', "\x1b[38;5;17m");
        let result = line.finish();
        assert_eq!(result, " \x1b[38;5;17ma\x1b[0m");
    }

    #[test]
    fn test_finish_without_active_style() {
        // Finish with no active style: no trailing reset
        let mut line = AnsiForegroundLine::with_capacity(64);
        line.push_plain('a');
        line.push_plain('b');
        let result = line.finish();
        assert_eq!(result, "ab");
        assert!(!result.ends_with('\x1b'));
    }

    #[test]
    fn test_finish_with_active_style() {
        // Finish with active style: exactly one trailing reset
        let mut line = AnsiForegroundLine::with_capacity(64);
        line.push_styled('a', "\x1b[38;5;17m");
        let result = line.finish();
        assert_eq!(result, "\x1b[38;5;17ma\x1b[0m");
        assert!(result.ends_with("\x1b[0m"));
    }

    #[test]
    fn test_empty_line() {
        // Empty line returns empty string
        let line = AnsiForegroundLine::with_capacity(64);
        let result = line.finish();
        assert_eq!(result, "");
    }

    #[test]
    fn test_multiple_style_changes() {
        // Multiple style changes: RESET between each
        let mut line = AnsiForegroundLine::with_capacity(64);
        line.push_styled('a', "\x1b[38;5;17m");
        line.push_styled('b', "\x1b[38;5;30m");
        line.push_styled('c', "\x1b[38;5;17m");
        let result = line.finish();
        assert_eq!(
            result,
            "\x1b[38;5;17ma\x1b[0m\x1b[38;5;30mb\x1b[0m\x1b[38;5;17mc\x1b[0m"
        );
    }

    #[test]
    fn test_mixed_plain_and_styled() {
        // Mixed plain and styled characters
        let mut line = AnsiForegroundLine::with_capacity(64);
        line.push_plain(' ');
        line.push_styled('a', "\x1b[38;5;17m");
        line.push_plain(' ');
        line.push_styled('b', "\x1b[38;5;30m");
        let result = line.finish();
        assert_eq!(result, " \x1b[38;5;17ma\x1b[0m \x1b[38;5;30mb\x1b[0m");
    }

    // ===== AnsiHalfBlockLine tests =====

    #[test]
    fn test_half_block_empty_line() {
        // Empty line returns empty string
        let line = AnsiHalfBlockLine::with_capacity(64);
        let result = line.finish();
        assert_eq!(result, "");
    }

    #[test]
    fn test_half_block_plain_only_line() {
        // Plain-only line should contain no ANSI sequences
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell(' ', None, None);
        line.push_cell(' ', None, None);
        let result = line.finish();
        assert_eq!(result, "  ");
        assert!(!result.contains('\x1b'));
    }

    #[test]
    fn test_half_block_repeated_foreground_only() {
        // Repeated foreground-only style: one sequence, one final reset
        let fg = "\x1b[38;5;65m";
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell('▀', Some(fg), None);
        line.push_cell('▀', Some(fg), None);
        let result = line.finish();
        assert_eq!(result, "\x1b[38;5;65m▀▀\x1b[0m");
    }

    #[test]
    fn test_half_block_repeated_foreground_plus_background() {
        // Repeated foreground+background style: one fg, one bg, one final reset
        let fg = "\x1b[38;5;65m";
        let bg = "\x1b[48;5;130m";
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell('▀', Some(fg), Some(bg));
        line.push_cell('▀', Some(fg), Some(bg));
        let result = line.finish();
        assert_eq!(result, "\x1b[38;5;65m\x1b[48;5;130m▀▀\x1b[0m");
    }

    #[test]
    fn test_half_block_foreground_change() {
        // Foreground change: RESET, new foreground
        let fg1 = "\x1b[38;5;65m";
        let fg2 = "\x1b[38;5;130m";
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell('▀', Some(fg1), None);
        line.push_cell('▄', Some(fg2), None);
        let result = line.finish();
        assert_eq!(result, "\x1b[38;5;65m▀\x1b[0m\x1b[38;5;130m▄\x1b[0m");
    }

    #[test]
    fn test_half_block_background_change() {
        // Background change: RESET, new background
        let fg = "\x1b[38;5;65m";
        let bg1 = "\x1b[48;5;130m";
        let bg2 = "\x1b[48;5;200m";
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell('▀', Some(fg), Some(bg1));
        line.push_cell('▀', Some(fg), Some(bg2));
        let result = line.finish();
        assert_eq!(
            result,
            "\x1b[38;5;65m\x1b[48;5;130m▀\x1b[0m\x1b[38;5;65m\x1b[48;5;200m▀\x1b[0m"
        );
    }

    #[test]
    fn test_half_block_foreground_and_background_change() {
        // Both foreground and background change: RESET, new fg, new bg
        let fg1 = "\x1b[38;5;65m";
        let bg1 = "\x1b[48;5;130m";
        let fg2 = "\x1b[38;5;130m";
        let bg2 = "\x1b[48;5;200m";
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell('▀', Some(fg1), Some(bg1));
        line.push_cell('█', Some(fg2), Some(bg2));
        let result = line.finish();
        assert_eq!(
            result,
            "\x1b[38;5;65m\x1b[48;5;130m▀\x1b[0m\x1b[38;5;130m\x1b[48;5;200m█\x1b[0m"
        );
    }

    #[test]
    fn test_half_block_plain_to_styled() {
        // Plain to styled: style before styled character
        let fg = "\x1b[38;5;65m";
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell(' ', None, None);
        line.push_cell('▀', Some(fg), None);
        let result = line.finish();
        assert_eq!(result, " \x1b[38;5;65m▀\x1b[0m");
    }

    #[test]
    fn test_half_block_styled_to_plain() {
        // Styled to plain: RESET before plain character
        let fg = "\x1b[38;5;65m";
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell('▀', Some(fg), None);
        line.push_cell(' ', None, None);
        let result = line.finish();
        assert_eq!(result, "\x1b[38;5;65m▀\x1b[0m ");
    }

    #[test]
    fn test_half_block_remove_background_when_to_foreground_only() {
        // Change from fg+bg to fg-only: RESET removes the old background
        let fg = "\x1b[38;5;65m";
        let bg = "\x1b[48;5;130m";
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell('▀', Some(fg), Some(bg));
        line.push_cell('▄', Some(fg), None);
        let result = line.finish();
        // RESET removes background, new fg is applied
        assert_eq!(
            result,
            "\x1b[38;5;65m\x1b[48;5;130m▀\x1b[0m\x1b[38;5;65m▄\x1b[0m"
        );
    }

    #[test]
    fn test_half_block_dim_to_non_dim_transition() {
        // Dim (2;) to non-dim must emit RESET before new style
        let dim_fg = "\x1b[2;38;5;17m";
        let non_dim_fg = "\x1b[38;5;30m";
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell('▀', Some(dim_fg), None);
        line.push_cell('▀', Some(non_dim_fg), None);
        let result = line.finish();
        assert_eq!(result, "\x1b[2;38;5;17m▀\x1b[0m\x1b[38;5;30m▀\x1b[0m");
    }

    #[test]
    fn test_half_block_exactly_one_final_reset() {
        // Line ends with style: exactly one final reset
        let fg = "\x1b[38;5;65m";
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell('▀', Some(fg), None);
        let result = line.finish();
        assert_eq!(result, "\x1b[38;5;65m▀\x1b[0m");
        assert!(result.ends_with("\x1b[0m"));
    }

    #[test]
    fn test_half_block_no_final_reset_when_plain() {
        // Line ends plain: no reset
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell(' ', None, None);
        let result = line.finish();
        assert_eq!(result, " ");
        assert!(!result.ends_with('\x1b'));
    }

    #[test]
    fn test_half_block_identical_complete_styles_grouped() {
        // Identical complete styles remain grouped even when glyphs differ
        let fg = "\x1b[38;5;65m";
        let bg = "\x1b[48;5;130m";
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell('▀', Some(fg), Some(bg));
        line.push_cell('█', Some(fg), Some(bg));
        line.push_cell('▄', Some(fg), Some(bg));
        let result = line.finish();
        // All three cells with same style should be grouped
        assert_eq!(result, "\x1b[38;5;65m\x1b[48;5;130m▀█▄\x1b[0m");
    }

    #[test]
    fn test_half_block_top_only_to_both_visible() {
        // Top-only to both-visible transition
        let fg = "\x1b[38;5;65m";
        let bg = "\x1b[48;5;130m";
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell('▀', Some(fg), None); // top only
        line.push_cell('▀', Some(fg), Some(bg)); // both visible
        let result = line.finish();
        assert_eq!(
            result,
            "\x1b[38;5;65m▀\x1b[0m\x1b[38;5;65m\x1b[48;5;130m▀\x1b[0m"
        );
    }

    #[test]
    fn test_half_block_both_visible_to_top_only() {
        // Both-visible to top-only transition: background removed
        let fg = "\x1b[38;5;65m";
        let bg = "\x1b[48;5;130m";
        let mut line = AnsiHalfBlockLine::with_capacity(64);
        line.push_cell('▀', Some(fg), Some(bg)); // both visible
        line.push_cell('▀', Some(fg), None); // top only
        let result = line.finish();
        assert_eq!(
            result,
            "\x1b[38;5;65m\x1b[48;5;130m▀\x1b[0m\x1b[38;5;65m▀\x1b[0m"
        );
    }
}
