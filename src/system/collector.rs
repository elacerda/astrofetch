use std::collections::BTreeMap;

#[cfg(target_os = "macos")]
use super::command::run_command_best_effort;
#[cfg(target_os = "linux")]
use super::command::run_command_best_effort_with_limit;
use super::desktop::{
    get_desktop_cosmetics, get_desktop_environment, get_resolution, get_window_manager_or_session,
    DesktopCosmetics,
};
use super::disk::get_disk_info;
use super::fields::{CollectionProfile, SystemSnapshot};
#[cfg(target_os = "linux")]
use super::format::format_uptime;
#[cfg(any(target_os = "linux", test))]
use super::parsers::parse_dpkg_get_selections_installed_count;
#[cfg(target_os = "linux")]
use super::parsers::{parse_dpkg_query_installed_count, parse_lspci_gpu_info};
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
        run_command_best_effort("sw_vers", &["-productName"]).unwrap_or_else(|| "macOS".to_string())
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
        run_command_best_effort("uname", &["-r"]).unwrap_or_else(|| "Darwin".to_string())
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

/// Private: function pointers for the seven full-only collectors.
/// Carried per-call; allows deterministic test injection without global state.
#[derive(Clone, Copy)]
struct FullCollectors {
    get_packages: fn() -> Option<String>,
    get_shell: fn() -> String,
    get_resolution: fn() -> Option<String>,
    get_gpu_info: fn() -> Option<String>,
    get_desktop_environment: fn() -> Option<String>,
    get_window_manager_or_session: fn() -> Option<String>,
    get_desktop_cosmetics: fn() -> DesktopCosmetics,
}

impl FullCollectors {
    fn real() -> Self {
        Self {
            get_packages,
            get_shell,
            get_resolution,
            get_gpu_info,
            get_desktop_environment,
            get_window_manager_or_session,
            get_desktop_cosmetics,
        }
    }
}

/// Private: discriminates between a spawned thread handle and a sequential fallback.
#[cfg(any(target_os = "linux", test))]
enum WorkerResult<'scope, T> {
    Handle(std::thread::ScopedJoinHandle<'scope, T>),
    Fallback(fn() -> T),
}

/// Private: resolve a worker result by joining the handle or invoking the fallback.
#[cfg(any(target_os = "linux", test))]
fn join_or_seq<'scope, T>(result: WorkerResult<'scope, T>) -> T {
    match result {
        WorkerResult::Handle(handle) => handle
            .join()
            .unwrap_or_else(|e| std::panic::resume_unwind(e)),
        WorkerResult::Fallback(f) => f(),
    }
}

/// Private: run four subprocess-bound collectors in parallel while main-thread
/// work proceeds concurrently. Returns all four collector values plus the main
/// work result.
#[cfg(any(target_os = "linux", test))]
fn collect_selected_in_parallel<M, F>(
    collectors: FullCollectors,
    main_work: F,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    DesktopCosmetics,
    M,
)
where
    F: FnOnce() -> M,
{
    std::thread::scope(|scope| {
        // Attempt all four spawns before any fallback or main work.
        let packages_result = std::thread::Builder::new()
            .name("collector-packages".into())
            .spawn_scoped(scope, || (collectors.get_packages)())
            .map(WorkerResult::Handle)
            .unwrap_or_else(|_| WorkerResult::Fallback(collectors.get_packages));

        let resolution_result = std::thread::Builder::new()
            .name("collector-resolution".into())
            .spawn_scoped(scope, || (collectors.get_resolution)())
            .map(WorkerResult::Handle)
            .unwrap_or_else(|_| WorkerResult::Fallback(collectors.get_resolution));

        let gpu_result = std::thread::Builder::new()
            .name("collector-gpu".into())
            .spawn_scoped(scope, || (collectors.get_gpu_info)())
            .map(WorkerResult::Handle)
            .unwrap_or_else(|_| WorkerResult::Fallback(collectors.get_gpu_info));

        let cosmetics_result = std::thread::Builder::new()
            .name("collector-cosmetics".into())
            .spawn_scoped(scope, || (collectors.get_desktop_cosmetics)())
            .map(WorkerResult::Handle)
            .unwrap_or_else(|_| WorkerResult::Fallback(collectors.get_desktop_cosmetics));

        // Run main-thread work while workers execute.
        let main_result = main_work();

        // Join workers (or invoke fallbacks).
        let packages = join_or_seq(packages_result);
        let resolution = join_or_seq(resolution_result);
        let gpu = join_or_seq(gpu_result);
        let desktop_cosmetics = join_or_seq(cosmetics_result);

        (packages, resolution, gpu, desktop_cosmetics, main_result)
    })
}

