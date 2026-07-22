use super::command::run_command_best_effort;
use super::format::normalize_desktop_string;
use super::parsers::{parse_gsettings_value, parse_xrandr_resolution};

/// Temas e fonte de desktop coletados de forma best-effort.
#[derive(Debug, Clone, Default)]
pub(crate) struct DesktopCosmetics {
    pub(crate) wm_theme: Option<String>,
    pub(crate) gtk_theme: Option<String>,
    pub(crate) icon_theme: Option<String>,
    pub(crate) font: Option<String>,
}

/// Obtém o Desktop Environment (DE) usando variáveis de ambiente.
/// Tenta XDG_CURRENT_DESKTOP, DESKTOP_SESSION, XDG_SESSION_DESKTOP.
pub(crate) fn get_desktop_environment() -> Option<String> {
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
pub(crate) fn get_window_manager_or_session() -> Option<String> {
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

/// Obtém temas e fonte via `gsettings` em ambientes GNOME-like.
pub(crate) fn get_desktop_cosmetics() -> DesktopCosmetics {
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
pub(crate) fn get_gsettings_string(schema: &str, key: &str) -> Option<String> {
    run_command_best_effort("gsettings", &["get", schema, key])
        .and_then(|output| parse_gsettings_value(&output))
}

/// Obtém resolução ativa do display no Linux via `xrandr --current` (best-effort).
/// Retorna apenas a resolução no formato `WxH` quando for possível identificar
/// um monitor conectado com modo atual.
pub(crate) fn get_resolution() -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::command::ENV_MUTEX;

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
}
