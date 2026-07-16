use crate::display_plan::LayoutKind;
use crate::terminal::visible_width;

/// Composição lado a lado (side-by-side).
///
/// Preserves current behavior:
/// * ANSI-aware width;
/// * Unicode-aware width;
/// * Deterministic padding.
pub fn compose_side_by_side(
    art_lines: &[String],
    info_lines: &[String],
    art_width: usize,
    gap: usize,
) -> Vec<String> {
    let mut result = Vec::new();

    let max_art_lines = art_lines.len();
    let max_info_lines = info_lines.len();
    let max_lines = max_art_lines.max(max_info_lines);

    for i in 0..max_lines {
        let art_line = art_lines.get(i).map(|s| s.as_str()).unwrap_or("");
        let info_line = info_lines.get(i).map(|s| s.as_str()).unwrap_or("");

        let art_vis_width = visible_width(art_line);
        let padding = gap + art_width.saturating_sub(art_vis_width);

        let line = format!(
            "{}{:>padding$}{}",
            art_line,
            "",
            info_line,
            padding = padding
        );
        result.push(line);
    }

    result
}

/// Composição empilhada (stacked).
///
/// Behavior:
/// ```text
/// art lines
/// one empty separator line
/// info lines
/// ```
///
/// Add the separator only when both blocks are non-empty.
pub fn compose_stacked(art_lines: &[String], info_lines: &[String]) -> Vec<String> {
    let mut result = Vec::new();

    // Add art lines
    for line in art_lines {
        result.push(line.clone());
    }

    // Add separator only when both blocks are non-empty
    if !art_lines.is_empty() && !info_lines.is_empty() {
        result.push(String::new());
    }

    // Add info lines
    for line in info_lines {
        result.push(line.clone());
    }

    result
}

/// Composição de layout escolhido.
pub fn compose_layout(
    art_lines: &[String],
    info_lines: &[String],
    art_width: usize,
    layout: LayoutKind,
) -> Vec<String> {
    match layout {
        LayoutKind::SideBySide => compose_side_by_side(art_lines, info_lines, art_width, 2),
        LayoutKind::Stacked => compose_stacked(art_lines, info_lines),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compose_side_by_side_basic() {
        let art = vec!["*".to_string(), "**".to_string()];
        let info = vec!["info1".to_string(), "info2".to_string()];

        let result = compose_side_by_side(&art, &info, 10, 2);

        assert_eq!(result.len(), 2);
        assert!(result[0].contains("*"));
        assert!(result[0].contains("info1"));
    }

    #[test]
    fn test_compose_side_by_side_ansi_aware() {
        let art = vec!["\x1b[31m*\x1b[0m".to_string()];
        let info = vec!["info".to_string()];

        let result = compose_side_by_side(&art, &info, 10, 2);

        assert!(result[0].contains("*"));
        assert!(result[0].contains("info"));
    }

    #[test]
    fn test_compose_side_by_side_unicode_aware() {
        let art = vec!["▀▄█".to_string()];
        let info = vec!["info".to_string()];

        let result = compose_side_by_side(&art, &info, 10, 2);

        assert!(result[0].contains("▀▄█"));
        assert!(result[0].contains("info"));
    }

    #[test]
    fn test_compose_side_by_side_deterministic_padding() {
        let art = vec!["*".to_string()];
        let info = vec!["info".to_string()];

        let result = compose_side_by_side(&art, &info, 10, 2);

        // Art is 1 char, so padding = 2 + (10 - 1) = 11
        // Total: 1 (art) + 11 (padding) + 4 (info) = 16
        assert_eq!(result[0].len(), 16);
    }

    #[test]
    fn test_compose_stacked_ordering() {
        let art = vec!["art1".to_string(), "art2".to_string()];
        let info = vec!["info1".to_string(), "info2".to_string()];

        let result = compose_stacked(&art, &info);

        assert_eq!(result.len(), 5); // 2 art + 1 separator + 2 info
        assert_eq!(result[0], "art1");
        assert_eq!(result[1], "art2");
        assert_eq!(result[2], "");
        assert_eq!(result[3], "info1");
        assert_eq!(result[4], "info2");
    }

    #[test]
    fn test_compose_stacked_conditional_separator() {
        // Both non-empty: separator should be added
        let art = vec!["art".to_string()];
        let info = vec!["info".to_string()];

        let result = compose_stacked(&art, &info);
        assert_eq!(result.len(), 3);
        assert_eq!(result[1], "");

        // Empty art: no separator
        let art: Vec<String> = vec![];
        let info = vec!["info".to_string()];

        let result = compose_stacked(&art, &info);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "info");

        // Empty info: no separator
        let art = vec!["art".to_string()];
        let info: Vec<String> = vec![];

        let result = compose_stacked(&art, &info);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "art");

        // Both empty: no separator
        let art: Vec<String> = vec![];
        let info: Vec<String> = vec![];

        let result = compose_stacked(&art, &info);
        assert!(result.is_empty());
    }

    #[test]
    fn test_compose_layout_side_by_side() {
        let art = vec!["*".to_string()];
        let info = vec!["info".to_string()];

        let result = compose_layout(&art, &info, 10, LayoutKind::SideBySide);

        assert!(result[0].contains("*"));
        assert!(result[0].contains("info"));
    }

    #[test]
    fn test_compose_layout_stacked() {
        let art = vec!["*".to_string()];
        let info = vec!["info".to_string()];

        let result = compose_layout(&art, &info, 10, LayoutKind::Stacked);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "*");
        assert_eq!(result[1], "");
        assert_eq!(result[2], "info");
    }
}
