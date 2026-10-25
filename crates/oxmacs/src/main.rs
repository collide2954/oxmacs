#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::ffi::OsString;
use std::process::ExitCode;

use oxmacs_core::cli::{self, Launch};

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match cli::parse(args) {
        Launch::Edit { visits, init } => match oxmacs_gui::run(visits, init) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                attach_console();
                eprintln!("oxmacs: {error}");
                ExitCode::FAILURE
            }
        },
        Launch::Help => {
            attach_console();
            print!("{}", cli::HELP);
            ExitCode::SUCCESS
        }
        Launch::Version => {
            attach_console();
            println!("oxmacs {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Launch::Unknown(option) => {
            attach_console();
            eprintln!("oxmacs: unknown option {option}; oxmacs --help lists them");
            ExitCode::from(2)
        }
    }
}

#[cfg(windows)]
#[expect(unsafe_code, reason = "AttachConsole has no safe wrapper")]
fn attach_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};

    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_console() {}
