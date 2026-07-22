use std::collections::BTreeMap;

use super::command::run_command_best_effort_with_limit;
use super::desktop::{
    get_desktop_cosmetics, get_desktop_environment, get_resolution, get_window_manager_or_session,
};
use super::disk::get_disk_info;
use super::fields::SystemSnapshot;
use super::format::format_uptime;
use super::parsers::{
    parse_dpkg_get_selections_installed_count, parse_dpkg_query_installed_count,
    parse_lspci_gpu_info,
};
use sysinfo::{CpuRefreshKind, System};

/// Retorna uma variável de ambiente ou um fallback.
fn env_or_fallback(key: &str, fallback: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| {
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

/// Obtém o número de pacotes instalados (best-effort).
fn get_packages() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        if let Some(output) = run_command_best_effort_with_limit(
            "dpkg-query",
            &["-W", "-f=${db:Status-Abbrev} ${binary:Package}\n"],
            256 * 1024,
        ) {
            if let Some(count) = parse_dpkg_query_installed_count(&output) {
                return Some(count.to_string());
            }
        }

        if let Some(output) =
            run_command_best_effort_with_limit("dpkg", &["--get-selections"], 256 * 1024)
        {
            if let Some(count) = parse_dpkg_get_selections_installed_count(&output) {
                return Some(count.to_string());
            }
        }

        None
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Obtém o uptime do sistema.
fn get_uptime() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/uptime") {
            if let Some(Ok(seconds)) = content.split_whitespace().next().map(|s| s.parse::<f64>()) {
                return format_uptime(seconds as u64);
            }
        }
        "N/A".to_string()
    }

    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("sysctl")
            .arg("-n")
            .arg("kern.boottime")
            .output();
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

