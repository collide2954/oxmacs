#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> oxmacs_gui::Result {
    oxmacs_gui::run(oxmacs_core::cli::parse(std::env::args_os().skip(1)))
}
