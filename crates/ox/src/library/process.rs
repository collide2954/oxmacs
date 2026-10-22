use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::library::paths::text;
use crate::portable::{Failure, Portable};
use crate::symbol::Sym;
use crate::value::{Value, print};
use crate::vm::build::full;

const MODULE: &str = "ox/process";

#[derive(Debug, Default)]
struct Run {
    program: String,
    arguments: Vec<String>,
    directory: Option<PathBuf>,
    environment: Vec<(String, Option<String>)>,
    input: Option<String>,
}

fn keyword(name: &str) -> Portable {
    Portable::Keyword(Sym::intern(name))
}

fn wrong(heap: &Heap, expected: &str, value: Value) -> Error {
    Error::new(format!(
        "process/run expects {expected}, got {}",
        print::write(heap, value)
    ))
    .of(ErrorKind::Type)
}

fn name_of(heap: &Heap, value: Value) -> String {
    match value {
        Value::Keyword(name) | Value::Symbol(name) => name.name().to_owned(),
        _ => print::display(heap, value),
    }
}

fn environment(heap: &Heap, value: Value) -> Result<Vec<(String, Option<String>)>, Error> {
    let entries = heap
        .entries(value)
        .ok_or_else(|| wrong(heap, "a map for :env", value))?;
    Ok(entries
        .into_iter()
        .map(|(key, given)| {
            let given = (given != Value::Nil).then(|| print::display(heap, given));
            (name_of(heap, key), given)
        })
        .collect())
}

fn parse(heap: &Heap, arguments: &[Value]) -> Result<Run, Error> {
    let program = text(heap, "process/run", arguments[0])?.to_owned();
    let listed = heap
        .text(arguments[1])
        .is_none()
        .then(|| heap.elements(arguments[1]))
        .flatten()
        .ok_or_else(|| wrong(heap, "a vector of arguments", arguments[1]))?;
    let mut run = Run {
        program,
        arguments: listed
            .into_iter()
            .map(|argument| print::display(heap, argument))
            .collect(),
        ..Run::default()
    };
    for pair in arguments[2..].chunks(2) {
        match pair {
            [Value::Keyword(option), value] if option.name() == "cwd" => {
                run.directory = Some(PathBuf::from(text(heap, "process/run", *value)?));
            }
            [Value::Keyword(option), value] if option.name() == "env" => {
                run.environment = environment(heap, *value)?;
            }
            [Value::Keyword(option), value] if option.name() == "input" => {
                run.input = Some(text(heap, "process/run", *value)?.to_owned());
            }
            _ => {
                return Err(Error::new(format!(
                    "process/run takes :cwd, :env and :input, not {}",
                    print::write(heap, pair[0])
                )));
            }
        }
    }
    Ok(run)
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_: &mut Command) {}

fn execute(run: &Run) -> Result<Portable, Failure> {
    let failed = |error: &dyn std::fmt::Display| Failure {
        value: None,
        message: format!("Couldn't run {}: {error}", run.program),
    };
    let mut command = Command::new(&run.program);
    command
        .args(&run.arguments)
        .stdin(if run.input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(directory) = &run.directory {
        command.current_dir(directory);
    }
    for (name, value) in &run.environment {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    hide_console(&mut command);
    let mut child = command.spawn().map_err(|error| failed(&error))?;
    let feeding = run.input.clone().and_then(|input| {
        let mut stdin = child.stdin.take()?;
        Some(thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        }))
    });
    let output = child.wait_with_output().map_err(|error| failed(&error))?;
    if let Some(feeding) = feeding {
        let _ = feeding.join();
    }
    let status = output
        .status
        .code()
        .map_or(Portable::Nil, |code| Portable::Int(i64::from(code)));
    let decoded = |bytes: &[u8]| Portable::String(String::from_utf8_lossy(bytes).into());
    Ok(Portable::Map(vec![
        (keyword("status"), status),
        (keyword("stdout"), decoded(&output.stdout)),
        (keyword("stderr"), decoded(&output.stderr)),
    ]))
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native_in(MODULE, "run", 2, None, |heap, arguments| {
        let run = parse(heap, arguments)?;
        let label = format!("ox process/run {}", run.program);
        heap.background(&label, move || execute(&run))
    });
    isolate.define_native_in(MODULE, "env", 1, Some(1), |heap, arguments| {
        let name = text(heap, "process/env", arguments[0])?.to_owned();
        match std::env::var(&name) {
            Ok(value) => heap.string(&value).map_err(full),
            Err(_) => Ok(Value::Nil),
        }
    });
}
