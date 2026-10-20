#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod applets;
use oxmacs_core::applet::{Applet, subcommand};
use oxmacs_core::cli::{self, Invocation};
use std::ffi::OsString;
use std::process::ExitCode;
fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let applets = applets::all();
    let named = args
        .first()
        .and_then(|first| first.to_str())
        .and_then(|name| {
            subcommand(
                applets.iter().map(|applet| applet.as_ref() as &dyn Applet),
                name,
            )
        });
    let outcome = match named {
        Some(found) => (found.run)(),
        None => match cli::invocation(args) {
            Invocation::Edit(visits) => {
                oxmacs_gui::run(visits, applets::all).map_err(|error| error.to_string())
            }
            Invocation::IrcDaemon => oxmacs_core::bouncer::daemon::main(),
        },
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("oxmacs: {error}");
            ExitCode::FAILURE
        }
    }
}
