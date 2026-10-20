#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod applets;

use std::process::ExitCode;

use oxmacs_core::cli::{self, Invocation};

fn main() -> ExitCode {
    let outcome = match cli::invocation(std::env::args_os().skip(1)) {
        Invocation::Edit(visits) => {
            oxmacs_gui::run(visits, applets::all).map_err(|error| error.to_string())
        }
        Invocation::IrcDaemon => oxmacs_core::bouncer::daemon::main(),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("oxmacs: {error}");
            ExitCode::FAILURE
        }
    }
}
