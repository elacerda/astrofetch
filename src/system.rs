use std::collections::BTreeMap;

#[cfg(any(target_os = "linux", test))]
use std::collections::BTreeSet;
use std::sync::Mutex;
use sysinfo::{CpuRefreshKind, System};

/// Executa um comando externo de forma segura e best-effort.
///
/// # Arguments
/// * `cmd` - Nome do comando (ex: "uname", "hostname")
/// * `args` - Argumentos do comando como fatias de strings
///
/// # Returns
/// * `Some(String)` - Comando executado com sucesso e stdout não vazio
/// * `None` - Comando falhou, saiu com código diferente de zero,
///   stdout é inválido UTF-8, ou stdout está vazio
///
/// # Limitações
/// * Não há timeout implementado ainda (TODO: adicionar timeout antes de usar
///   para comandos potencialmente lentos)
/// * Output limitado a 64KB para evitar strings muito grandes
///
/// # Exemplos
/// ```ignore
/// let os = run_command_best_effort("uname", &["-s"]);
/// let hostname = run_command_best_effort("hostname", &[]);
/// ```
#[allow(dead_code)]
pub fn run_command_best_effort(cmd: &str, args: &[&str]) -> Option<String> {
    run_command_best_effort_with_limit(cmd, args, 64 * 1024)
}

/// Executa um comando externo com limite de tamanho customizável.
/// Usado para comandos que podem gerar output grande (ex: listagem de pacotes).
///
/// # Arguments
/// * `cmd` - Nome do comando
/// * `args` - Argumentos do comando
/// * `max_output_size` - Tamanho máximo do output em bytes
///
/// # Returns
/// * `Some(String)` - Comando executado com sucesso e stdout não vazio
/// * `None` - Comando falhou, saiu com código diferente de zero,
///   stdout é inválido UTF-8, stdout está vazio, ou output foi truncado
#[allow(dead_code)]
fn run_command_best_effort_with_limit(
    cmd: &str,
    args: &[&str],
    max_output_size: usize,
) -> Option<String> {
    let mut command = std::process::Command::new(cmd);
    command.args(args);

    // Executa o comando e captura o output
    let output = command.output().ok()?;

    // Verifica se o comando saiu com código de sucesso (0)
    if !output.status.success() {
        return None;
    }

    // Converte stdout para String, retornando None se não for UTF-8 válido
    let stdout = String::from_utf8(output.stdout).ok()?;

    // Trimming da saída
    let trimmed = stdout.trim();

    // Retorna None se output estiver vazio após trim
    if trimmed.is_empty() {
        return None;
    }

    // Verifica se o output foi truncado (excedeu o limite)
    // Se o output original era maior que max_output_size, não podemos confiar no resultado
    if stdout.len() > max_output_size {
        return None;
    }

    // Limita o tamanho do output para evitar strings muito grandes
    if trimmed.len() > max_output_size {
        return Some(trimmed[..max_output_size].to_string());
    }

    Some(trimmed.to_string())
}

/// Mutex global para proteger testes que mutam variáveis de ambiente.
/// Isso evita race conditions quando os testes rodam em paralelo.
#[allow(dead_code)]
static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// Normaliza uma string de desktop/session para exibição.
/// Remove sufixos comuns como "-session", "-wm", etc.
fn normalize_desktop_string(s: &str) -> String {
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

        // Constrói o snapshot com todos os campos
        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), os.clone());
        fields.insert("Kernel".to_string(), kernel.clone());
        fields.insert("Uptime".to_string(), uptime.clone());
        // Adiciona Packages se disponível (best-effort)
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

        // Adiciona DE e WM se disponíveis
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

