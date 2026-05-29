use crate::terminal::visible_width;

/// Composição da arte ASCII com informações do sistema.
pub fn compose_layout(
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

/// Adiciona bordas ou padding à arte.
#[allow(dead_code)]
pub fn pad_art(art_lines: &[String], padding: usize) -> Vec<String> {
    art_lines
        .iter()
        .map(|line| format!("{:width$}{}{:width$}", "", line, "", width = padding))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compose_layout_basic() {
        let art = vec!["*".to_string(), "**".to_string(), "***".to_string()];
        let info = vec!["user@host".to_string(), "OS: Linux".to_string()];

        let result = compose_layout(&art, &info, 10, 2);

        assert_eq!(result.len(), 3);
        // Verifica que a linha tem arte e info
        assert!(result[0].contains("*"));
        assert!(result[0].contains("user@host"));
    }

    #[test]
    fn test_compose_layout_different_lengths() {
        let art = vec!["*".to_string()];
        let info = vec![
            "line1".to_string(),
            "line2".to_string(),
            "line3".to_string(),
        ];

        let result = compose_layout(&art, &info, 10, 2);

        assert_eq!(result.len(), 3);
        // Primeira linha tem arte
        assert!(result[0].contains("*"));
        // Linhas 2 e 3 são apenas info
        assert!(result[1].contains("line2"));
        assert!(result[2].contains("line3"));
    }

    #[test]
    fn test_compose_layout_with_ansi() {
        let art = vec!["\x1b[31m*\x1b[0m".to_string()];
        let info = vec!["info".to_string()];

        let result = compose_layout(&art, &info, 10, 2);

        assert!(result[0].contains("*"));
        assert!(result[0].contains("info"));
    }

    #[test]
    fn test_compose_layout_no_panic() {
        // Deve funcionar mesmo com listas vazias
        let art: Vec<String> = vec![];
        let info: Vec<String> = vec![];

        let result = compose_layout(&art, &info, 10, 2);
        assert!(result.is_empty());
    }
}
