/// Conta pacotes instalados em saída de `dpkg-query -W -f=${db:Status-Abbrev} ${binary:Package}\n`.
/// Formato: "ii package-name" (ii = installed).
/// Outros status: rc (removed but config), un (not installed), etc.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn parse_dpkg_query_installed_count(output: &str) -> Option<usize> {
    let count = output
        .lines()
        .filter(|line| line.trim().starts_with("ii "))
        .count();

    (count > 0).then_some(count)
}

/// Conta pacotes instalados em saída de `dpkg --get-selections`.
/// Formato: "package-name    install"
/// Conta linhas onde a segunda coluna é "install".
#[cfg(any(target_os = "linux", test))]
pub(crate) fn parse_dpkg_get_selections_installed_count(output: &str) -> Option<usize> {
    let count = output
        .lines()
        .filter(|line| {
            let mut parts = line.split_whitespace();
            parts.next().is_some() && parts.next() == Some("install")
        })
        .count();

    (count > 0).then_some(count)
}

/// Normaliza saída string do `gsettings get`.
/// Remove aspas simples circundantes. Retorna `None` se o valor for vazio ou
/// começar com `@` (indicando um tipo composto como `@as []`).
#[cfg(any(target_os = "linux", test))]
pub(crate) fn parse_gsettings_value(output: &str) -> Option<String> {
    let trimmed = output.trim();
    if trimmed.is_empty() || trimmed.starts_with('@') {
        return None;
    }

    let value = trimmed
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .unwrap_or(trimmed)
        .trim();

    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Faz parse da saída do `xrandr --current` e retorna a melhor resolução disponível.
///
/// Regras:
/// - Prefere monitor marcado como `primary`.
/// - Caso contrário, usa o primeiro monitor `connected` com modo atual.
/// - Ignora linhas `disconnected`.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn parse_xrandr_resolution(output: &str) -> Option<String> {
    let lines: Vec<&str> = output.lines().collect();
    let mut fallback: Option<String> = None;
    let mut i = 0usize;

    while i < lines.len() {
        let line = lines[i];

        if line.contains(" connected ") && !line.contains(" disconnected ") {
            let is_primary = line.contains(" connected primary ");

            let mut candidate = extract_resolution_from_connected_line(line);

            // Fallback: alguns drivers não colocam `WxH+X+Y` na linha do monitor.
            // Nesse caso, procura no bloco de modos abaixo (linha com `*`).
            if candidate.is_none() {
                let mut j = i + 1;
                while j < lines.len() {
                    let mode_line = lines[j];
                    if !mode_line.starts_with(' ') && !mode_line.starts_with('\t') {
                        break;
                    }

                    if let Some(mode_resolution) = extract_resolution_from_mode_line(mode_line) {
                        candidate = Some(mode_resolution);
                        break;
                    }

                    j += 1;
                }
            }

            if let Some(resolution) = candidate {
                if is_primary {
                    return Some(resolution);
                }
                if fallback.is_none() {
                    fallback = Some(resolution);
                }
            }
        }

        i += 1;
    }

    fallback
}

/// Extrai resolução da linha principal de um monitor conectado.
/// Exemplo suportado: `HDMI-0 connected primary 3440x1440+0+0 ...`
#[cfg(any(target_os = "linux", test))]
pub(crate) fn extract_resolution_from_connected_line(line: &str) -> Option<String> {
    for token in line.split_whitespace() {
        if let Some((resolution, _)) = token.split_once('+') {
            if is_resolution_token(resolution) {
                return Some(resolution.to_string());
            }
        }
    }
    None
}

/// Extrai resolução da linha de modos do xrandr (bloco indentado).
/// Exemplo suportado: `1920x1080     60.00*+ 59.93`.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn extract_resolution_from_mode_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let mode = trimmed.split_whitespace().next()?;
    if is_resolution_token(mode) {
        Some(mode.to_string())
    } else {
        None
    }
}

/// Verifica se uma string está no formato de resolução `WxH`.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn is_resolution_token(token: &str) -> bool {
    let (width, height) = match token.split_once('x') {
        Some(parts) => parts,
        None => return false,
    };

    !width.is_empty()
        && !height.is_empty()
        && width.chars().all(|c| c.is_ascii_digit())
        && height.chars().all(|c| c.is_ascii_digit())
}

