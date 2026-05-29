use std::collections::BTreeMap;
use sysinfo::{CpuRefreshKind, System};

/// Representação de um campo do sistema com label e valor.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct SystemField {
    pub label: String,
    pub value: String,
}

impl SystemField {
    #[allow(dead_code)]
    pub fn new(label: String, value: String) -> Self {
        Self { label, value }
    }
}

/// Snapshot das informações do sistema.
/// Usa BTreeMap para armazenar campos, mas a ordem de exibição é controlada
/// explicitamente em app.rs para preservar a ordem screenFetch-like.
#[derive(Debug, Clone, Default)]
pub struct SystemSnapshot {
    /// user@host (concatenação para exibição)
    pub user_host: String,
    /// Mapa de campos por label para ordenação consistente
    pub fields: BTreeMap<String, String>,
}

impl SystemSnapshot {
    /// Coleta informações do sistema com fallbacks gracefulls.
    pub fn collect() -> Self {
        let mut system = System::new();

        // Atualiza apenas CPU e memória para manter rápido
        system.refresh_cpu_specifics(CpuRefreshKind::new().with_frequency());
        system.refresh_memory();

        let user = env_or_fallback("USER", "unknown");
        let host = env_or_fallback("HOSTNAME", "unknown");
        let os = get_os();
        let kernel = get_kernel();
        let uptime = get_uptime();
        let shell = get_shell();
        let cpu = get_cpu_info(&system);
        let ram = get_ram_info(&system);
        let disk = get_disk_info();

        // Constrói o snapshot com todos os campos
        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), os.clone());
        fields.insert("Kernel".to_string(), kernel.clone());
        fields.insert("Uptime".to_string(), uptime.clone());
        fields.insert("Shell".to_string(), shell.clone());
        fields.insert("CPU".to_string(), cpu.clone());
        fields.insert("RAM".to_string(), ram.clone());
        fields.insert("Disk".to_string(), disk.clone());

        Self {
            user_host: format!("{}@{}", user, host),
            fields,
        }
    }

    /// Retorna o valor de um campo pelo label, ou "N/A" se não encontrado.
    pub fn get(&self, label: &str) -> String {
        self.fields
            .get(label)
            .cloned()
            .unwrap_or_else(|| "N/A".to_string())
    }

    /// Retorna true quando o campo existe no snapshot.
    pub fn has_field(&self, label: &str) -> bool {
        self.fields.contains_key(label)
    }

    /// Retorna todos os campos em ordem alfabética.
    #[allow(dead_code)]
    pub fn fields(&self) -> Vec<SystemField> {
        self.fields
            .iter()
            .map(|(k, v)| SystemField::new(k.clone(), v.clone()))
            .collect()
    }
}

/// Ordem screenFetch-like para full mode.
const FULL_FIELD_ORDER: [&str; 16] = [
    "OS",
    "Kernel",
    "Uptime",
    "Packages",
    "Shell",
    "Resolution",
    "DE",
    "WM",
    "WM Theme",
    "GTK Theme",
    "Icon Theme",
    "Font",
    "Disk",
    "CPU",
    "GPU",
    "RAM",
];

/// Ordem de campos para compact mode.
const COMPACT_FIELD_ORDER: [&str; 6] = ["OS", "Kernel", "Uptime", "Disk", "CPU", "RAM"];

/// Retorna uma variável de ambiente ou um fallback.
fn env_or_fallback(key: &str, fallback: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| {
        // Fallback específico para HOSTNAME no Linux
        if key == "HOSTNAME" {
            #[cfg(target_os = "linux")]
            {
                std::fs::read_to_string("/proc/sys/kernel/hostname")
                    .ok()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|| fallback.to_string())
            }
            #[cfg(not(target_os = "linux"))]
            {
                fallback.to_string()
            }
        } else {
            fallback.to_string()
        }
    })
}