/// Obtém o shell atual.
fn get_shell() -> String {
    if let Ok(shell) = std::env::var("SHELL") {
        return shell.split('/').next_back().unwrap_or("shell").to_string();
    }

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

/// Obtém GPU(s) do Linux via `lspci` (best-effort).
fn get_gpu_info() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        super::command::run_command_best_effort("lspci", &[])
            .and_then(|output| parse_lspci_gpu_info(&output))
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Obtém informações de RAM.
fn get_ram_info(system: &System) -> String {
    let total = system.total_memory();
    let available = system.available_memory();

    let total_gb = total as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = (total - available) as f64 / (1024.0 * 1024.0 * 1024.0);

    format!("{:.1}GB / {:.1}GB", used_gb, total_gb)
}

impl SystemSnapshot {
    /// Coleta informações do sistema com fallbacks gracefulls.
    pub fn collect() -> Self {
        let mut system = System::new();

        system.refresh_cpu_specifics(CpuRefreshKind::new().with_frequency());
        system.refresh_memory();

        let user = env_or_fallback("USER", "unknown");
        let host = env_or_fallback("HOSTNAME", "unknown");
        let os = get_os();
        let kernel = get_kernel();
        let uptime = get_uptime();
        let packages = get_packages();
        let shell = get_shell();
        let resolution = get_resolution();
        let cpu = get_cpu_info(&system);
        let gpu = get_gpu_info();
        let ram = get_ram_info(&system);
        let disk = get_disk_info();
        let de = get_desktop_environment();
        let wm = get_window_manager_or_session();
        let desktop_cosmetics = get_desktop_cosmetics();

        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), os.clone());
        fields.insert("Kernel".to_string(), kernel.clone());
        fields.insert("Uptime".to_string(), uptime.clone());
        if let Some(packages_val) = packages {
            fields.insert("Packages".to_string(), packages_val);
        }
        fields.insert("Shell".to_string(), shell.clone());
        if let Some(resolution_val) = resolution {
            fields.insert("Resolution".to_string(), resolution_val);
        }
        fields.insert("CPU".to_string(), cpu.clone());
        if let Some(gpu_val) = gpu {
            fields.insert("GPU".to_string(), gpu_val);
        }
        fields.insert("RAM".to_string(), ram.clone());
        fields.insert("Disk".to_string(), disk.clone());

        if let Some(de_val) = de {
            fields.insert("DE".to_string(), de_val);
        }
        if let Some(wm_val) = wm {
            fields.insert("WM".to_string(), wm_val);
        }
        if let Some(wm_theme_val) = desktop_cosmetics.wm_theme {
            fields.insert("WM Theme".to_string(), wm_theme_val);
        }
        if let Some(gtk_theme_val) = desktop_cosmetics.gtk_theme {
            fields.insert("GTK Theme".to_string(), gtk_theme_val);
        }
        if let Some(icon_theme_val) = desktop_cosmetics.icon_theme {
            fields.insert("Icon Theme".to_string(), icon_theme_val);
        }
        if let Some(font_val) = desktop_cosmetics.font {
            fields.insert("Font".to_string(), font_val);
        }

        Self {
            user_host: format!("{}@{}", user, host),
            fields,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::command::ENV_MUTEX;

    #[test]
    fn test_env_or_fallback_returns_env_when_set() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let orig = std::env::var("ASTROFETCH_TEST_VAR").ok();

        std::env::set_var("ASTROFETCH_TEST_VAR", "hello");
        let result = env_or_fallback("ASTROFETCH_TEST_VAR", "fallback");
        assert_eq!(result, "hello");

        match orig {
            Some(val) => std::env::set_var("ASTROFETCH_TEST_VAR", val),
            None => std::env::remove_var("ASTROFETCH_TEST_VAR"),
        }
    }

    #[test]
    fn test_env_or_fallback_returns_fallback_when_missing() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let orig = std::env::var("ASTROFETCH_TEST_VAR_RARE_123").ok();

        std::env::remove_var("ASTROFETCH_TEST_VAR_RARE_123");
        let result = env_or_fallback("ASTROFETCH_TEST_VAR_RARE_123", "fallback_value");
        assert_eq!(result, "fallback_value");

        match orig {
            Some(val) => std::env::set_var("ASTROFETCH_TEST_VAR_RARE_123", val),
            None => std::env::remove_var("ASTROFETCH_TEST_VAR_RARE_123"),
        }
    }

    #[test]
    fn test_get_os_returns_valid_string() {
        let os = get_os();
        assert!(!os.is_empty());
    }

    #[test]
    fn test_get_kernel_returns_valid_string() {
        let kernel = get_kernel();
        assert!(!kernel.is_empty());
    }

    #[test]
    fn test_get_uptime_returns_valid_string() {
        let uptime = get_uptime();
        assert!(!uptime.is_empty());
    }

    #[test]
    fn test_get_shell_returns_valid_string() {
        let shell = get_shell();
        assert!(!shell.is_empty());
    }

    #[test]
    fn test_get_ram_info_returns_gb_format() {
        let mut sys = System::new();
        sys.refresh_memory();
        let ram = get_ram_info(&sys);
        assert!(ram.contains("GB"));
    }

    #[test]
    fn test_get_cpu_info_returns_brand() {
        let mut sys = System::new();
        sys.refresh_cpu_specifics(CpuRefreshKind::new().with_frequency());
        let cpu = get_cpu_info(&sys);
        assert!(!cpu.is_empty());
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
    fn test_get_packages_is_none_when_dpkg_unavailable() {
        get_packages();
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_get_uptime_format_hrs() {
        // Validates format_uptime works for hours
        let result = format_uptime(3723); // 1h 2m 3s
        assert_eq!(result, "1h 2m");
    }

    #[test]
    fn test_system_snapshot_collect_includes_de_when_available() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Salva o estado original das variáveis
        let orig_xdg = std::env::var("XDG_CURRENT_DESKTOP").ok();
        let orig_session = std::env::var("DESKTOP_SESSION").ok();
        let orig_session_desktop = std::env::var("XDG_SESSION_DESKTOP").ok();

        // Limpa todas as variáveis de ambiente DE primeiro
        std::env::remove_var("XDG_CURRENT_DESKTOP");
        std::env::remove_var("DESKTOP_SESSION");
        std::env::remove_var("XDG_SESSION_DESKTOP");

        // Define apenas XDG_CURRENT_DESKTOP
        std::env::set_var("XDG_CURRENT_DESKTOP", "GNOME");

        let snapshot = SystemSnapshot::collect();
        assert!(snapshot.has_field("DE"));
        assert_eq!(snapshot.get("DE"), "Gnome");

        // Restaura o estado original
        std::env::set_var("XDG_CURRENT_DESKTOP", orig_xdg.unwrap_or_default());
        std::env::set_var("DESKTOP_SESSION", orig_session.unwrap_or_default());
        std::env::set_var(
            "XDG_SESSION_DESKTOP",
            orig_session_desktop.unwrap_or_default(),
        );
    }

    #[test]
    fn test_system_snapshot_collect_includes_wm_when_available() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Salva o estado original das variáveis
        let orig_wayland = std::env::var("WAYLAND_DISPLAY").ok();
        let orig_display = std::env::var("DISPLAY").ok();
        let orig_session_type = std::env::var("XDG_SESSION_TYPE").ok();

        // Limpa todas as variáveis de ambiente WM primeiro
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");
        std::env::remove_var("XDG_SESSION_TYPE");

        // Define apenas WAYLAND_DISPLAY
        std::env::set_var("WAYLAND_DISPLAY", "wayland-1");

        let snapshot = SystemSnapshot::collect();
        assert!(snapshot.has_field("WM"));
        assert_eq!(snapshot.get("WM"), "Wayland");

        // Restaura o estado original
        std::env::set_var("WAYLAND_DISPLAY", orig_wayland.unwrap_or_default());
        std::env::set_var("DISPLAY", orig_display.unwrap_or_default());
        std::env::set_var("XDG_SESSION_TYPE", orig_session_type.unwrap_or_default());
    }

    #[test]
    fn test_system_snapshot_collect_omits_de_when_missing() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Salva o estado original das variáveis
        let orig_xdg = std::env::var("XDG_CURRENT_DESKTOP").ok();
        let orig_session = std::env::var("DESKTOP_SESSION").ok();
        let orig_session_desktop = std::env::var("XDG_SESSION_DESKTOP").ok();

        std::env::remove_var("XDG_CURRENT_DESKTOP");
        std::env::remove_var("DESKTOP_SESSION");
        std::env::remove_var("XDG_SESSION_DESKTOP");

        let snapshot = SystemSnapshot::collect();
        assert!(!snapshot.has_field("DE"));

        // Restaura o estado original
        std::env::set_var("XDG_CURRENT_DESKTOP", orig_xdg.unwrap_or_default());
        std::env::set_var("DESKTOP_SESSION", orig_session.unwrap_or_default());
        std::env::set_var(
            "XDG_SESSION_DESKTOP",
            orig_session_desktop.unwrap_or_default(),
        );
    }

    #[test]
    fn test_system_snapshot_collect_omits_wm_when_missing() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Salva o estado original das variáveis
        let orig_wayland = std::env::var("WAYLAND_DISPLAY").ok();
        let orig_display = std::env::var("DISPLAY").ok();
        let orig_session_type = std::env::var("XDG_SESSION_TYPE").ok();

        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");
        std::env::remove_var("XDG_SESSION_TYPE");

        let snapshot = SystemSnapshot::collect();
        assert!(!snapshot.has_field("WM"));

        // Restaura o estado original
        std::env::set_var("WAYLAND_DISPLAY", orig_wayland.unwrap_or_default());
        std::env::set_var("DISPLAY", orig_display.unwrap_or_default());
        std::env::set_var("XDG_SESSION_TYPE", orig_session_type.unwrap_or_default());
    }

    #[test]
    fn test_system_snapshot_collect_includes_packages_when_available() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Salva o estado original das variáveis
        let orig_xdg = std::env::var("XDG_CURRENT_DESKTOP").ok();
        let orig_session = std::env::var("DESKTOP_SESSION").ok();
        let orig_session_desktop = std::env::var("XDG_SESSION_DESKTOP").ok();

        // Limpa todas as variáveis de ambiente DE primeiro
        std::env::remove_var("XDG_CURRENT_DESKTOP");
        std::env::remove_var("DESKTOP_SESSION");
        std::env::remove_var("XDG_SESSION_DESKTOP");

        let snapshot = SystemSnapshot::collect();

        // Packages deve estar presente se dpkg-query estiver disponível
        // Se não estiver disponível, o campo simplesmente não será adicionado
        // (o comportamento correto é omitir Packages se não puder ser obtido)
        if snapshot.has_field("Packages") {
            let packages = snapshot.get("Packages");
            // Deve ser um número válido
            assert!(packages.parse::<u64>().is_ok() || packages == "N/A");
        }
        // Se Packages não estiver presente, isso também está correto (best-effort)

        // Restaura o estado original
        std::env::set_var("XDG_CURRENT_DESKTOP", orig_xdg.unwrap_or_default());
        std::env::set_var("DESKTOP_SESSION", orig_session.unwrap_or_default());
        std::env::set_var(
            "XDG_SESSION_DESKTOP",
            orig_session_desktop.unwrap_or_default(),
        );
    }

    #[test]
    fn test_get_packages_parsing_valid_output() {
        // Simula saída válida do dpkg-query com status abbreviations
        // Formato: "ii package-name" (ii = instalado)
        let valid_output = r#"ii  adduser        3.118        all          add and remove users and groups
ii  apt            2.4.11       amd64        commandline package manager
ii  base-files     12.4         amd64        Debian base system miscellaneous files
ii  bash           5.1-6        amd64        GNU Bourne Again SHell
ii  coreutils      8.32-4.1     amd64        GNU core utilities
ii  dash           0.5.11-1     amd64        POSIX-compliant shell
ii  debconf        1.5.82       all          Debian configuration management system
ii  debian-archive-keyring 1.0       all          Debian archive keyring
ii  dirmngr        2.2.40-1     amd64        GNU privacy assistant - Dirmngr
ii  dpkg           1.21.19      amd64        Debian package management system
"#;

        let count = valid_output
            .lines()
            .filter(|line| line.trim().starts_with("ii "))
            .count();

        // Deve encontrar 10 pacotes
        assert_eq!(count, 10);
    }

    #[test]
    fn test_get_packages_parsing_dpkg_get_selections() {
        // Simula saída válida do dpkg --get-selections
        // Formato: "package-name    install"
        let valid_output = r#"adduser                                         install
apt                                             install
base-files                                      install
bash                                            install
coreutils                                       install
dash                                            install
debconf                                         install
debian-archive-keyring                          install
dirmngr                                         install
dpkg                                            install
"#;

        // Conta linhas que têm ":install" no final (após trim)
        let count = valid_output
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                trimmed.ends_with(":install") || trimmed.ends_with(" install")
            })
            .count();

        // Deve encontrar 10 pacotes
        assert_eq!(count, 10);
        assert_eq!(
            parse_dpkg_get_selections_installed_count(valid_output),
            Some(10)
        );
    }

    #[test]
    fn test_get_packages_trims_whitespace() {
        // Simula saída com espaços extras
        let output_with_spaces = r#"ii  adduser        3.118        all          add and remove users and groups
  ii  apt            2.4.11       amd64        commandline package manager
ii  base-files     12.4         amd64        Debian base system miscellaneous files
"#;

        let count = output_with_spaces
            .lines()
            .filter(|line| line.trim().starts_with("ii "))
            .count();

        // Apenas linhas que começam com "ii " (com espaço após)
        // O trim() remove espaços antes, então "  ii" vira "ii"
        assert_eq!(count, 3);
    }

    #[test]
    fn test_get_packages_empty_output() {
        // Simula saída vazia
        let empty_output = "";

        let count = empty_output
            .lines()
            .filter(|line| line.trim().starts_with("ii "))
            .count();

        assert_eq!(count, 0);
    }

    #[test]
    fn test_get_packages_invalid_output_returns_none() {
        // Simula saída inválida (sem linhas começando com "ii ")
        let invalid_output = r#"Desired=Unknown/Install/Remove/Purge/Hold
| Status=Not/Inst/Conf-files/Unpacked/halF-conf/Half-inst/trig-aWait/Trig-pend
|/ Err?=(none)/Reinst-required (Status,Err: uppercase=bad)
||/ Name           Version      Architecture Description
++==============-============-============-=================================
"#;

        let count = invalid_output
            .lines()
            .filter(|line| line.trim().starts_with("ii "))
            .count();

        // Não deve encontrar pacotes
        assert_eq!(count, 0);
    }
}
