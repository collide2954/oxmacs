#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::ffi::OsString;
use std::process::ExitCode;

use oxmacs_core::cli;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match oxmacs_gui::run(cli::parse(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("oxmacs: {error}");
            ExitCode::FAILURE
        }
    }
}
