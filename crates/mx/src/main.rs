#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> mx_gui::Result {
    mx_gui::run(mx_core::cli::parse(std::env::args_os().skip(1)))
}