/// Obtém o nome do sistema operacional.
fn get_os() -> String {
    // Tenta usar uname no Linux/macOS
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|l| l.starts_with("PRETTY_NAME="))
                    .map(|l| {
                        l.trim_start_matches("PRETTY_NAME=")
                            .trim_matches('"')
                            .to_string()
                    })
            })
            .unwrap_or_else(|| "Linux".to_string())
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sw_vers")
            .arg("-productName")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "macOS".to_string())
    }

    #[cfg(target_os = "windows")]
    {
        "Windows".to_string()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "Unknown OS".to_string()
    }
}

/// Obtém a versão do kernel.
fn get_kernel() -> String {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Linux".to_string())
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Darwin".to_string())
    }

    #[cfg(target_os = "windows")]
    {
        "Windows".to_string()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "Unknown".to_string()
    }
}

/// Obtém o uptime do sistema.
fn get_uptime() -> String {
    #[cfg(target_os = "linux")]
    {
        // Tenta ler /proc/uptime
        if let Ok(content) = std::fs::read_to_string("/proc/uptime") {
            if let Some(Ok(seconds)) = content.split_whitespace().next().map(|s| s.parse::<f64>()) {
                return format_uptime(seconds as u64);
            }
        }
        "N/A".to_string()
    }

    #[cfg(target_os = "macos")]
    {
        // Tenta usar sysctl
        if let Ok(output) = std::process::Command::new("sysctl")
            .arg("-n")
            .arg("kern.boottime")
            .output()
        {
            if let Ok(text) = String::from_utf8(output.stdout) {
                // Parse boottime para calcular uptime
                return "N/A".to_string(); // Simplificado para MVP
            }
        }
        "N/A".to_string()
    }

    #[cfg(target_os = "windows")]
    {
        "N/A".to_string()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "N/A".to_string()
    }
}

