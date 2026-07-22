//! Module responsável pela coleta de informações do sistema.
//!
//! Decomposição em sub-módulos:
//! - `fields`: Structs `SystemSnapshot` e `SystemField`, ordem de campos.
//! - `command`: Execução de comandos externos (best-effort).
//! - `parsers`: Parsing de saídas de comandos (`dpkg`, `xrandr`, `lspci`, `gsettings`).
//! - `format`: Formatação de valores (uptime, bytes, strings de desktop).
//! - `disk`: Coleta de informações de disco com deduplicação.
//! - `desktop`: DE, WM, resolução, cosméticos de desktop.
//! - `collector`: `SystemSnapshot::collect()` e helpers de coleta.

mod collector;
mod command;
mod desktop;
mod disk;
mod fields;
mod format;
mod parsers;

#[allow(unused_imports)]
pub use command::run_command_best_effort;
pub use disk::get_disk_detail_fields;
#[allow(unused_imports)]
pub use fields::{
    get_compact_field_order, get_display_field_order, get_field_order, SystemField, SystemSnapshot,
};
