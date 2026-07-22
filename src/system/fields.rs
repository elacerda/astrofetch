use std::collections::BTreeMap;

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

/// Collection profile controlling which system collectors are invoked.
/// Full collects everything; Compact skips seven collectors whose output
/// is never rendered in compact mode. Both profiles still populate `user_host`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CollectionProfile {
    /// Collect all available system information.
    #[default]
    Full,
    /// Collect only OS, Kernel, Uptime, Disk, CPU, RAM (and user_host).
    /// Skips: Packages, Shell, Resolution, GPU, DE, WM, DesktopCosmetics.
    Compact,
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
    use std::collections::BTreeMap;

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
    fn test_get_display_field_order_compact_excludes_de_wm() {
        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), "Linux".to_string());
        fields.insert("Kernel".to_string(), "6.x".to_string());
        fields.insert("Uptime".to_string(), "1h 2m".to_string());
        fields.insert("Disk".to_string(), "1G/2G (50%)".to_string());
        fields.insert("CPU".to_string(), "Test CPU".to_string());
        fields.insert("RAM".to_string(), "1.0GB / 2.0GB".to_string());

        let snapshot = SystemSnapshot {
            user_host: "user@host".to_string(),
            fields,
        };

        let order = get_display_field_order(&snapshot, true);
        assert_eq!(order, vec!["OS", "Kernel", "Uptime", "Disk", "CPU", "RAM"]);
    }

    #[test]
    fn test_get_display_field_order_packages_present() {
        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), "Linux".to_string());
        fields.insert("Kernel".to_string(), "6.x".to_string());
        fields.insert("Uptime".to_string(), "1h 2m".to_string());
        fields.insert("Packages".to_string(), "1234".to_string());
        fields.insert("Shell".to_string(), "bash".to_string());
        fields.insert("Disk".to_string(), "1G/2G (50%)".to_string());
        fields.insert("CPU".to_string(), "Test CPU".to_string());
        fields.insert("RAM".to_string(), "1.0GB / 2.0GB".to_string());

        let snapshot = SystemSnapshot {
            user_host: "user@host".to_string(),
            fields,
        };

        let order = get_display_field_order(&snapshot, false);
        // Packages deve estar na posição correta (índice 3, após Uptime)
        assert_eq!(order[0], "OS");
        assert_eq!(order[1], "Kernel");
        assert_eq!(order[2], "Uptime");
        assert_eq!(order[3], "Packages");
        assert_eq!(order[4], "Shell");
    }

    #[test]
    fn test_get_display_field_order_compact_excludes_packages() {
        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), "Linux".to_string());
        fields.insert("Kernel".to_string(), "6.x".to_string());
        fields.insert("Uptime".to_string(), "1h 2m".to_string());
        fields.insert("Packages".to_string(), "1234".to_string());
        fields.insert("Disk".to_string(), "1G/2G (50%)".to_string());
        fields.insert("CPU".to_string(), "Test CPU".to_string());
        fields.insert("RAM".to_string(), "1.0GB / 2.0GB".to_string());

        let snapshot = SystemSnapshot {
            user_host: "user@host".to_string(),
            fields,
        };

        let order = get_display_field_order(&snapshot, true);
        // Compact mode deve excluir Packages
        assert_eq!(order, vec!["OS", "Kernel", "Uptime", "Disk", "CPU", "RAM"]);
    }

    #[test]
    fn test_get_display_field_order_positions_desktop_cosmetics() {
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
        fields.insert("GTK Theme".to_string(), "Yaru".to_string());
        fields.insert("Icon Theme".to_string(), "Yaru".to_string());
        fields.insert("Font".to_string(), "Cantarell 11".to_string());
        fields.insert("Disk".to_string(), "1G/2G (50%)".to_string());
        fields.insert("CPU".to_string(), "Test CPU".to_string());
        fields.insert("GPU".to_string(), "Test GPU".to_string());
        fields.insert("RAM".to_string(), "1.0GB / 2.0GB".to_string());

        let snapshot = SystemSnapshot {
            user_host: "user@host".to_string(),
            fields,
        };

        let order = get_display_field_order(&snapshot, false);
        assert_eq!(
            order,
            vec![
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
                "RAM"
            ]
        );
    }

    #[test]
    fn test_get_display_field_order_positions_resolution_and_gpu() {
        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), "Linux".to_string());
        fields.insert("Kernel".to_string(), "6.x".to_string());
        fields.insert("Uptime".to_string(), "1h 2m".to_string());
        fields.insert("Packages".to_string(), "1234".to_string());
        fields.insert("Shell".to_string(), "bash".to_string());
        fields.insert("Resolution".to_string(), "1920x1080".to_string());
        fields.insert("Disk".to_string(), "1G/2G (50%)".to_string());
        fields.insert("CPU".to_string(), "Test CPU".to_string());
        fields.insert("GPU".to_string(), "Test GPU".to_string());
        fields.insert("RAM".to_string(), "1.0GB / 2.0GB".to_string());

        let snapshot = SystemSnapshot {
            user_host: "user@host".to_string(),
            fields,
        };

        let order = get_display_field_order(&snapshot, false);
        let shell_idx = order.iter().position(|field| *field == "Shell").unwrap();
        let resolution_idx = order
            .iter()
            .position(|field| *field == "Resolution")
            .unwrap();
        let cpu_idx = order.iter().position(|field| *field == "CPU").unwrap();
        let gpu_idx = order.iter().position(|field| *field == "GPU").unwrap();

        assert_eq!(resolution_idx, shell_idx + 1);
        assert_eq!(gpu_idx, cpu_idx + 1);
    }

    #[test]
    fn test_get_display_field_order_compact_excludes_resolution_and_gpu() {
        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), "Linux".to_string());
        fields.insert("Kernel".to_string(), "6.x".to_string());
        fields.insert("Uptime".to_string(), "1h 2m".to_string());
        fields.insert("Resolution".to_string(), "1920x1080".to_string());
        fields.insert("WM Theme".to_string(), "Adwaita".to_string());
        fields.insert("GTK Theme".to_string(), "Yaru".to_string());
        fields.insert("Icon Theme".to_string(), "Yaru".to_string());
        fields.insert("Font".to_string(), "Cantarell 11".to_string());
        fields.insert("Disk".to_string(), "1G/2G (50%)".to_string());
        fields.insert("CPU".to_string(), "Test CPU".to_string());
        fields.insert("GPU".to_string(), "Test GPU".to_string());
        fields.insert("RAM".to_string(), "1.0GB / 2.0GB".to_string());

        let snapshot = SystemSnapshot {
            user_host: "user@host".to_string(),
            fields,
        };

        let order = get_display_field_order(&snapshot, true);
        assert_eq!(order, vec!["OS", "Kernel", "Uptime", "Disk", "CPU", "RAM"]);
        assert!(!order.contains(&"Resolution"));
        assert!(!order.contains(&"WM Theme"));
        assert!(!order.contains(&"GTK Theme"));
        assert!(!order.contains(&"Icon Theme"));
        assert!(!order.contains(&"Font"));
        assert!(!order.contains(&"GPU"));
    }
}