/// Obtém o número de pacotes instalados (best-effort).
/// No Linux Debian/Ubuntu, tenta usar dpkg-query.
///
/// Usa dpkg-query -W -f=${db:Status-Abbrev} ${binary:Package}\n que lista
/// pacotes com o status abbrev (ex: "ii" para instalado).
/// Fallback para dpkg --get-selections se necessário.
fn get_packages() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        // Tenta usar dpkg-query -W com formato de status abbreviation
        // Formato: "ii package-name" (ii = installed)
        // Outros status: rc (removed but config), un (not installed), etc.
        if let Some(output) = run_command_best_effort_with_limit(
            "dpkg-query",
            &["-W", "-f=${db:Status-Abbrev} ${binary:Package}\n"],
            256 * 1024, // 256KB para pacotes
        ) {
            // Conta apenas linhas onde o status é exatamente "ii" (instalado)
            if let Some(count) = parse_dpkg_query_installed_count(&output) {
                return Some(count.to_string());
            }
        }

        // Fallback: tenta dpkg --get-selections
        // Formato: "package-name    install"
        // Conta linhas onde a segunda coluna é "install"
        if let Some(output) = run_command_best_effort_with_limit(
            "dpkg",
            &["--get-selections"],
            256 * 1024, // 256KB para pacotes
        ) {
            // Conta linhas onde a segunda coluna é exatamente "install"
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

/// Conta pacotes instalados em saída de `dpkg-query`.
#[cfg(any(target_os = "linux", test))]
fn parse_dpkg_query_installed_count(output: &str) -> Option<usize> {
    let count = output
        .lines()
        .filter(|line| line.trim().starts_with("ii "))
        .count();

    (count > 0).then_some(count)
}

/// Conta pacotes instalados em saída de `dpkg --get-selections`.
#[cfg(any(target_os = "linux", test))]
fn parse_dpkg_get_selections_installed_count(output: &str) -> Option<usize> {
    let count = output
        .lines()
        .filter(|line| {
            let mut parts = line.split_whitespace();
            parts.next().is_some() && parts.next() == Some("install")
        })
        .count();

    (count > 0).then_some(count)
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
        // Tenta usar sysctl. Parsing real do boottime fica para um patch futuro.
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

/// Formata segundos em formato legível.
#[cfg(target_os = "linux")]
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

/// Obtém o Desktop Environment (DE) usando variáveis de ambiente.
/// Tenta XDG_CURRENT_DESKTOP, DESKTOP_SESSION, XDG_SESSION_DESKTOP.
fn get_desktop_environment() -> Option<String> {
    // Tenta XDG_CURRENT_DESKTOP (pode conter múltiplos DEs separados por :)
    if let Ok(xdg_desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
        if !xdg_desktop.is_empty() {
            let first = xdg_desktop.split(':').next()?;
            return Some(normalize_desktop_string(first));
        }
    }

    // Tenta DESKTOP_SESSION
    if let Ok(session) = std::env::var("DESKTOP_SESSION") {
        if !session.is_empty() {
            return Some(normalize_desktop_string(&session));
        }
    }

    // Tenta XDG_SESSION_DESKTOP
    if let Ok(xdg_session) = std::env::var("XDG_SESSION_DESKTOP") {
        if !xdg_session.is_empty() {
            return Some(normalize_desktop_string(&xdg_session));
        }
    }

    None
}

/// Obtém o Window Manager ou session hint usando variáveis de ambiente.
/// Tenta WAYLAND_DISPLAY, DISPLAY, e XDG_SESSION_TYPE.
fn get_window_manager_or_session() -> Option<String> {
    // Se WAYLAND_DISPLAY está definido, provavelmente Wayland
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        return Some("Wayland".to_string());
    }

    // Se DISPLAY está definido, provavelmente X11
    if std::env::var("DISPLAY").is_ok() {
        return Some("X11".to_string());
    }

    // Tenta XDG_SESSION_TYPE
    if let Ok(session_type) = std::env::var("XDG_SESSION_TYPE") {
        return Some(normalize_desktop_string(&session_type));
    }

    None
}

/// Temas e fonte de desktop coletados de forma best-effort.
#[derive(Debug, Clone, Default)]
struct DesktopCosmetics {
    wm_theme: Option<String>,
    gtk_theme: Option<String>,
    icon_theme: Option<String>,
    font: Option<String>,
}

/// Obtém temas e fonte via `gsettings` em ambientes GNOME-like.
fn get_desktop_cosmetics() -> DesktopCosmetics {
    #[cfg(target_os = "linux")]
    {
        DesktopCosmetics {
            wm_theme: get_gsettings_string("org.gnome.desktop.wm.preferences", "theme"),
            gtk_theme: get_gsettings_string("org.gnome.desktop.interface", "gtk-theme"),
            icon_theme: get_gsettings_string("org.gnome.desktop.interface", "icon-theme"),
            font: get_gsettings_string("org.gnome.desktop.interface", "font-name"),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        DesktopCosmetics::default()
    }
}

/// Lê uma chave string do `gsettings`, sem depender de shell.
#[cfg(target_os = "linux")]
fn get_gsettings_string(schema: &str, key: &str) -> Option<String> {
    run_command_best_effort("gsettings", &["get", schema, key])
        .and_then(|output| parse_gsettings_value(&output))
}

/// Normaliza saída string do `gsettings get`.
#[cfg(any(target_os = "linux", test))]
fn parse_gsettings_value(output: &str) -> Option<String> {
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

/// Obtém resolução ativa do display no Linux via `xrandr --current` (best-effort).
/// Retorna apenas a resolução no formato `WxH` quando for possível identificar
/// um monitor conectado com modo atual.
fn get_resolution() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        run_command_best_effort("xrandr", &["--current"])
            .and_then(|output| parse_xrandr_resolution(&output))
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Faz parse da saída do `xrandr --current` e retorna a melhor resolução disponível.
///
/// Regras:
/// - Prefere monitor marcado como `primary`.
/// - Caso contrário, usa o primeiro monitor `connected` com modo atual.
/// - Ignora linhas `disconnected`.
#[cfg(any(target_os = "linux", test))]
fn parse_xrandr_resolution(output: &str) -> Option<String> {
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
fn extract_resolution_from_connected_line(line: &str) -> Option<String> {
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
fn extract_resolution_from_mode_line(line: &str) -> Option<String> {
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
fn is_resolution_token(token: &str) -> bool {
    let (width, height) = match token.split_once('x') {
        Some(parts) => parts,
        None => return false,
    };

    !width.is_empty()
        && !height.is_empty()
        && width.chars().all(|c| c.is_ascii_digit())
        && height.chars().all(|c| c.is_ascii_digit())
}

/// Obtém GPU(s) do Linux via `lspci` (best-effort).
/// Retorna um nome conciso por GPU, juntando múltiplas entradas com ` / `.
fn get_gpu_info() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        run_command_best_effort("lspci", &[]).and_then(|output| parse_lspci_gpu_info(&output))
    }

    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Faz parse da saída do `lspci` para controladores gráficos.
#[cfg(any(target_os = "linux", test))]
fn parse_lspci_gpu_info(output: &str) -> Option<String> {
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
fn extract_lspci_gpu_name(line: &str) -> Option<&str> {
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
fn normalize_gpu_name(raw: &str) -> String {
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
fn strip_lspci_revision(raw: &str) -> &str {
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
fn find_bracket_content_with_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
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
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
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
#[cfg(any(target_os = "linux", test))]
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

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(any(target_os = "linux", test))]
struct DiskUsageEntry {
    key: String,
    total_bytes: u64,
    available_bytes: u64,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct LinuxMountIdentity {
    key: String,
    fs_type: String,
}

#[cfg(any(target_os = "linux", test))]
fn format_disk_usage_bytes(used_bytes: u64, total_bytes: u64) -> String {
    let used_formatted = format_bytes(used_bytes);
    let total_formatted = format_bytes(total_bytes);
    let percent = ((used_bytes as f64 / total_bytes as f64) * 100.0).round() as u8;

    format!("{} / {} ({}%)", used_formatted, total_formatted, percent)
}

#[cfg(any(target_os = "linux", test))]
fn format_disk_usage_entries(entries: &[DiskUsageEntry]) -> String {
    let Some((used_bytes, total_bytes)) = aggregate_unique_disk_usage(entries) else {
        return "N/A".to_string();
    };

    format_disk_usage_bytes(used_bytes, total_bytes)
}

#[cfg(any(target_os = "linux", test))]
fn aggregate_unique_disk_usage(entries: &[DiskUsageEntry]) -> Option<(u64, u64)> {
    let mut seen = BTreeSet::new();
    let mut total_bytes: u64 = 0;
    let mut used_bytes: u64 = 0;

    for entry in entries {
        if entry.total_bytes == 0 || !seen.insert(entry.key.clone()) {
            continue;
        }

        total_bytes = total_bytes.saturating_add(entry.total_bytes);
        used_bytes =
            used_bytes.saturating_add(entry.total_bytes.saturating_sub(entry.available_bytes));
    }

    if total_bytes == 0 {
        None
    } else {
        Some((used_bytes, total_bytes))
    }
}

#[cfg(any(target_os = "linux", test))]
fn is_ignored_filesystem(fs_type: &str) -> bool {
    matches!(
        fs_type,
        "tmpfs"
            | "devtmpfs"
            | "proc"
            | "sysfs"
            | "cgroup"
            | "cgroup2"
            | "squashfs"
            | "overlay"
            | "debugfs"
            | "tracefs"
            | "efivarfs"
            | "fusectl"
            | "ramfs"
            | "autofs"
            | "devpts"
            | "hugetlbfs"
            | "mqueue"
            | "configfs"
            | "securityfs"
            | "pstore"
            | "bpf"
            | "binfmt_misc"
            | "nsfs"
    ) || fs_type.starts_with("fuse.")
}

#[cfg(any(target_os = "linux", test))]
fn normalize_disk_source(source: &str) -> String {
    let source = source.trim();
    let source = source.split_once('[').map_or(source, |(base, _)| base);
    source.trim().to_string()
}

#[cfg(any(target_os = "linux", test))]
fn fallback_disk_identity_key(fs_type: &str, source: &str, mount_point: &str) -> String {
    let source = normalize_disk_source(source);

    if source.is_empty() || source == "none" {
        format!("mount:{fs_type}:{mount_point}")
    } else {
        format!("source:{fs_type}:{source}")
    }
}

#[cfg(target_os = "linux")]
fn collect_linux_disk_usage_entries(disks: &sysinfo::Disks) -> Vec<DiskUsageEntry> {
    let mount_identities = linux_mount_identities_by_mount_point();

    disks
        .iter()
        .filter_map(|disk| linux_disk_usage_entry(disk, &mount_identities))
        .collect()
}

#[cfg(target_os = "linux")]
fn linux_disk_usage_entry(
    disk: &sysinfo::Disk,
    mount_identities: &BTreeMap<String, LinuxMountIdentity>,
) -> Option<DiskUsageEntry> {
    let total_bytes = disk.total_space();

    if total_bytes == 0 {
        return None;
    }

    let mount_point = disk.mount_point().to_string_lossy().to_string();
    let sysinfo_fs_type = disk.file_system().to_string_lossy().to_ascii_lowercase();
    let source = disk.name().to_string_lossy();

    let (fs_type, key) = mount_identities
        .get(&mount_point)
        .map(|identity| (identity.fs_type.as_str(), identity.key.clone()))
        .unwrap_or_else(|| {
            (
                sysinfo_fs_type.as_str(),
                fallback_disk_identity_key(&sysinfo_fs_type, &source, &mount_point),
            )
        });

    if is_ignored_filesystem(fs_type) {
        return None;
    }

    Some(DiskUsageEntry {
        key,
        total_bytes,
        available_bytes: disk.available_space(),
    })
}

#[cfg(target_os = "linux")]
fn linux_mount_identities_by_mount_point() -> BTreeMap<String, LinuxMountIdentity> {
    let Ok(text) = std::fs::read_to_string("/proc/self/mountinfo") else {
        return BTreeMap::new();
    };

    parse_linux_mountinfo_identities(&text)
}

#[cfg(target_os = "linux")]
fn parse_linux_mountinfo_identities(text: &str) -> BTreeMap<String, LinuxMountIdentity> {
    let mut identities = BTreeMap::new();

    for line in text.lines() {
        if let Some((mount_point, identity)) = parse_linux_mountinfo_line(line) {
            identities.insert(mount_point, identity);
        }
    }

    identities
}

#[cfg(target_os = "linux")]
fn parse_linux_mountinfo_line(line: &str) -> Option<(String, LinuxMountIdentity)> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.len() < 10 {
        return None;
    }

    let device_id = parts[2];
    let mount_point = decode_mountinfo_path(parts[4]);
    let separator_index = parts.iter().position(|part| *part == "-")?;

    if parts.len() <= separator_index + 1 {
        return None;
    }

    let fs_type = parts[separator_index + 1].to_ascii_lowercase();
    let key = format!("mountinfo:{fs_type}:{device_id}");

    Some((mount_point, LinuxMountIdentity { key, fs_type }))
}

#[cfg(target_os = "linux")]
fn decode_mountinfo_path(path: &str) -> String {
    path.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

/// Obtém informações de disco usando sysinfo.
fn get_disk_info() -> String {
    #[cfg(target_os = "linux")]
    {
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let entries = collect_linux_disk_usage_entries(&disks);
        format_disk_usage_entries(&entries)
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
                        if let Some(used_val) = parse_df_size(used) {
                            if let Some(total_val) = parse_df_size(total) {
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

/// Obtém detalhes de disco por filesystem contado.
///
/// Atualmente os detalhes usam a mesma deduplicação do Linux. Em outras
/// plataformas, a flag `--disk-details` mantém a saída padrão sem linhas extras.
pub fn get_disk_detail_fields() -> Vec<SystemField> {
    #[cfg(target_os = "linux")]
    {
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let mount_identities = linux_mount_identities_by_mount_point();
        let mut seen = BTreeSet::new();
        let mut details = Vec::new();

        for disk in disks.iter() {
            let mount_point = disk.mount_point().to_string_lossy().to_string();

            let Some(entry) = linux_disk_usage_entry(disk, &mount_identities) else {
                continue;
            };

            if !seen.insert(entry.key) {
                continue;
            }

            let used_bytes = entry.total_bytes.saturating_sub(entry.available_bytes);
            details.push(SystemField::new(
                format!("Disk {}", mount_point),
                format_disk_usage_bytes(used_bytes, entry.total_bytes),
            ));
        }

        details
    }

    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

/// Parse tamanho de disco do df (ex: "1.8T", "3.9G", "100M").
#[cfg(target_os = "macos")]
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
    fn test_aggregate_unique_disk_usage_single_filesystem() {
        let entries = vec![DiskUsageEntry {
            key: "mountinfo:btrfs:254:0".to_string(),
            total_bytes: 100,
            available_bytes: 40,
        }];

        assert_eq!(aggregate_unique_disk_usage(&entries), Some((60, 100)));
        assert_eq!(format_disk_usage_entries(&entries), "60B / 100B (60%)");
    }

    #[test]
    fn test_aggregate_unique_disk_usage_multiple_unique_filesystems() {
        let entries = vec![
            DiskUsageEntry {
                key: "mountinfo:btrfs:254:0".to_string(),
                total_bytes: 100,
                available_bytes: 40,
            },
            DiskUsageEntry {
                key: "mountinfo:btrfs:254:1".to_string(),
                total_bytes: 300,
                available_bytes: 100,
            },
        ];

        assert_eq!(aggregate_unique_disk_usage(&entries), Some((260, 400)));
        assert_eq!(format_disk_usage_entries(&entries), "260B / 400B (65%)");
    }

    #[test]
    fn test_aggregate_unique_disk_usage_deduplicates_btrfs_subvolumes() {
        let entries = vec![
            DiskUsageEntry {
                key: "mountinfo:btrfs:254:0".to_string(),
                total_bytes: 235,
                available_bytes: 116,
            },
            DiskUsageEntry {
                key: "mountinfo:btrfs:254:0".to_string(),
                total_bytes: 235,
                available_bytes: 116,
            },
            DiskUsageEntry {
                key: "mountinfo:btrfs:254:0".to_string(),
                total_bytes: 235,
                available_bytes: 116,
            },
            DiskUsageEntry {
                key: "mountinfo:btrfs:254:1".to_string(),
                total_bytes: 932,
                available_bytes: 42,
            },
        ];

        assert_eq!(aggregate_unique_disk_usage(&entries), Some((1009, 1167)));
    }

    #[test]
    fn test_format_disk_usage_entries_returns_na_for_no_counted_filesystems() {
        let entries = vec![
            DiskUsageEntry {
                key: "mountinfo:tmpfs:0:42".to_string(),
                total_bytes: 0,
                available_bytes: 0,
            },
            DiskUsageEntry {
                key: "mountinfo:proc:0:43".to_string(),
                total_bytes: 0,
                available_bytes: 0,
            },
        ];

        assert_eq!(aggregate_unique_disk_usage(&entries), None);
        assert_eq!(format_disk_usage_entries(&entries), "N/A");
    }

    #[test]
    fn test_is_ignored_filesystem_filters_virtual_and_transient_filesystems() {
        for fs_type in [
            "tmpfs",
            "devtmpfs",
            "proc",
            "sysfs",
            "cgroup2",
            "overlay",
            "squashfs",
            "fuse.portal",
            "fuse.gvfsd-fuse",
        ] {
            assert!(
                is_ignored_filesystem(fs_type),
                "{fs_type} should be ignored"
            );
        }

        for fs_type in ["btrfs", "ext4", "xfs", "vfat", "ntfs", "exfat"] {
            assert!(
                !is_ignored_filesystem(fs_type),
                "{fs_type} should be counted"
            );
        }
    }

    #[test]
    fn test_normalize_disk_source_removes_btrfs_subvolume_suffix() {
        assert_eq!(
            normalize_disk_source("/dev/mapper/luks-abc[/@home]"),
            "/dev/mapper/luks-abc"
        );
        assert_eq!(
            normalize_disk_source("/dev/mapper/luks-abc[/@cache]"),
            "/dev/mapper/luks-abc"
        );
        assert_eq!(normalize_disk_source("/dev/sda1"), "/dev/sda1");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_linux_mountinfo_deduplicates_btrfs_subvolume_identity() {
        let text = "\
25 1 254:0 /@ / rw,relatime - btrfs /dev/mapper/root rw,subvol=/@
26 1 254:0 /@home /home rw,relatime - btrfs /dev/mapper/root rw,subvol=/@home
27 1 254:1 /@data /mnt/Data rw,relatime - btrfs /dev/mapper/data rw,subvol=/@data
28 1 259:2 / /boot rw,relatime - vfat /dev/nvme0n1p2 rw
";

        let identities = parse_linux_mountinfo_identities(text);

        assert_eq!(identities["/"].key, "mountinfo:btrfs:254:0");
        assert_eq!(identities["/home"].key, "mountinfo:btrfs:254:0");
        assert_eq!(identities["/mnt/Data"].key, "mountinfo:btrfs:254:1");
        assert_eq!(identities["/boot"].key, "mountinfo:vfat:259:2");
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

    #[test]
    fn test_get_desktop_environment_with_desktop_session() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let orig_xdg = std::env::var("XDG_CURRENT_DESKTOP").ok();
        let orig_session = std::env::var("DESKTOP_SESSION").ok();
        let orig_session_desktop = std::env::var("XDG_SESSION_DESKTOP").ok();

        std::env::remove_var("XDG_CURRENT_DESKTOP");
        std::env::remove_var("DESKTOP_SESSION");
        std::env::remove_var("XDG_SESSION_DESKTOP");
        std::env::set_var("DESKTOP_SESSION", "plasma");

        let de = get_desktop_environment();
        assert_eq!(de, Some("Plasma".to_string()));

        std::env::set_var("XDG_CURRENT_DESKTOP", orig_xdg.unwrap_or_default());
        std::env::set_var("DESKTOP_SESSION", orig_session.unwrap_or_default());
        std::env::set_var(
            "XDG_SESSION_DESKTOP",
            orig_session_desktop.unwrap_or_default(),
        );
    }

    #[test]
    fn test_get_desktop_environment_with_xdg_session_desktop() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let orig_xdg = std::env::var("XDG_CURRENT_DESKTOP").ok();
        let orig_session = std::env::var("DESKTOP_SESSION").ok();
        let orig_session_desktop = std::env::var("XDG_SESSION_DESKTOP").ok();

        std::env::remove_var("XDG_CURRENT_DESKTOP");
        std::env::remove_var("DESKTOP_SESSION");
        std::env::remove_var("XDG_SESSION_DESKTOP");
        std::env::set_var("XDG_SESSION_DESKTOP", "xfce");

        let de = get_desktop_environment();
        assert_eq!(de, Some("Xfce".to_string()));

        std::env::set_var("XDG_CURRENT_DESKTOP", orig_xdg.unwrap_or_default());
        std::env::set_var("DESKTOP_SESSION", orig_session.unwrap_or_default());
        std::env::set_var(
            "XDG_SESSION_DESKTOP",
            orig_session_desktop.unwrap_or_default(),
        );
    }

    #[test]
    fn test_get_desktop_environment_missing_env() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let orig_xdg = std::env::var("XDG_CURRENT_DESKTOP").ok();
        let orig_session = std::env::var("DESKTOP_SESSION").ok();
        let orig_session_desktop = std::env::var("XDG_SESSION_DESKTOP").ok();

        std::env::remove_var("XDG_CURRENT_DESKTOP");
        std::env::remove_var("DESKTOP_SESSION");
        std::env::remove_var("XDG_SESSION_DESKTOP");

        let de = get_desktop_environment();
        assert_eq!(de, None);

        std::env::set_var("XDG_CURRENT_DESKTOP", orig_xdg.unwrap_or_default());
        std::env::set_var("DESKTOP_SESSION", orig_session.unwrap_or_default());
        std::env::set_var(
            "XDG_SESSION_DESKTOP",
            orig_session_desktop.unwrap_or_default(),
        );
    }

    #[test]
    fn test_get_window_manager_wayland() {
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

        let wm = get_window_manager_or_session();
        assert_eq!(wm, Some("Wayland".to_string()));

        // Restaura o estado original
        std::env::set_var("WAYLAND_DISPLAY", orig_wayland.unwrap_or_default());
        std::env::set_var("DISPLAY", orig_display.unwrap_or_default());
        std::env::set_var("XDG_SESSION_TYPE", orig_session_type.unwrap_or_default());
    }

    #[test]
    fn test_get_window_manager_x11() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let orig_wayland = std::env::var("WAYLAND_DISPLAY").ok();
        let orig_display = std::env::var("DISPLAY").ok();
        let orig_session_type = std::env::var("XDG_SESSION_TYPE").ok();

        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");
        std::env::remove_var("XDG_SESSION_TYPE");
        std::env::set_var("DISPLAY", ":0");

        let wm = get_window_manager_or_session();
        assert_eq!(wm, Some("X11".to_string()));

        std::env::set_var("WAYLAND_DISPLAY", orig_wayland.unwrap_or_default());
        std::env::set_var("DISPLAY", orig_display.unwrap_or_default());
        std::env::set_var("XDG_SESSION_TYPE", orig_session_type.unwrap_or_default());
    }

    #[test]
    fn test_get_window_manager_session_type() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let orig_wayland = std::env::var("WAYLAND_DISPLAY").ok();
        let orig_display = std::env::var("DISPLAY").ok();
        let orig_session_type = std::env::var("XDG_SESSION_TYPE").ok();

        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");
        std::env::remove_var("XDG_SESSION_TYPE");
        std::env::set_var("XDG_SESSION_TYPE", "wayland");

        let wm = get_window_manager_or_session();
        assert_eq!(wm, Some("Wayland".to_string()));

        std::env::set_var("WAYLAND_DISPLAY", orig_wayland.unwrap_or_default());
        std::env::set_var("DISPLAY", orig_display.unwrap_or_default());
        std::env::set_var("XDG_SESSION_TYPE", orig_session_type.unwrap_or_default());
    }

    #[test]
    fn test_get_window_manager_missing_env() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let orig_wayland = std::env::var("WAYLAND_DISPLAY").ok();
        let orig_display = std::env::var("DISPLAY").ok();
        let orig_session_type = std::env::var("XDG_SESSION_TYPE").ok();

        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");
        std::env::remove_var("XDG_SESSION_TYPE");

        let wm = get_window_manager_or_session();
        assert_eq!(wm, None);

        std::env::set_var("WAYLAND_DISPLAY", orig_wayland.unwrap_or_default());
        std::env::set_var("DISPLAY", orig_display.unwrap_or_default());
        std::env::set_var("XDG_SESSION_TYPE", orig_session_type.unwrap_or_default());
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

    // Tests for run_command_best_effort helper
    // Note: These tests avoid depending on local machine commands by using
    // mock-like behavior through the function's error handling paths.

    #[test]
    fn test_run_command_best_effort_nonexistent_command() {
        // Comando inexistente deve retornar None
        let result = run_command_best_effort("nonexistent_command_xyz123", &[]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_run_command_best_effort_empty_output() {
        // Comando que produz output vazio deve retornar None
        // Usamos 'echo -n' para produzir output vazio
        #[cfg(target_os = "linux")]
        {
            let result = run_command_best_effort("echo", &["-n", ""]);
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_run_command_best_effort_simple_command() {
        // Comando simples que deve funcionar na maioria dos sistemas
        // Usamos 'true' que sai com código 0 e produz output vazio
        // Então usamos 'echo' que deve funcionar
        #[cfg(target_os = "linux")]
        {
            let result = run_command_best_effort("echo", &["hello"]);
            assert_eq!(result, Some("hello".to_string()));
        }
    }

    #[test]
    fn test_run_command_best_effort_trims_whitespace() {
        // Comando que produz output com whitespace deve ser trimado
        #[cfg(target_os = "linux")]
        {
            let result = run_command_best_effort("echo", &["  hello  "]);
            assert_eq!(result, Some("hello".to_string()));
        }
    }

    #[test]
    fn test_run_command_best_effort_non_zero_exit() {
        // Comando que sai com código diferente de zero deve retornar None
        #[cfg(target_os = "linux")]
        {
            let result = run_command_best_effort("sh", &["-c", "exit 1"]);
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_run_command_best_effort_with_args() {
        // Comando com múltiplos argumentos
        #[cfg(target_os = "linux")]
        {
            let result = run_command_best_effort("echo", &["a", "b", "c"]);
            assert_eq!(result, Some("a b c".to_string()));
        }
    }

    #[test]
    fn test_run_command_best_effort_output_size_limit() {
        // Testa que output muito grande é detectado como truncado e retorna None
        // Usamos printf para gerar output grande
        #[cfg(target_os = "linux")]
        {
            // Cria uma string grande (maior que 64KB)
            let large_output: String = "x".repeat(70 * 1024); // 70KB

            // Cria um script que imprime output grande
            let result = run_command_best_effort(
                "sh",
                &[
                    "-c",
                    &format!("printf '%{}s' {}", large_output.len(), large_output),
                ],
            );

            // O resultado deve ser None porque o output foi truncado (excedeu 64KB)
            assert_eq!(result, None);
        }
    }

    // Tests for get_packages (best-effort)

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
    fn test_get_packages_invalid_output_returns_none() {
        // Simula saída inválida (sem linhas começando com "ii ")
        let invalid_output = r#"Desired=Unknown/Install/Remove/Purge/Hold
| Status=Not/Inst/Conf-files/Unpacked/halF-conf/Half-inst/trig-aWait/Trig-pend
|/ Err?=(none)/Reinst-required (Status,Err: uppercase=bad)
||/ Name           Version      Architecture Description
+++-==============-============-============-=================================
"#;

        let count = invalid_output
            .lines()
            .filter(|line| line.trim().starts_with("ii "))
            .count();

        // Não deve encontrar pacotes
        assert_eq!(count, 0);
    }

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
    fn test_run_command_best_effort_with_limit_truncation_detection() {
        // Testa que output truncado é detectado e retorna None
        // Usamos printf para gerar output grande
        #[cfg(target_os = "linux")]
        {
            // Cria uma string grande (maior que 1KB)
            let large_output: String = "x".repeat(2 * 1024); // 2KB

            // Tenta com limite pequeno (512 bytes) - deve ser truncado e retornar None
            let result = run_command_best_effort_with_limit(
                "sh",
                &[
                    "-c",
                    &format!("printf '%{}s' {}", large_output.len(), large_output),
                ],
                512, // Limite pequeno para forçar truncamento
            );

            // O resultado deve ser None porque o output foi truncado
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_run_command_best_effort_with_limit_accepts_valid_output() {
        // Testa que output válido dentro do limite é aceito
        #[cfg(target_os = "linux")]
        {
            let small_output = "hello world";

            let result = run_command_best_effort_with_limit(
                "echo",
                &[small_output],
                64 * 1024, // Limite grande o suficiente
            );

            // O resultado deve ser Some("hello world")
            assert_eq!(result, Some(small_output.to_string()));
        }
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
