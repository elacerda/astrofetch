/// Formata segundos em formato legível.
#[cfg(target_os = "linux")]
pub(crate) fn format_uptime(seconds: u64) -> String {
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;

    if days > 0 {
        format!("{}d {}h", days, hours % 24)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes % 60)
    } else {
        format!("{}m", minutes % 60)
    }
}

/// Formata bytes em uma unidade apropriada (B, K, M, G, T).
#[cfg(any(target_os = "linux", test))]
pub(crate) fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{}B", bytes);
    }
    let kib = bytes as f64 / 1024.0;
    if kib < 1024.0 {
        return format!("{:.1}K", kib);
    }
    let mib = kib / 1024.0;
    if mib < 1024.0 {
        return format!("{:.1}M", mib);
    }
    let gib = mib / 1024.0;
    if gib < 1024.0 {
        return format!("{:.1}G", gib);
    }
    let tib = gib / 1024.0;
    format!("{:.1}T", tib)
}

/// Normaliza uma string de desktop/session para exibição.
/// Remove sufixos comuns como "-session", "-wm", etc.
pub(crate) fn normalize_desktop_string(s: &str) -> String {
    let s = s.trim();

    // Tenta obter apenas o primeiro item se houver múltiplos DEs separados por :
    let s = s.split(':').next().unwrap_or(s);

    // Remove sufixos comuns
    let s = s
        .strip_suffix("-session")
        .unwrap_or(s)
        .strip_suffix("-wm")
        .unwrap_or(s)
        .strip_suffix("-session")
        .unwrap_or(s);

    // Converte para title case (primeira letra maiúscula, resto minúsculo)
    let mut result = String::new();
    let mut capitalize = true;
    for c in s.chars() {
        if capitalize {
            result.push(c.to_ascii_uppercase());
            capitalize = false;
        } else {
            result.push(c.to_ascii_lowercase());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes_bytes() {
        assert_eq!(format_bytes(0), "0B");
        assert_eq!(format_bytes(512), "512B");
        assert_eq!(format_bytes(1023), "1023B");
    }

    #[test]
    fn test_format_bytes_kilobytes() {
        assert_eq!(format_bytes(1024), "1.0K");
        assert_eq!(format_bytes(2048), "2.0K");
        assert_eq!(format_bytes(5120), "5.0K");
    }

    #[test]
    fn test_format_bytes_megabytes() {
        assert_eq!(format_bytes(1024 * 1024), "1.0M");
        assert_eq!(format_bytes(1024 * 1024 * 2), "2.0M");
        assert_eq!(format_bytes(1024 * 1024 * 512), "512.0M");
    }

    #[test]
    fn test_format_bytes_gigabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0G");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 2), "2.0G");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 10), "10.0G");
    }

    #[test]
    fn test_format_bytes_terabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.0T");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024 * 2), "2.0T");
    }

    #[test]
    fn test_format_bytes_mixed_values() {
        // Valores que não são exatamente potências de 1024
        assert_eq!(format_bytes(1500), "1.5K");
        assert_eq!(format_bytes(1_500_000), "1.4M");
        assert_eq!(format_bytes(1_500_000_000), "1.4G");
    }

    #[test]
    fn test_format_bytes_zero_total_does_not_panic() {
        // Testa que format_bytes não panica com valores extremos
        assert_eq!(format_bytes(u64::MAX), "16777216.0T");
    }

    #[test]
    fn test_normalize_desktop_string_basic() {
        assert_eq!(normalize_desktop_string("gnome"), "Gnome");
        assert_eq!(normalize_desktop_string("KDE"), "Kde");
        assert_eq!(normalize_desktop_string("xfce"), "Xfce");
    }

    #[test]
    fn test_normalize_desktop_string_with_suffix() {
        assert_eq!(normalize_desktop_string("gnome-session"), "Gnome");
        assert_eq!(normalize_desktop_string("plasma"), "Plasma");
        assert_eq!(normalize_desktop_string("plasma-desktop"), "Plasma-desktop");
    }

    #[test]
    fn test_normalize_desktop_string_multiple_de() {
        // XDG_CURRENT_DESKTOP pode conter múltiplos DEs separados por :
        assert_eq!(normalize_desktop_string("GNOME:XDG"), "Gnome");
        assert_eq!(normalize_desktop_string("ubuntu:GNOME"), "Ubuntu");
    }
}
