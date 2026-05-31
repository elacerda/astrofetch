mod app;
mod cli;
mod engine;
mod error;
mod layout;
mod render;
mod setup_shell;
mod system;
mod terminal;

use app::App;

fn main() {
    if let Err(e) = App::run() {
        eprintln!("Erro: {}", e);
        std::process::exit(1);
    }
}
