use sysinfo::{CpuRefreshKind, System};

/// Snapshot das informações do sistema.
#[derive(Debug, Clone, Default)]
pub struct SystemSnapshot {
    pub user: String,
    pub host: String,
    pub os: String,
    pub kernel: String,
    pub uptime: String,
    pub shell: String,
    pub cpu: String,
    pub ram: String,
    #[allow(dead_code)]
    pub disk: String,
}

impl SystemSnapshot {
    /// Coleta informações do sistema com fallbacks gracefulls.
    pub fn collect() -> Self {
        let mut system = System::new();

        // Atualiza apenas CPU e memória para manter rápido
        system.refresh_cpu_specifics(CpuRefreshKind::new().with_frequency());
        system.refresh_memory();

        Self {
            user: env_or_fallback("USER", "unknown"),
            host: env_or_fallback("HOSTNAME", "unknown"),
            os: get_os(),
            kernel: get_kernel(),
            uptime: get_uptime(),
            shell: get_shell(),
            cpu: get_cpu_info(&system),
            ram: get_ram_info(&system),
            disk: get_disk_info(),
        }
    }
}

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

/// Obtém informações de disco.
fn get_disk_info() -> String {
    // Simplificado para MVP - mostra N/A
    // Implementação completa exigiria subprocessos específicos por plataforma
    "N/A".to_string()
}