/// Faz parse da saída do `lspci` para controladores gráficos.
/// Retorna um nome conciso por GPU, juntando múltiplas entradas com ` / `.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn parse_lspci_gpu_info(output: &str) -> Option<String> {
    let mut gpus: Vec<String> = Vec::new();

    for line in output.lines() {
        let Some(raw_name) = extract_lspci_gpu_name(line) else {
            continue;
        };

        let normalized = normalize_gpu_name(raw_name);
        if normalized.is_empty() || gpus.iter().any(|gpu| gpu == &normalized) {
            continue;
        }

        gpus.push(normalized);
    }

    if gpus.is_empty() {
        None
    } else {
        Some(gpus.join(" / "))
    }
}

/// Extrai o nome bruto do controlador gráfico a partir de uma linha do `lspci`.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn extract_lspci_gpu_name(line: &str) -> Option<&str> {
    let lower = line.to_ascii_lowercase();
    let markers = [
        "vga compatible controller:",
        "3d controller:",
        "display controller:",
    ];

    for marker in markers {
        if let Some(idx) = lower.find(marker) {
            let start = idx + marker.len();
            return Some(line[start..].trim());
        }
    }

    None
}

/// Normaliza nome bruto de GPU para uma versão mais concisa.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn normalize_gpu_name(raw: &str) -> String {
    let cleaned = strip_lspci_revision(raw);

    if cleaned.contains("NVIDIA Corporation") {
        let remainder = cleaned.replace("NVIDIA Corporation", "").trim().to_string();
        if let Some(geforce) = find_bracket_content_with_keyword(&remainder, "GeForce") {
            return collapse_whitespace(&format!("NVIDIA {}", geforce));
        }
        return collapse_whitespace(&format!("NVIDIA {}", remainder));
    }

    if cleaned.contains("Advanced Micro Devices")
        || cleaned.contains("[AMD/ATI]")
        || cleaned.starts_with("AMD ")
    {
        if let Some(radeon) = find_bracket_content_with_keyword(cleaned, "Radeon") {
            return collapse_whitespace(&format!("AMD {}", radeon));
        }

        let remainder = cleaned
            .replace("Advanced Micro Devices, Inc.", "")
            .replace("Advanced Micro Devices, Inc", "")
            .replace("[AMD/ATI]", "")
            .replace("AMD/ATI", "")
            .trim()
            .to_string();

        if remainder.is_empty() {
            return "AMD".to_string();
        }
        if remainder.starts_with("AMD ") {
            return collapse_whitespace(&remainder);
        }
        return collapse_whitespace(&format!("AMD {}", remainder));
    }

    if cleaned.contains("Intel Corporation") {
        let remainder = cleaned.replace("Intel Corporation", "").trim().to_string();
        if remainder.starts_with("Intel ") {
            return collapse_whitespace(&remainder);
        }
        return collapse_whitespace(&format!("Intel {}", remainder));
    }

    collapse_whitespace(cleaned)
}

/// Remove sufixo de revisão comum do `lspci`, ex: `(rev a1)`.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn strip_lspci_revision(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.ends_with(')') {
        if let Some(idx) = trimmed.rfind(" (rev ") {
            return trimmed[..idx].trim();
        }
    }
    trimmed
}

/// Busca conteúdo entre colchetes que contenha uma palavra-chave.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn find_bracket_content_with_keyword<'a>(
    text: &'a str,
    keyword: &str,
) -> Option<&'a str> {
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let after_start = &rest[start + 1..];
        let end = after_start.find(']')?;
        let content = after_start[..end].trim();
        if content.contains(keyword) {
            return Some(content);
        }
        rest = &after_start[end + 1..];
    }
    None
}

/// Colapsa whitespace duplicado.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