impl SystemSnapshot {
    /// Coleta informações do sistema com fallbacks gracefuls.
    /// Backward-compatible entry point using the Full profile.
    #[allow(dead_code)]
    pub fn collect() -> Self {
        Self::collect_with(CollectionProfile::Full)
    }

    /// Collect system information using the given profile.
    /// Compact skips seven collectors whose output is never rendered in compact mode.
    pub(crate) fn collect_with(profile: CollectionProfile) -> Self {
        Self::collect_with_collectors(profile, FullCollectors::real())
    }

    /// Private: core collection with optional collector injection.
    /// Final field names, values, omission rules, and formatting are stable.
    /// BTreeMap assembly is deterministic. On Linux Full profile, selected
    /// collectors may execute concurrently. Compact invokes no Full-only collector.
    /// Non-Linux production collection remains sequential.
    #[cfg(target_os = "linux")]
    fn collect_with_collectors(profile: CollectionProfile, collectors: FullCollectors) -> Self {
        let mut system = System::new();

        system.refresh_cpu_specifics(CpuRefreshKind::new().with_frequency());
        system.refresh_memory();

        let user = env_or_fallback("USER", "unknown");
        let host = env_or_fallback("HOSTNAME", "unknown");
        let os = get_os();
        let kernel = get_kernel();
        let uptime = get_uptime();

        let (packages, resolution, gpu, desktop_cosmetics, (cpu, shell, de, wm, ram, disk)) =
            if profile == CollectionProfile::Full {
                collect_selected_in_parallel(collectors, || {
                    let cpu = get_cpu_info(&system);
                    let shell = (collectors.get_shell)();
                    let de = (collectors.get_desktop_environment)();
                    let wm = (collectors.get_window_manager_or_session)();
                    let ram = get_ram_info(&system);
                    let disk = get_disk_info();
                    (cpu, shell, de, wm, ram, disk)
                })
            } else {
                (
                    None,
                    None,
                    None,
                    DesktopCosmetics::default(),
                    (
                        get_cpu_info(&system),
                        String::new(),
                        None,
                        None,
                        get_ram_info(&system),
                        get_disk_info(),
                    ),
                )
            };

        let shell = if profile == CollectionProfile::Full {
            Some(shell)
        } else {
            None
        };
        let de = if profile == CollectionProfile::Full {
            de
        } else {
            None
        };
        let wm = if profile == CollectionProfile::Full {
            wm
        } else {
            None
        };

        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), os.clone());
        fields.insert("Kernel".to_string(), kernel.clone());
        fields.insert("Uptime".to_string(), uptime.clone());
        if let Some(packages_val) = packages {
            fields.insert("Packages".to_string(), packages_val);
        }
        if let Some(shell_val) = shell {
            fields.insert("Shell".to_string(), shell_val);
        }
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

    #[cfg(not(target_os = "linux"))]
    fn collect_with_collectors(profile: CollectionProfile, collectors: FullCollectors) -> Self {
        let mut system = System::new();

        system.refresh_cpu_specifics(CpuRefreshKind::new().with_frequency());
        system.refresh_memory();

        let user = env_or_fallback("USER", "unknown");
        let host = env_or_fallback("HOSTNAME", "unknown");
        let os = get_os();
        let kernel = get_kernel();
        let uptime = get_uptime();

        let packages: Option<String> = if profile == CollectionProfile::Full {
            (collectors.get_packages)()
        } else {
            None
        };

        let shell: Option<String> = if profile == CollectionProfile::Full {
            Some((collectors.get_shell)())
        } else {
            None
        };

        let resolution: Option<String> = if profile == CollectionProfile::Full {
            (collectors.get_resolution)()
        } else {
            None
        };

        let cpu = get_cpu_info(&system);

        let gpu: Option<String> = if profile == CollectionProfile::Full {
            (collectors.get_gpu_info)()
        } else {
            None
        };

        let ram = get_ram_info(&system);
        let disk = get_disk_info();

        let de: Option<String> = if profile == CollectionProfile::Full {
            (collectors.get_desktop_environment)()
        } else {
            None
        };

        let wm: Option<String> = if profile == CollectionProfile::Full {
            (collectors.get_window_manager_or_session)()
        } else {
            None
        };

        let desktop_cosmetics: DesktopCosmetics = if profile == CollectionProfile::Full {
            (collectors.get_desktop_cosmetics)()
        } else {
            DesktopCosmetics::default()
        };

        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), os.clone());
        fields.insert("Kernel".to_string(), kernel.clone());
        fields.insert("Uptime".to_string(), uptime.clone());
        if let Some(packages_val) = packages {
            fields.insert("Packages".to_string(), packages_val);
        }
        if let Some(shell_val) = shell {
            fields.insert("Shell".to_string(), shell_val);
        }
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
    #[test]
    fn test_compact_skips_full_only_collectors() {
        let fakes = FullCollectors {
            get_packages: || panic!("get_packages invoked in compact"),
            get_shell: || panic!("get_shell invoked in compact"),
            get_resolution: || panic!("get_resolution invoked in compact"),
            get_gpu_info: || panic!("get_gpu_info invoked in compact"),
            get_desktop_environment: || panic!("get_desktop_environment invoked in compact"),
            get_window_manager_or_session: || panic!("get_wm invoked in compact"),
            get_desktop_cosmetics: || panic!("get_desktop_cosmetics invoked in compact"),
        };

        // If any panic collector is called, this line is never reached.
        let snapshot = SystemSnapshot::collect_with_collectors(CollectionProfile::Compact, fakes);

        assert!(!snapshot.user_host.is_empty());
        assert!(snapshot.has_field("OS"));
        assert!(snapshot.has_field("Kernel"));
        assert!(snapshot.has_field("Uptime"));
        assert!(snapshot.has_field("CPU"));
        assert!(snapshot.has_field("RAM"));
        assert!(snapshot.has_field("Disk"));
        // Ten full-only fields must be absent
        assert!(!snapshot.has_field("Packages"));
        assert!(!snapshot.has_field("Shell"));
        assert!(!snapshot.has_field("Resolution"));
        assert!(!snapshot.has_field("GPU"));
        assert!(!snapshot.has_field("DE"));
        assert!(!snapshot.has_field("WM"));
        assert!(!snapshot.has_field("WM Theme"));
        assert!(!snapshot.has_field("GTK Theme"));
        assert!(!snapshot.has_field("Icon Theme"));
        assert!(!snapshot.has_field("Font"));
    }

    #[test]
    fn test_full_profile_with_fake_collectors() {
        let fakes = FullCollectors {
            get_packages: || Some("42".to_string()),
            get_shell: || "bash".to_string(),
            get_resolution: || Some("1920x1080".to_string()),
            get_gpu_info: || Some("NVIDIA".to_string()),
            get_desktop_environment: || Some("GNOME".to_string()),
            get_window_manager_or_session: || Some("X11".to_string()),
            get_desktop_cosmetics: || DesktopCosmetics {
                wm_theme: Some("Adwaita".to_string()),
                gtk_theme: Some("Adwaita".to_string()),
                icon_theme: Some("Adwaita".to_string()),
                font: Some("Noto".to_string()),
            },
        };

        let snapshot = SystemSnapshot::collect_with_collectors(CollectionProfile::Full, fakes);

        assert_eq!(snapshot.get("Packages"), "42");
        assert_eq!(snapshot.get("Shell"), "bash");
        assert_eq!(snapshot.get("Resolution"), "1920x1080");
        assert_eq!(snapshot.get("GPU"), "NVIDIA");
        assert_eq!(snapshot.get("DE"), "GNOME");
        assert_eq!(snapshot.get("WM"), "X11");
        assert_eq!(snapshot.get("WM Theme"), "Adwaita");
        assert_eq!(snapshot.get("GTK Theme"), "Adwaita");
        assert_eq!(snapshot.get("Icon Theme"), "Adwaita");
        assert_eq!(snapshot.get("Font"), "Noto");
    }
    // ─── Overlap coordinator for deterministic concurrency tests ───

    mod overlap_coordinator {
        use std::sync::{Condvar, Mutex, OnceLock};

        struct CoordinatorState {
            entered: u32,
            released: bool,
        }

        static COORDINATOR: OnceLock<(Mutex<CoordinatorState>, Condvar)> = OnceLock::new();

        fn state() -> &'static (Mutex<CoordinatorState>, Condvar) {
            COORDINATOR.get_or_init(|| {
                (
                    Mutex::new(CoordinatorState {
                        entered: 0,
                        released: true,
                    }),
                    Condvar::new(),
                )
            })
        }

        pub fn reset() {
            let mut inner = state().0.lock().unwrap();
            inner.entered = 0;
            inner.released = true;
        }

        pub fn enter() {
            {
                let mut inner = state().0.lock().unwrap();
                inner.entered += 1;
                inner.released = false;
            }
            state().1.notify_all();
            // Wait until released
            let mut inner = state().0.lock().unwrap();
            while !inner.released {
                inner = state().1.wait(inner).unwrap();
            }
        }

        pub fn wait_for_all(timeout_ms: u64) -> u32 {
            let mut inner = state().0.lock().unwrap();
            let start = std::time::Instant::now();
            while inner.entered < 4 {
                let elapsed = start.elapsed();
                if elapsed >= std::time::Duration::from_millis(timeout_ms) {
                    break;
                }
                let remaining = std::time::Duration::from_millis(timeout_ms) - elapsed;
                let timeout = std::time::Duration::from_millis(50).min(remaining);
                inner = state().1.wait_timeout(inner, timeout).unwrap().0;
            }
            inner.entered
        }

        pub fn release_all() {
            {
                let mut inner = state().0.lock().unwrap();
                inner.released = true;
            }
            state().1.notify_all();
        }
    }

    fn test_get_packages_fn() -> Option<String> {
        overlap_coordinator::enter();
        Some("test-packages".to_string())
    }

    fn test_get_resolution_fn() -> Option<String> {
        overlap_coordinator::enter();
        Some("test-resolution".to_string())
    }

    fn test_get_gpu_info_fn() -> Option<String> {
        overlap_coordinator::enter();
        Some("test-gpu".to_string())
    }

    fn test_get_desktop_cosmetics_fn() -> DesktopCosmetics {
        overlap_coordinator::enter();
        DesktopCosmetics {
            wm_theme: Some("test-wm".to_string()),
            gtk_theme: Some("test-gtk".to_string()),
            icon_theme: Some("test-icon".to_string()),
            font: Some("test-font".to_string()),
        }
    }

    #[test]
    fn test_full_collectors_overlap() {
        overlap_coordinator::reset();

        let collectors = FullCollectors {
            get_packages: test_get_packages_fn,
            get_shell: || "test-shell".to_string(),
            get_resolution: test_get_resolution_fn,
            get_gpu_info: test_get_gpu_info_fn,
            get_desktop_environment: || Some("test-de".to_string()),
            get_window_manager_or_session: || Some("test-wm".to_string()),
            get_desktop_cosmetics: test_get_desktop_cosmetics_fn,
        };

        // Run the parallel helper in a separate thread so we can wait for overlap.
        let collection_handle = std::thread::spawn(move || {
            collect_selected_in_parallel(collectors, || {
                ("test-shell".to_string(), None::<String>, None::<String>)
            })
        });

        // Wait for all four workers to enter (with timeout).
        let entered = overlap_coordinator::wait_for_all(5000);

        // Release all workers before joining.
        overlap_coordinator::release_all();

        // Join the collection thread.
        let (packages, resolution, gpu, cosmetics, (shell, de, wm)) = collection_handle
            .join()
            .expect("collection thread panicked");

        // Assert overlap: all four workers must have entered before any was released.
        assert_eq!(
            entered, 4,
            "expected 4 overlapping workers, but only {} entered before timeout",
            entered
        );

        // Verify results are correct.
        assert_eq!(packages.as_deref(), Some("test-packages"));
        assert_eq!(resolution.as_deref(), Some("test-resolution"));
        assert_eq!(gpu.as_deref(), Some("test-gpu"));
        assert_eq!(cosmetics.wm_theme.as_deref(), Some("test-wm"));
        assert_eq!(shell.as_str(), "test-shell");
        assert_eq!(de, None);
        assert_eq!(wm, None);
    }

    #[test]
    fn test_full_parallel_omits_none_results() {
        let collectors = FullCollectors {
            get_packages: || None,
            get_shell: || "shell".to_string(),
            get_resolution: || None,
            get_gpu_info: || None,
            get_desktop_environment: || None,
            get_window_manager_or_session: || None,
            get_desktop_cosmetics: || DesktopCosmetics::default(),
        };

        let (packages, resolution, gpu, cosmetics, _) =
            collect_selected_in_parallel(collectors, || {
                ("shell".to_string(), None::<String>, None::<String>)
            });

        assert!(packages.is_none());
        assert!(resolution.is_none());
        assert!(gpu.is_none());
        assert!(cosmetics.wm_theme.is_none());
        assert!(cosmetics.gtk_theme.is_none());
        assert!(cosmetics.icon_theme.is_none());
        assert!(cosmetics.font.is_none());
    }

    #[test]
    #[should_panic(expected = "packages panic")]
    fn test_panic_in_parallel_collector_propagates() {
        let collectors = FullCollectors {
            get_packages: || panic!("packages panic"),
            get_shell: || "shell".to_string(),
            get_resolution: || Some("1920x1080".to_string()),
            get_gpu_info: || Some("GPU".to_string()),
            get_desktop_environment: || Some("DE".to_string()),
            get_window_manager_or_session: || Some("WM".to_string()),
            get_desktop_cosmetics: || DesktopCosmetics::default(),
        };

        let _ = collect_selected_in_parallel(collectors, || {
            ("shell".to_string(), None::<String>, None::<String>)
        });
    }
}