/// Formata segundos em formato legível.
fn format_uptime(seconds: u64) -> String {
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

/// Obtém o shell atual.
fn get_shell() -> String {
    // Tenta usar SHELL
    if let Ok(shell) = std::env::var("SHELL") {
        return shell.split('/').next_back().unwrap_or("shell").to_string();
    }

    // Tenta usar PATH
    if let Ok(path) = std::env::var("PATH") {
        if path.contains("zsh") {
            return "zsh".to_string();
        }
        if path.contains("bash") {
            return "bash".to_string();
        }
    }

    "N/A".to_string()
}

/// Obtém informações da CPU.
fn get_cpu_info(system: &System) -> String {
    if system.cpus().is_empty() {
        return "N/A".to_string();
    }

    let cpu = &system.cpus()[0];
    cpu.brand().to_string()
}

/// Obtém informações de RAM.
fn get_ram_info(system: &System) -> String {
    let total = system.total_memory();
    let available = system.available_memory();

    // Converte para GB
    let total_gb = total as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = (total - available) as f64 / (1024.0 * 1024.0 * 1024.0);

    format!("{:.1}GB / {:.1}GB", used_gb, total_gb)
}

/// Formata bytes em uma unidade apropriada (B, K, M, G, T).
fn format_bytes(bytes: u64) -> String {
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

/// Obtém informações de disco usando sysinfo.
fn get_disk_info() -> String {
    #[cfg(target_os = "linux")]
    {
        // Tenta obter informações do disco raiz
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let mut total_bytes: u64 = 0;
        let mut used_bytes: u64 = 0;

        for disk in disks.iter() {
            let disk_total = disk.total_space();
            let disk_available = disk.available_space();

            // Ignora discos com total zero (pseudo/empty entries)
            if disk_total == 0 {
                continue;
            }

            let disk_used = disk_total - disk_available;
            total_bytes += disk_total;
            used_bytes += disk_used;
        }

        // Se não encontrou discos válidos, retorna N/A
        if total_bytes == 0 {
            return "N/A".to_string();
        }

        let used_formatted = format_bytes(used_bytes);
        let total_formatted = format_bytes(total_bytes);

        let percent = if total_bytes > 0 {
            ((used_bytes as f64 / total_bytes as f64) * 100.0).round() as u8
        } else {
            0
        };

        format!("{} / {} ({}%)", used_formatted, total_formatted, percent)
    }

    #[cfg(target_os = "macos")]
    {
        // Tenta usar df para obter informações do disco raiz
        if let Ok(output) = std::process::Command::new("df").arg("-h").arg("/").output() {
            if let Ok(text) = String::from_utf8(output.stdout) {
                // Parse a saída do df
                let lines: Vec<&str> = text.lines().collect();
                if lines.len() >= 2 {
                    let parts: Vec<&str> = lines[1].split_whitespace().collect();
                    if parts.len() >= 4 {
                        let used = parts[2];
                        let total = parts[1];
                        // Tenta calcular percentual
                        if let Ok(used_val) = parse_df_size(used) {
                            if let Ok(total_val) = parse_df_size(total) {
                                if total_val > 0 {
                                    let percent = ((used_val as f64 / total_val as f64) * 100.0)
                                        .round()
                                        as u8;
                                    return format!("{} / {} ({}%)", used, total, percent);
                                }
                            }
                        }
                    }
                }
            }
        }
        "N/A".to_string()
    }

    #[cfg(target_os = "windows")]
    {
        // Tenta usar wmic para obter informações do disco
        if let Ok(output) = std::process::Command::new("wmic")
            .arg("logicaldisk")
            .arg("where")
            .arg("DeviceID='C:'")
            .arg("get")
            .arg("Size,FreeSpace")
            .output()
        {
            if let Ok(text) = String::from_utf8(output.stdout) {
                // Parse a saída
                let lines: Vec<&str> = text.lines().collect();
                if lines.len() >= 2 {
                    let parts: Vec<&str> = lines[1].split_whitespace().collect();
                    if parts.len() >= 2 {
                        let total = parts[0];
                        let free = parts[1];
                        if let (Ok(total_val), Ok(free_val)) =
                            (total.parse::<u64>(), free.parse::<u64>())
                        {
                            let used = total_val - free_val;
                            let total_gb = total_val as f64 / (1024.0 * 1024.0 * 1024.0);
                            let used_gb = used as f64 / (1024.0 * 1024.0 * 1024.0);
                            let percent = if total_val > 0 {
                                ((used as f64 / total_val as f64) * 100.0).round() as u8
                            } else {
                                0
                            };
                            return format!("{:.1}GB / {:.1}GB ({}%)", used_gb, total_gb, percent);
                        }
                    }
                }
            }
        }
        "N/A".to_string()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "N/A".to_string()
    }
}

/// Parse tamanho de disco do df (ex: "1.8T", "3.9G", "100M").
#[allow(dead_code)]
fn parse_df_size(s: &str) -> Option<u64> {
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

/// Retorna os nomes dos campos na ordem de exibição desejada.
/// Esta função define a ordem de campos para full mode.
pub fn get_field_order() -> Vec<&'static str> {
    FULL_FIELD_ORDER.to_vec()
}

/// Retorna os nomes dos campos para compact mode.
pub fn get_compact_field_order() -> Vec<&'static str> {
    COMPACT_FIELD_ORDER.to_vec()
}

/// Retorna os campos na ordem de exibição, omitindo campos ausentes em full mode.
pub fn get_display_field_order(system: &SystemSnapshot, compact: bool) -> Vec<&'static str> {
    if compact {
        return get_compact_field_order();
    }

    get_field_order()
        .into_iter()
        .filter(|field_name| system.has_field(field_name))
        .collect()
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
    fn test_get_field_order() {
        let order = get_field_order();
        assert_eq!(order[0], "OS");
        assert_eq!(order[1], "Kernel");
        assert_eq!(order[2], "Uptime");
        assert_eq!(order[3], "Packages");
        assert_eq!(order[4], "Shell");
        assert_eq!(order[5], "Resolution");
        assert_eq!(order[6], "DE");
        assert_eq!(order[7], "WM");
        assert_eq!(order[8], "WM Theme");
        assert_eq!(order[9], "GTK Theme");
        assert_eq!(order[10], "Icon Theme");
        assert_eq!(order[11], "Font");
        assert_eq!(order[12], "Disk");
        assert_eq!(order[13], "CPU");
        assert_eq!(order[14], "GPU");
        assert_eq!(order[15], "RAM");
    }

    #[test]
    fn test_get_compact_field_order() {
        let order = get_compact_field_order();
        assert_eq!(order, vec!["OS", "Kernel", "Uptime", "Disk", "CPU", "RAM"]);
    }

    #[test]
    fn test_get_display_field_order_omits_missing_advanced_fields() {
        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), "Linux".to_string());
        fields.insert("Kernel".to_string(), "6.x".to_string());
        fields.insert("Uptime".to_string(), "1h 2m".to_string());
        fields.insert("Shell".to_string(), "bash".to_string());
        fields.insert("Disk".to_string(), "1G/2G (50%)".to_string());
        fields.insert("CPU".to_string(), "Test CPU".to_string());
        fields.insert("RAM".to_string(), "1.0GB / 2.0GB".to_string());

        let snapshot = SystemSnapshot {
            user_host: "user@host".to_string(),
            fields,
        };

        let order = get_display_field_order(&snapshot, false);
        assert_eq!(
            order,
            vec!["OS", "Kernel", "Uptime", "Shell", "Disk", "CPU", "RAM"]
        );
    }

    #[test]
    fn test_get_display_field_order_keeps_future_order_when_present() {
        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), "Linux".to_string());
        fields.insert("Kernel".to_string(), "6.x".to_string());
        fields.insert("Uptime".to_string(), "1h 2m".to_string());
        fields.insert("Packages".to_string(), "1234".to_string());
        fields.insert("Shell".to_string(), "bash".to_string());
        fields.insert("Resolution".to_string(), "1920x1080".to_string());
        fields.insert("DE".to_string(), "GNOME".to_string());
        fields.insert("WM".to_string(), "Mutter".to_string());
        fields.insert("WM Theme".to_string(), "Adwaita".to_string());
        fields.insert("GTK Theme".to_string(), "Adwaita".to_string());
        fields.insert("Icon Theme".to_string(), "Adwaita".to_string());
        fields.insert("Font".to_string(), "Noto Sans 11".to_string());
        fields.insert("Disk".to_string(), "1G/2G (50%)".to_string());
        fields.insert("CPU".to_string(), "Test CPU".to_string());
        fields.insert("GPU".to_string(), "Test GPU".to_string());
        fields.insert("RAM".to_string(), "1.0GB / 2.0GB".to_string());

        let snapshot = SystemSnapshot {
            user_host: "user@host".to_string(),
            fields,
        };

        let order = get_display_field_order(&snapshot, false);
        assert_eq!(order, get_field_order());
    }

    #[test]
    fn test_system_snapshot_collect() {
        let snapshot = SystemSnapshot::collect();
        assert!(!snapshot.user_host.is_empty());
        assert!(snapshot.fields.contains_key("OS"));
        assert!(snapshot.fields.contains_key("Kernel"));
        assert!(snapshot.fields.contains_key("Uptime"));
        assert!(snapshot.fields.contains_key("Shell"));
        assert!(snapshot.fields.contains_key("Disk"));
        assert!(snapshot.fields.contains_key("CPU"));
        assert!(snapshot.fields.contains_key("RAM"));
    }

    #[test]
    fn test_system_snapshot_get() {
        let snapshot = SystemSnapshot::collect();
        assert!(!snapshot.get("OS").is_empty());
        assert_eq!(snapshot.get("NonExistent"), "N/A");
    }

    #[test]
    fn test_format_bytes_zero_total_does_not_panic() {
        // Testa que format_bytes não panica com valores extremos
        assert_eq!(format_bytes(u64::MAX), "16777216.0T");
    }
}
