mod animation;
mod app;
#[allow(dead_code)]
mod bar;
mod cli;
mod density;
mod display_plan;
// Phase 2B: deterministic dust-lane configuration, consumed as Spiral extinction.
mod dust;
// Elliptical Morphology v2, B1: deterministic morphology contract (config +
// derivation) that the Elliptical density generator will consume in B2.
#[allow(dead_code)]
mod elliptical;
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
use error::AppError;

fn main() {
    if let Err(e) = App::run() {
        if matches!(e, AppError::Interrupted) {
            // Conventional interrupted semantics: exit code 128 + SIGINT,
            // without printing an error message.
            std::process::exit(130);
        }
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
