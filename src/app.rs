use crate::cli::{Args, ArtModel};
use crate::engine::ArtModel as EngineModel;
use crate::error::AppError;
use crate::layout::compose_layout;
use crate::render::{
    normalize_with_stretch, render_ascii, render_colored_ascii, Palette, StretchType,
};
use crate::system::{get_field_order, SystemSnapshot};
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
            lines.push(format!("OS: {}", system.get("OS")));
            lines.push(format!("Kernel: {}", system.get("Kernel")));
            lines.push(format!("Uptime: {}", system.get("Uptime")));
            lines.push(format!("Disk: {}", system.get("Disk")));
            lines.push(format!("CPU: {}", system.get("CPU")));
            lines.push(format!("RAM: {}", system.get("RAM")));
        } else {
            // Full mode: ordem específica definida em get_field_order()
            for field_name in get_field_order() {
                lines.push(format!("{}: {}", field_name, system.get(field_name)));
            }
        }

        lines
    }
}
