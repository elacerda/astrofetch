use crate::cli::{Args, ArtModel};
use crate::engine::ArtModel as EngineModel;
use crate::error::AppError;
use crate::layout::compose_layout;
use crate::render::{
    normalize_with_stretch, render_ascii, render_colored_ascii, Palette, StretchType,
};
use crate::system::{get_display_field_order, SystemSnapshot};
use crate::terminal::Terminal;
use clap::Parser;

/// Aplicação principal do AstroFetch.
pub struct App {
    args: Args,
    terminal: Terminal,
}

impl App {
    /// Cria uma nova instância do App.
    pub fn new() -> Result<Self, AppError> {
        let args = Args::parse();
        let terminal = Terminal::new();

        Ok(Self { args, terminal })
    }

    /// Executa o app principal.
    pub fn run() -> Result<(), AppError> {
        let app = Self::new()?;
        app.execute()
    }

    /// Executa a lógica principal.
    fn execute(&self) -> Result<(), AppError> {
        // Verifica --no-color explicito
        let colors_enabled = if self.args.no_color {
            false
        } else {
            self.terminal.colors_enabled()
        };

        // Cria o terminal com a configuração final de cores
        let terminal = Terminal {
            is_tty: self.terminal.is_tty,
            colors_enabled,
        };

        // Determina o modelo de arte
        let engine_model = match self.args.model {
            ArtModel::Random => EngineModel::Random,
            ArtModel::Elliptical => EngineModel::Elliptical,
            ArtModel::Spiral => EngineModel::Spiral,
            ArtModel::Cluster => EngineModel::Cluster,
            ArtModel::Starfield => EngineModel::Starfield,
        };

        // Gera a arte ASCII
        let mut canvas = engine_model.generate(self.args.width, self.args.height, self.args.seed);

        // Aplica stretch para melhor contraste
        canvas = normalize_with_stretch(&canvas, StretchType::default());

        let palette = Palette::DEFAULT;

        // Imprime na saída
        if self.args.info_only {
            // Modo info-only: apenas informações, sem arte ASCII
            let info_lines = self.build_info_lines(&SystemSnapshot::collect());
            for line in info_lines {
                println!("{}", line);
            }
        } else if self.args.logo_only {
            // Modo logo-only: apenas arte ASCII
            let art_lines = if colors_enabled {
                render_colored_ascii(&canvas, &palette, &terminal)
            } else {
                render_ascii(&canvas, &palette)
            };
            for line in art_lines {
                println!("{}", line);
            }
        } else {
            // Modo normal: arte + informações (side-by-side)
            let art_lines = if colors_enabled {
                render_colored_ascii(&canvas, &palette, &terminal)
            } else {
                render_ascii(&canvas, &palette)
            };

            let system = SystemSnapshot::collect();
            let info_lines = self.build_info_lines(&system);
            let output_lines = compose_layout(&art_lines, &info_lines, self.args.width, 2);
            for line in output_lines {
                println!("{}", line);
            }
        }

        Ok(())
    }

