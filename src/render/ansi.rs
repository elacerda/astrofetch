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

#[cfg(test)]
mod tests {
    use super::*;

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
}
