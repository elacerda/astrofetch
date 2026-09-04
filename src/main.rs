mod app;
mod cli;
mod density;
mod display_plan;
mod engine;
mod error;
mod galaxy;
mod layout;
mod render;
// Phase 0 foundation: feature-seed derivation is consumed by barred spirals in Phase 1.
#[allow(dead_code)]
mod seed;
mod setup_shell;
mod system;
mod terminal;
mod update_check;
#[cfg(test)]
mod visual_baseline;

use app::App;

fn main() {
    if let Err(e) = App::run() {
        eprintln!("Erro: {}", e);
        std::process::exit(1);
    }
}
