use crate::cli::{Args, ArtModel, Command};
use crate::display_plan::{DisplayPlanner, OutputMode, PlannerRequest};
use crate::engine::{ArtModel as EngineModel, GeneratedScene};
use crate::error::AppError;
use crate::layout::compose_layout;
use crate::render::{
    prepare_density, render_half_blocks, render_starfield, PreparedDensity, RenderProfile,
};
use crate::system::{get_disk_detail_fields, get_display_field_order, SystemSnapshot};
use crate::terminal::{visible_width, Terminal, TerminalDimensions};
use clap::Parser;

const HEADER_COLOR: &str = "\x1b[93m";
const LABEL_COLOR: &str = "\x1b[94m";
const VALUE_COLOR: &str = "\x1b[97m";
const RESET: &str = "\x1b[0m";

#[derive(Debug)]
struct InfoLine {
    label: String,
    value: String,
}

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
        if let Some(command) = &self.args.command {
            return match command {
                Command::SetupShell(args) => crate::setup_shell::run(args),
                Command::UninstallShell(args) => crate::setup_shell::uninstall(args),
            };
        }

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

        // Detect terminal capabilities
        let terminal_dims = self.terminal.dimensions();

        // Branch by output mode
        if self.args.info_only {
            self.execute_info_only(&terminal)
        } else if self.args.logo_only {
            self.execute_logo_only(&terminal, colors_enabled, engine_model, terminal_dims)
        } else {
            self.execute_combined(&terminal, colors_enabled, engine_model, terminal_dims)
        }
    }

    /// Executa em modo InfoOnly: apenas informações do sistema.
    fn execute_info_only(&self, terminal: &Terminal) -> Result<(), AppError> {
        // Build formatted information lines
        let system = SystemSnapshot::collect();
        let info_lines = self.build_info_lines(&system);

        terminal.print_lines(&info_lines)?;
        Ok(())
    }

    /// Executa em modo LogoOnly: apenas arte ASCII.
    fn execute_logo_only(
        &self,
        terminal: &Terminal,
        colors_enabled: bool,
        engine_model: EngineModel,
        terminal_dims: Option<TerminalDimensions>,
    ) -> Result<(), AppError> {
        // Detect terminal dimensions for art planning
        let planner = DisplayPlanner::new();
        let request = PlannerRequest {
            terminal_dimensions: terminal_dims,
            requested_width: self.args.width,
            requested_height: self.args.height,
            requested_layout: self.args.layout,
            output_mode: OutputMode::LogoOnly,
            info_visible_width: 0,
            info_line_count: 0,
        };

        let display_plan = planner.plan(request);

        let (art_width, art_height) = match display_plan {
            crate::display_plan::DisplayPlan::LogoOnly { art } => (art.width, art.height),
            _ => unreachable!(),
        };

        let GeneratedScene {
            resolved_model,
            density,
            ..
        } = engine_model.generate_scene(art_width, art_height, self.args.seed);

        let profile: RenderProfile = RenderProfile::for_model(resolved_model);
        let prepared = prepare_density(density, profile);

        let art_lines = Self::render_art_lines(prepared, colors_enabled, terminal);

        terminal.print_lines(&art_lines)?;
        Ok(())
    }

    /// Executa em modo Combined: arte ASCII + informações do sistema.
    fn execute_combined(
        &self,
        terminal: &Terminal,
        colors_enabled: bool,
        engine_model: EngineModel,
        terminal_dims: Option<TerminalDimensions>,
    ) -> Result<(), AppError> {
        // Build formatted information lines and measure dimensions
        let system = SystemSnapshot::collect();
        let info_lines = self.build_info_lines(&system);
        let info_visible_width = info_lines
            .iter()
            .map(|s| visible_width(s))
            .max()
            .unwrap_or(0);
        let info_line_count = info_lines.len();

        // Resolve art dimensions from terminal capabilities and explicit overrides
        let planner = DisplayPlanner::new();
        let request = PlannerRequest {
            terminal_dimensions: terminal_dims,
            requested_width: self.args.width,
            requested_height: self.args.height,
            requested_layout: self.args.layout,
            output_mode: OutputMode::Combined,
            info_visible_width,
            info_line_count,
        };

        let display_plan = planner.plan(request);

        let (art_width, art_height) = match display_plan {
            crate::display_plan::DisplayPlan::Combined { art, .. } => (art.width, art.height),
            _ => unreachable!(),
        };

        let GeneratedScene {
            resolved_model,
            density,
            ..
        } = engine_model.generate_scene(art_width, art_height, self.args.seed);

        let profile: RenderProfile = RenderProfile::for_model(resolved_model);
        let prepared = prepare_density(density, profile);

        let art_lines = Self::render_art_lines(prepared, colors_enabled, terminal);

        match display_plan {
            crate::display_plan::DisplayPlan::Combined { art, layout } => {
                let output_lines = compose_layout(&art_lines, &info_lines, art.width, layout);
                terminal.print_lines(&output_lines)?;
            }
            _ => unreachable!(),
        }

        Ok(())
    }

    /// Renderiza a arte usando o renderer adequado para cada modelo.
    ///
    /// Usa o PreparedDensity para selecionar o renderer correto,
    /// garantindo que tanto Starfield explícito quanto Random que resolve
    /// para Starfield usem o renderer de campo de estrelas.
    fn render_art_lines(
        prepared: PreparedDensity,
        colors_enabled: bool,
        terminal: &Terminal,
    ) -> Vec<String> {
        match prepared {
            PreparedDensity::Starfield { density } => {
                let canvas = density.into_rows();
                render_starfield(&canvas, colors_enabled, terminal)
            }
            PreparedDensity::Galaxy { density, threshold } => {
                let canvas = density.into_rows();
                render_half_blocks(
                    &canvas,
                    threshold,
                    colors_enabled && terminal.colors_enabled(),
                )
            }
        }
    }

    /// Constrói as linhas de informações do sistema.
    fn build_info_lines(&self, system: &SystemSnapshot) -> Vec<String> {
        let mut lines = Vec::new();
        let colors_enabled = self.info_colors_enabled();

        // user@host (apenas em modo full)
        if !self.args.compact {
            lines.push(format_header(&system.user_host, colors_enabled));
        }

        let mut info_fields: Vec<InfoLine> = get_display_field_order(system, self.args.compact)
            .into_iter()
            .map(|field_name| InfoLine {
                label: field_name.to_string(),
                value: system.get(field_name),
            })
            .collect();

        if self.args.disk_details {
            let disk_detail_fields: Vec<InfoLine> = get_disk_detail_fields()
                .into_iter()
                .map(|field| InfoLine {
                    label: field.label,
                    value: field.value,
                })
                .collect();

            if !disk_detail_fields.is_empty() {
                if let Some(disk_index) = info_fields.iter().position(|line| line.label == "Disk") {
                    info_fields.splice(disk_index + 1..disk_index + 1, disk_detail_fields);
                } else {
                    info_fields.extend(disk_detail_fields);
                }
            }
        }
        let label_width = info_fields
            .iter()
            .map(|line| visible_width(&line.label))
            .max()
            .unwrap_or(0);

        lines.extend(
            info_fields
                .iter()
                .map(|line| format_info_line(line, label_width, colors_enabled)),
        );

        lines
    }

    fn info_colors_enabled(&self) -> bool {
        !self.args.no_color && self.terminal.colors_enabled()
    }
}

