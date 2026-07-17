use crate::cli::{Args, ArtModel, Command, RendererChoice};
use crate::display_plan::{DisplayPlanner, OutputMode, PlannerRequest};
use crate::engine::{ArtModel as EngineModel, GeneratedScene};
use crate::error::AppError;
use crate::layout::compose_layout;
use crate::render::{
    prepare_density, render_ascii, render_half_blocks, render_shades, render_starfield,
    EffectiveRenderer, PreparedDensity, RenderProfile,
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

        // Detect terminal capabilities
        let terminal_dims = self.terminal.dimensions();

        // Branch by output mode
        if self.args.info_only {
            self.execute_info_only(&terminal)
        } else {
            let engine_model = match self.args.model {
                ArtModel::Random => EngineModel::Random,
                ArtModel::Elliptical => EngineModel::Elliptical,
                ArtModel::Spiral => EngineModel::Spiral,
                ArtModel::Cluster => EngineModel::Cluster,
                ArtModel::Starfield => EngineModel::Starfield,
            };

            if self.args.logo_only {
                self.execute_logo_only(&terminal, colors_enabled, engine_model, terminal_dims)
            } else {
                self.execute_combined(&terminal, colors_enabled, engine_model, terminal_dims)
            }
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
            resolved_model: engine_resolved,
            density,
            ..
        } = engine_model.generate_scene(art_width, art_height, self.args.seed);

        let effective_renderer =
            Self::resolve_effective_renderer(self.args.renderer, engine_resolved)?;

        let profile: RenderProfile = RenderProfile::for_model(engine_resolved);
        let prepared = prepare_density(density, profile);

        let art_lines =
            Self::render_prepared_density(prepared, effective_renderer, colors_enabled, terminal)?;

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
            resolved_model: engine_resolved,
            density,
            ..
        } = engine_model.generate_scene(art_width, art_height, self.args.seed);

        let effective_renderer =
            Self::resolve_effective_renderer(self.args.renderer, engine_resolved)?;

        let profile: RenderProfile = RenderProfile::for_model(engine_resolved);
        let prepared = prepare_density(density, profile);

        let art_lines =
            Self::render_prepared_density(prepared, effective_renderer, colors_enabled, terminal)?;

        match display_plan {
            crate::display_plan::DisplayPlan::Combined { art, layout } => {
                let output_lines = compose_layout(&art_lines, &info_lines, art.width, layout);
                terminal.print_lines(&output_lines)?;
            }
            _ => unreachable!(),
        }

        Ok(())
    }

    /// Resolves the effective renderer based on the requested renderer choice and resolved model.
    ///
    /// Implements the compatibility matrix:
    /// - Galaxy models (Spiral, Elliptical, Cluster) with Auto → HalfBlock
    /// - Galaxy models with HalfBlock → HalfBlock
    /// - Galaxy models with Shade → Shade
    /// - Galaxy models with Ascii → Ascii
    /// - Starfield with Auto → Starfield
    /// - Starfield with Ascii → Starfield
    /// - Starfield with HalfBlock → CLI error
    /// - Starfield with Shade → CLI error
    /// - Random model (unresolved) → Render error (should never happen)
    fn resolve_effective_renderer(
        requested: RendererChoice,
        resolved_model: EngineModel,
    ) -> Result<EffectiveRenderer, AppError> {
        match (resolved_model, requested) {
            // Galaxy models
            (EngineModel::Spiral, RendererChoice::Auto) => Ok(EffectiveRenderer::HalfBlock),
            (EngineModel::Spiral, RendererChoice::HalfBlock) => Ok(EffectiveRenderer::HalfBlock),
            (EngineModel::Spiral, RendererChoice::Shade) => Ok(EffectiveRenderer::Shade),
            (EngineModel::Spiral, RendererChoice::Ascii) => Ok(EffectiveRenderer::Ascii),
            (EngineModel::Elliptical, RendererChoice::Auto) => Ok(EffectiveRenderer::HalfBlock),
            (EngineModel::Elliptical, RendererChoice::HalfBlock) => {
                Ok(EffectiveRenderer::HalfBlock)
            }
            (EngineModel::Elliptical, RendererChoice::Shade) => Ok(EffectiveRenderer::Shade),
            (EngineModel::Elliptical, RendererChoice::Ascii) => Ok(EffectiveRenderer::Ascii),
            (EngineModel::Cluster, RendererChoice::Auto) => Ok(EffectiveRenderer::HalfBlock),
            (EngineModel::Cluster, RendererChoice::HalfBlock) => Ok(EffectiveRenderer::HalfBlock),
            (EngineModel::Cluster, RendererChoice::Shade) => Ok(EffectiveRenderer::Shade),
            (EngineModel::Cluster, RendererChoice::Ascii) => Ok(EffectiveRenderer::Ascii),

            // Starfield model
            (EngineModel::Starfield, RendererChoice::Auto) => Ok(EffectiveRenderer::Starfield),
            (EngineModel::Starfield, RendererChoice::HalfBlock) => Err(AppError::Cli(
                "model starfield is incompatible with --renderer half-block".to_string(),
            )),
            (EngineModel::Starfield, RendererChoice::Shade) => Err(AppError::Cli(
                "model starfield is incompatible with --renderer shade".to_string(),
            )),
            (EngineModel::Starfield, RendererChoice::Ascii) => Ok(EffectiveRenderer::Starfield),

            // Random model should be resolved before this function is called
            (EngineModel::Random, _) => Err(AppError::Render(
                "unresolved Random model reached renderer selection (internal error)".to_string(),
            )),
        }
    }

    /// Renders prepared density using the effective renderer.
    ///
    /// Validates that the prepared density and effective renderer are compatible.
    fn render_prepared_density(
        prepared: PreparedDensity,
        effective_renderer: EffectiveRenderer,
        colors_enabled: bool,
        terminal: &Terminal,
    ) -> Result<Vec<String>, AppError> {
        match (prepared, effective_renderer) {
            (PreparedDensity::Starfield { density }, EffectiveRenderer::Starfield) => {
                let canvas = density.into_rows();
                Ok(render_starfield(&canvas, colors_enabled, terminal))
            }
            (PreparedDensity::Galaxy { density, threshold }, EffectiveRenderer::HalfBlock) => {
                let canvas = density.into_rows();
                Ok(render_half_blocks(
                    &canvas,
                    threshold,
                    colors_enabled && terminal.colors_enabled(),
                ))
            }
            (PreparedDensity::Galaxy { density, threshold }, EffectiveRenderer::Shade) => {
                let canvas = density.into_rows();
                Ok(render_shades(
                    &canvas,
                    threshold,
                    colors_enabled && terminal.colors_enabled(),
                ))
            }
            (PreparedDensity::Galaxy { density, threshold }, EffectiveRenderer::Ascii) => {
                let canvas = density.into_rows();
                Ok(render_ascii(
                    &canvas,
                    threshold,
                    colors_enabled && terminal.colors_enabled(),
                ))
            }
            // Internal mismatch - should never happen if resolve_effective_renderer is correct
            (PreparedDensity::Starfield { .. }, _) => Err(AppError::Render(
                "Starfield density cannot be rendered with galaxy renderers".to_string(),
            )),
            (PreparedDensity::Galaxy { .. }, EffectiveRenderer::Starfield) => {
                Err(AppError::Render(
                    "Galaxy density cannot be rendered with starfield renderer".to_string(),
                ))
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
    use crate::cli::{ArtModel, LayoutChoice, RendererChoice};
    use crate::engine::ArtModel as EngineModel;
    use crate::terminal::Terminal;
    use std::collections::BTreeMap;

    use crate::density::DensityMap;

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
                renderer: RendererChoice::Auto,
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

    // ===== resolve_effective_renderer tests =====

    #[test]
    fn test_resolve_effective_renderer_galaxy_auto_halfblock() {
        for model in [
            EngineModel::Spiral,
            EngineModel::Elliptical,
            EngineModel::Cluster,
        ] {
            let result = App::resolve_effective_renderer(RendererChoice::Auto, model);
            assert_eq!(result.unwrap(), EffectiveRenderer::HalfBlock);
        }
    }

    #[test]
    fn test_resolve_effective_renderer_galaxy_halfblock() {
        for model in [
            EngineModel::Spiral,
            EngineModel::Elliptical,
            EngineModel::Cluster,
        ] {
            let result = App::resolve_effective_renderer(RendererChoice::HalfBlock, model);
            assert_eq!(result.unwrap(), EffectiveRenderer::HalfBlock);
        }
    }

    #[test]
    fn test_resolve_effective_renderer_galaxy_shade() {
        for model in [
            EngineModel::Spiral,
            EngineModel::Elliptical,
            EngineModel::Cluster,
        ] {
            let result = App::resolve_effective_renderer(RendererChoice::Shade, model);
            assert_eq!(result.unwrap(), EffectiveRenderer::Shade);
        }
    }

    #[test]
    fn test_resolve_effective_renderer_galaxy_ascii() {
        for model in [
            EngineModel::Spiral,
            EngineModel::Elliptical,
            EngineModel::Cluster,
        ] {
            let result = App::resolve_effective_renderer(RendererChoice::Ascii, model);
            assert_eq!(result.unwrap(), EffectiveRenderer::Ascii);
        }
    }

    #[test]
    fn test_resolve_effective_renderer_starfield_auto_starfield() {
        let result = App::resolve_effective_renderer(RendererChoice::Auto, EngineModel::Starfield);
        assert_eq!(result.unwrap(), EffectiveRenderer::Starfield);
    }

    #[test]
    fn test_resolve_effective_renderer_starfield_halfblock_error() {
        let result =
            App::resolve_effective_renderer(RendererChoice::HalfBlock, EngineModel::Starfield);
        assert!(matches!(result, Err(AppError::Cli(_))));
    }

    #[test]
    fn test_resolve_effective_renderer_starfield_shade_error() {
        let result = App::resolve_effective_renderer(RendererChoice::Shade, EngineModel::Starfield);
        assert!(matches!(result, Err(AppError::Cli(_))));
    }

    #[test]
    fn test_resolve_effective_renderer_starfield_ascii_starfield() {
        let result = App::resolve_effective_renderer(RendererChoice::Ascii, EngineModel::Starfield);
        assert_eq!(result.unwrap(), EffectiveRenderer::Starfield);
    }

    #[test]
    fn test_resolve_effective_renderer_random_error() {
        let result = App::resolve_effective_renderer(RendererChoice::Auto, EngineModel::Random);
        assert!(matches!(result, Err(AppError::Render(_))));
    }

    #[test]
    fn test_resolve_effective_renderer_incompatible_errors_contain_model_and_renderer() {
        let result1 =
            App::resolve_effective_renderer(RendererChoice::HalfBlock, EngineModel::Starfield);
        match result1 {
            Err(AppError::Cli(ref message)) => {
                assert!(message.contains("starfield"));
                assert!(message.contains("half-block"));
            }
            other => panic!("expected CLI error, got {other:?}"),
        }

        let result2 =
            App::resolve_effective_renderer(RendererChoice::Shade, EngineModel::Starfield);
        match result2 {
            Err(AppError::Cli(ref message)) => {
                assert!(message.contains("starfield"));
                assert!(message.contains("shade"));
            }
            other => panic!("expected CLI error, got {other:?}"),
        }
    }

    // ===== render_prepared_density tests =====

    #[test]
    fn test_render_prepared_density_galaxy_halfblock_succeeds() {
        let canvas = vec![vec![0.5], vec![0.5]];
        let prepared = PreparedDensity::Galaxy {
            density: DensityMap::from_rows(canvas).unwrap(),
            threshold: 0.1,
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result =
            App::render_prepared_density(prepared, EffectiveRenderer::HalfBlock, false, &terminal);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_prepared_density_galaxy_shade_succeeds() {
        let canvas = vec![vec![0.5], vec![0.5]];
        let prepared = PreparedDensity::Galaxy {
            density: DensityMap::from_rows(canvas).unwrap(),
            threshold: 0.1,
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result =
            App::render_prepared_density(prepared, EffectiveRenderer::Shade, false, &terminal);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_prepared_density_galaxy_ascii_succeeds() {
        let canvas = vec![vec![0.5], vec![0.5]];
        let prepared = PreparedDensity::Galaxy {
            density: DensityMap::from_rows(canvas).unwrap(),
            threshold: 0.1,
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result =
            App::render_prepared_density(prepared, EffectiveRenderer::Ascii, false, &terminal);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_prepared_density_starfield_starfield_succeeds() {
        let canvas = vec![vec![0.0, 0.04, 0.10, 0.20]];
        let prepared = PreparedDensity::Starfield {
            density: DensityMap::from_rows(canvas).unwrap(),
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result =
            App::render_prepared_density(prepared, EffectiveRenderer::Starfield, false, &terminal);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_prepared_density_starfield_halfblock_error() {
        let canvas = vec![vec![0.5]];
        let prepared = PreparedDensity::Starfield {
            density: DensityMap::from_rows(canvas).unwrap(),
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result =
            App::render_prepared_density(prepared, EffectiveRenderer::HalfBlock, false, &terminal);
        assert!(matches!(result, Err(AppError::Render(_))));
    }

    #[test]
    fn test_render_prepared_density_starfield_shade_error() {
        let canvas = vec![vec![0.5]];
        let prepared = PreparedDensity::Starfield {
            density: DensityMap::from_rows(canvas).unwrap(),
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result =
            App::render_prepared_density(prepared, EffectiveRenderer::Shade, false, &terminal);
        assert!(matches!(result, Err(AppError::Render(_))));
    }

    #[test]
    fn test_render_prepared_density_galaxy_starfield_error() {
        let canvas = vec![vec![0.5], vec![0.5]];
        let prepared = PreparedDensity::Galaxy {
            density: DensityMap::from_rows(canvas).unwrap(),
            threshold: 0.1,
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result =
            App::render_prepared_density(prepared, EffectiveRenderer::Starfield, false, &terminal);
        assert!(matches!(result, Err(AppError::Render(_))));
    }

    #[test]
    fn test_render_prepared_density_starfield_ascii_error() {
        let canvas = vec![vec![0.0, 0.04, 0.10, 0.20]];
        let prepared = PreparedDensity::Starfield {
            density: DensityMap::from_rows(canvas).unwrap(),
        };
        let terminal = crate::terminal::Terminal::with_colors(true, false);
        let result =
            App::render_prepared_density(prepared, EffectiveRenderer::Ascii, false, &terminal);
        assert!(matches!(result, Err(AppError::Render(_))));
    }
}
