use crate::cli::{Args, ArtModel};
use crate::engine::ArtModel as EngineModel;
use crate::error::AppError;
use crate::layout::compose_layout;
use crate::render::{render_ascii, Palette};
use crate::system::SystemSnapshot;
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
        let _terminal = Terminal {
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
        let canvas = engine_model.generate(self.args.width, self.args.height, self.args.seed);
        let palette = Palette::DEFAULT;
        let art_lines = render_ascii(&canvas, &palette);

        // Coleta informações do sistema
        let system = SystemSnapshot::collect();
        let info_lines = self.build_info_lines(&system);

        // Imprime na saída
        if self.args.info_only {
            // Modo info-only: apenas informações, sem arte ASCII
            for line in info_lines {
                println!("{}", line);
            }
        } else if self.args.logo_only {
            // Modo logo-only: apenas arte ASCII
            for line in art_lines {
                println!("{}", line);
            }
        } else {
            // Modo normal: arte + informações (side-by-side)
            let output_lines = compose_layout(&art_lines, &info_lines, self.args.width, 2);
            for line in output_lines {
                println!("{}", line);
            }
        }

        Ok(())
    }

    /// Constrói as linhas de informações do sistema.
    fn build_info_lines(&self, system: &SystemSnapshot) -> Vec<String> {
        if self.args.info_only {
            // Modo info-only: apenas informações
            let mut lines = Vec::new();

            if !self.args.compact {
                lines.push(format!("{}@{}", system.user, system.host));
            }

            lines.push(format!("OS: {}", system.os));
            lines.push(format!("Kernel: {}", system.kernel));
            lines.push(format!("Uptime: {}", system.uptime));

            if !self.args.compact {
                if system.shell != "N/A" {
                    lines.push(format!("Shell: {}", system.shell));
                }
                lines.push(format!("CPU: {}", system.cpu));
                lines.push(format!("RAM: {}", system.ram));
            }

            lines
        } else if self.args.logo_only {
            // Modo logo-only: apenas arte (sem informações)
            Vec::new()
        } else {
            // Modo normal: arte + informações
            let mut lines = Vec::new();

            if !self.args.compact {
                lines.push(format!("{}@{}", system.user, system.host));
            }

            lines.push(format!("OS: {}", system.os));
            lines.push(format!("Kernel: {}", system.kernel));
            lines.push(format!("Uptime: {}", system.uptime));

            if !self.args.compact {
                if system.shell != "N/A" {
                    lines.push(format!("Shell: {}", system.shell));
                }
                lines.push(format!("CPU: {}", system.cpu));
                lines.push(format!("RAM: {}", system.ram));
            }

            lines
        }
    }
}