fn format_header(text: &str, colors_enabled: bool) -> String {
    if colors_enabled {
        format!("{}{}{}", HEADER_COLOR, text, RESET)
    } else {
        text.to_string()
    }
}

fn format_info_line(line: &InfoLine, label_width: usize, colors_enabled: bool) -> String {
    let label_padding = " ".repeat(label_width.saturating_sub(visible_width(&line.label)) + 1);

    if colors_enabled {
        format!(
            "{}{}:{}{}{}{}{}",
            LABEL_COLOR, line.label, RESET, label_padding, VALUE_COLOR, line.value, RESET
        )
    } else {
        format!("{}:{}{}", line.label, label_padding, line.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{ArtModel, LayoutChoice};
    use crate::terminal::Terminal;
    use std::collections::BTreeMap;

    fn build_test_app(compact: bool, no_color: bool, colors_enabled: bool) -> App {
        App {
            args: Args {
                command: None,
                model: ArtModel::Random,
                width: None,
                height: None,
                seed: None,
                no_color,
                logo_only: false,
                info_only: false,
                compact,
                disk_details: false,
                layout: LayoutChoice::Auto,
            },
            terminal: Terminal {
                is_tty: colors_enabled,
                colors_enabled,
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
        let app = build_test_app(false, true, false);
        let lines = app.build_info_lines(&base_snapshot());

        assert_eq!(lines[0], "astro@station");
        assert_eq!(
            lines[1..],
            [
                "OS:     Linux",
                "Kernel: 6.x",
                "Uptime: 1h 2m",
                "Shell:  bash",
                "Disk:   1G/2G (50%)",
                "CPU:    Test CPU",
                "RAM:    1.0GB / 2.0GB"
            ]
        );
        assert!(!lines.join("\n").contains('\x1b'));
    }

    #[test]
    fn test_build_info_lines_compact_field_ordering() {
        let app = build_test_app(true, true, false);
        let lines = app.build_info_lines(&base_snapshot());

        assert_eq!(
            lines,
            [
                "OS:     Linux",
                "Kernel: 6.x",
                "Uptime: 1h 2m",
                "Disk:   1G/2G (50%)",
                "CPU:    Test CPU",
                "RAM:    1.0GB / 2.0GB"
            ]
        );
    }

    #[test]
    fn test_build_info_lines_full_future_ordering_when_present() {
        let app = build_test_app(false, true, false);
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
                "OS:         Linux",
                "Kernel:     6.x",
                "Uptime:     1h 2m",
                "Packages:   1234",
                "Shell:      bash",
                "Resolution: 1920x1080",
                "DE:         GNOME",
                "WM:         Mutter",
                "WM Theme:   Adwaita",
                "GTK Theme:  Adwaita",
                "Icon Theme: Adwaita",
                "Font:       Noto Sans 11",
                "Disk:       1G/2G (50%)",
                "CPU:        Test CPU",
                "GPU:        Test GPU",
                "RAM:        1.0GB / 2.0GB"
            ]
        );
    }

    #[test]
    fn test_build_info_lines_full_omits_missing_advanced_fields() {
        let app = build_test_app(false, true, false);
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
        let app = build_test_app(true, true, false);
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
                "OS:     Linux",
                "Kernel: 6.x",
                "Uptime: 1h 2m",
                "Disk:   1G/2G (50%)",
                "CPU:    Test CPU",
                "RAM:    1.0GB / 2.0GB"
            ]
        );
        assert!(!joined.contains("Resolution:"));
        assert!(!joined.contains("WM Theme:"));
        assert!(!joined.contains("GTK Theme:"));
        assert!(!joined.contains("Icon Theme:"));
        assert!(!joined.contains("Font:"));
        assert!(!joined.contains("GPU:"));
    }

    #[test]
    fn test_build_info_lines_colorizes_header_labels_and_values() {
        let app = build_test_app(false, false, true);
        let lines = app.build_info_lines(&base_snapshot());

        assert_eq!(lines[0], "\x1b[93mastro@station\x1b[0m");
        assert_eq!(lines[1], "\x1b[94mOS:\x1b[0m     \x1b[97mLinux\x1b[0m");
        assert_eq!(lines[2], "\x1b[94mKernel:\x1b[0m \x1b[97m6.x\x1b[0m");
        assert!(lines.join("\n").contains("\x1b[0m"));
    }

    #[test]
    fn test_build_info_lines_no_color_overrides_colored_terminal() {
        let app = build_test_app(false, true, true);
        let lines = app.build_info_lines(&base_snapshot());

        assert_eq!(lines[0], "astro@station");
        assert_eq!(lines[1], "OS:     Linux");
        assert!(!lines.join("\n").contains('\x1b'));
    }

    #[test]
    fn test_compose_layout_keeps_info_ansi_from_affecting_art_padding() {
        let app = build_test_app(false, false, true);
        let info = app.build_info_lines(&base_snapshot());
        let art = vec!["**".to_string()];

        let result = crate::layout::compose_side_by_side(&art, &info, 6, 2);

        assert!(result[0].starts_with("**      \x1b[93mastro@station\x1b[0m"));
    }
}