    /// Constrói as linhas de informações do sistema.
    fn build_info_lines(&self, system: &SystemSnapshot) -> Vec<String> {
        let mut lines = Vec::new();

        // user@host (apenas em modo full)
        if !self.args.compact {
            lines.push(system.user_host.clone());
        }

        // Compact mode: OS, Kernel, Uptime, Disk, CPU, RAM
        if self.args.compact {
            for field_name in get_display_field_order(system, true) {
                lines.push(format!("{}: {}", field_name, system.get(field_name)));
            }
        } else {
            // Full mode: ordem screenFetch-like, omitindo campos indisponíveis.
            for field_name in get_display_field_order(system, false) {
                lines.push(format!("{}: {}", field_name, system.get(field_name)));
            }
        }

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ArtModel;
    use crate::terminal::Terminal;
    use std::collections::BTreeMap;

    fn build_test_app(compact: bool) -> App {
        App {
            args: Args {
                model: ArtModel::Random,
                width: 40,
                height: 20,
                seed: None,
                no_color: true,
                logo_only: false,
                info_only: false,
                compact,
            },
            terminal: Terminal {
                is_tty: false,
                colors_enabled: false,
            },
        }
    }

    fn base_snapshot() -> SystemSnapshot {
        let mut fields = BTreeMap::new();
        fields.insert("OS".to_string(), "Linux".to_string());
        fields.insert("Kernel".to_string(), "6.x".to_string());
        fields.insert("Uptime".to_string(), "1h 2m".to_string());
        fields.insert("Shell".to_string(), "bash".to_string());
        fields.insert("Disk".to_string(), "1G/2G (50%)".to_string());
        fields.insert("CPU".to_string(), "Test CPU".to_string());
        fields.insert("RAM".to_string(), "1.0GB / 2.0GB".to_string());

        SystemSnapshot {
            user_host: "astro@station".to_string(),
            fields,
        }
    }

    #[test]
    fn test_build_info_lines_full_field_ordering() {
        let app = build_test_app(false);
        let lines = app.build_info_lines(&base_snapshot());

        assert_eq!(lines[0], "astro@station");
        assert_eq!(
            lines[1..],
            [
                "OS: Linux",
                "Kernel: 6.x",
                "Uptime: 1h 2m",
                "Shell: bash",
                "Disk: 1G/2G (50%)",
                "CPU: Test CPU",
                "RAM: 1.0GB / 2.0GB"
            ]
        );
    }

    #[test]
    fn test_build_info_lines_compact_field_ordering() {
        let app = build_test_app(true);
        let lines = app.build_info_lines(&base_snapshot());

        assert_eq!(
            lines,
            [
                "OS: Linux",
                "Kernel: 6.x",
                "Uptime: 1h 2m",
                "Disk: 1G/2G (50%)",
                "CPU: Test CPU",
                "RAM: 1.0GB / 2.0GB"
            ]
        );
    }

    #[test]
    fn test_build_info_lines_full_future_ordering_when_present() {
        let app = build_test_app(false);
        let mut snapshot = base_snapshot();
        snapshot
            .fields
            .insert("Packages".to_string(), "1234".to_string());
        snapshot
            .fields
            .insert("Resolution".to_string(), "1920x1080".to_string());
        snapshot
            .fields
            .insert("DE".to_string(), "GNOME".to_string());
        snapshot
            .fields
            .insert("WM".to_string(), "Mutter".to_string());
        snapshot
            .fields
            .insert("WM Theme".to_string(), "Adwaita".to_string());
        snapshot
            .fields
            .insert("GTK Theme".to_string(), "Adwaita".to_string());
        snapshot
            .fields
            .insert("Icon Theme".to_string(), "Adwaita".to_string());
        snapshot
            .fields
            .insert("Font".to_string(), "Noto Sans 11".to_string());
        snapshot
            .fields
            .insert("GPU".to_string(), "Test GPU".to_string());

        let lines = app.build_info_lines(&snapshot);

        assert_eq!(lines[0], "astro@station");
        assert_eq!(
            lines[1..],
            [
                "OS: Linux",
                "Kernel: 6.x",
                "Uptime: 1h 2m",
                "Packages: 1234",
                "Shell: bash",
                "Resolution: 1920x1080",
                "DE: GNOME",
                "WM: Mutter",
                "WM Theme: Adwaita",
                "GTK Theme: Adwaita",
                "Icon Theme: Adwaita",
                "Font: Noto Sans 11",
                "Disk: 1G/2G (50%)",
                "CPU: Test CPU",
                "GPU: Test GPU",
                "RAM: 1.0GB / 2.0GB"
            ]
        );
    }

    #[test]
    fn test_build_info_lines_full_omits_missing_advanced_fields() {
        let app = build_test_app(false);
        let lines = app.build_info_lines(&base_snapshot());
        let joined = lines.join("\n");

        assert!(!joined.contains("Packages:"));
        assert!(!joined.contains("Resolution:"));
        assert!(!joined.contains("DE:"));
        assert!(!joined.contains("WM:"));
        assert!(!joined.contains("WM Theme:"));
        assert!(!joined.contains("GTK Theme:"));
        assert!(!joined.contains("Icon Theme:"));
        assert!(!joined.contains("Font:"));
        assert!(!joined.contains("GPU:"));
    }

    #[test]
    fn test_build_info_lines_compact_excludes_resolution_and_gpu_when_present() {
        let app = build_test_app(true);
        let mut snapshot = base_snapshot();
        snapshot
            .fields
            .insert("Resolution".to_string(), "1920x1080".to_string());
        snapshot
            .fields
            .insert("WM Theme".to_string(), "Adwaita".to_string());
        snapshot
            .fields
            .insert("GTK Theme".to_string(), "Yaru".to_string());
        snapshot
            .fields
            .insert("Icon Theme".to_string(), "Yaru".to_string());
        snapshot
            .fields
            .insert("Font".to_string(), "Cantarell 11".to_string());
        snapshot
            .fields
            .insert("GPU".to_string(), "Test GPU".to_string());

        let lines = app.build_info_lines(&snapshot);
        let joined = lines.join("\n");

        assert_eq!(
            lines,
            [
                "OS: Linux",
                "Kernel: 6.x",
                "Uptime: 1h 2m",
                "Disk: 1G/2G (50%)",
                "CPU: Test CPU",
                "RAM: 1.0GB / 2.0GB"
            ]
        );
        assert!(!joined.contains("Resolution:"));
        assert!(!joined.contains("WM Theme:"));
        assert!(!joined.contains("GTK Theme:"));
        assert!(!joined.contains("Icon Theme:"));
        assert!(!joined.contains("Font:"));
        assert!(!joined.contains("GPU:"));
    }
}