/// Parse tamanho de disco do `df` (ex: "1.8T", "3.9G", "100M").
#[cfg(target_os = "macos")]
pub(crate) fn parse_df_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let (value, multiplier) = if s.ends_with('T') || s.ends_with('t') {
        (
            s.trim_end_matches('T').trim_end_matches('t'),
            1024u64 * 1024 * 1024 * 1024,
        )
    } else if s.ends_with('G') || s.ends_with('g') {
        (
            s.trim_end_matches('G').trim_end_matches('g'),
            1024u64 * 1024 * 1024,
        )
    } else if s.ends_with('M') || s.ends_with('m') {
        (
            s.trim_end_matches('M').trim_end_matches('m'),
            1024u64 * 1024,
        )
    } else if s.ends_with('K') || s.ends_with('k') {
        (s.trim_end_matches('K').trim_end_matches('k'), 1024u64)
    } else {
        (s, 1u64)
    };

    value
        .parse::<f64>()
        .ok()
        .map(|v| (v * multiplier as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dpkg_query_installed_count_ignores_non_installed_statuses() {
        let output = r#"ii  bash:amd64
rc  old-package:amd64
un  missing-package
hi  held-unconfigured:amd64
ii  coreutils:amd64
"#;

        assert_eq!(parse_dpkg_query_installed_count(output), Some(2));
    }

    #[test]
    fn test_parse_xrandr_resolution_primary_monitor() {
        let xrandr_output = r#"Screen 0: minimum 8 x 8, current 3440 x 1440, maximum 32767 x 32767
HDMI-0 connected primary 3440x1440+0+0 (normal left inverted right x axis y axis) 800mm x 340mm
DP-0 disconnected (normal left inverted right x axis y axis)
"#;

        let resolution = parse_xrandr_resolution(xrandr_output);
        assert_eq!(resolution, Some("3440x1440".to_string()));
    }

    #[test]
    fn test_parse_xrandr_resolution_connected_non_primary_monitor() {
        let xrandr_output = r#"Screen 0: minimum 8 x 8, current 1920 x 1080, maximum 32767 x 32767
eDP-1 connected 1920x1080+0+0 (normal left inverted right x axis y axis) 309mm x 174mm
HDMI-1 disconnected (normal left inverted right x axis y axis)
"#;

        let resolution = parse_xrandr_resolution(xrandr_output);
        assert_eq!(resolution, Some("1920x1080".to_string()));
    }

    #[test]
    fn test_parse_xrandr_resolution_disconnected_or_unusable_output() {
        let xrandr_output = r#"Screen 0: minimum 8 x 8, current 0 x 0, maximum 32767 x 32767
HDMI-1 disconnected (normal left inverted right x axis y axis)
DP-1 connected (normal left inverted right x axis y axis)
   60.00
"#;

        let resolution = parse_xrandr_resolution(xrandr_output);
        assert_eq!(resolution, None);
    }

    #[test]
    fn test_parse_lspci_gpu_info_nvidia() {
        let lspci_output = r#"01:00.0 VGA compatible controller: NVIDIA Corporation GA106 [GeForce RTX 3060] (rev a1)
"#;

        let gpu = parse_lspci_gpu_info(lspci_output);
        assert_eq!(gpu, Some("NVIDIA GeForce RTX 3060".to_string()));
    }

    #[test]
    fn test_parse_lspci_gpu_info_amd_and_intel() {
        let lspci_output = r#"00:02.0 VGA compatible controller: Intel Corporation UHD Graphics 620 (rev 07)
01:00.0 Display controller: Advanced Micro Devices, Inc. [AMD/ATI] Navi 23 [Radeon RX 6600 XT] (rev c7)
"#;

        let gpu = parse_lspci_gpu_info(lspci_output);
        assert_eq!(
            gpu,
            Some("Intel UHD Graphics 620 / AMD Radeon RX 6600 XT".to_string())
        );
    }

    #[test]
    fn test_parse_gsettings_value_strips_single_quotes() {
        assert_eq!(
            parse_gsettings_value("'Adwaita'\n"),
            Some("Adwaita".to_string())
        );
        assert_eq!(
            parse_gsettings_value("'Cantarell 11'"),
            Some("Cantarell 11".to_string())
        );
    }

    #[test]
    fn test_parse_gsettings_value_rejects_empty_or_unusable_output() {
        assert_eq!(parse_gsettings_value("''"), None);
        assert_eq!(parse_gsettings_value("   "), None);
        assert_eq!(parse_gsettings_value("@as []"), None);
    }
}
