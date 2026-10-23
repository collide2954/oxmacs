use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use crate::error::{Error, ErrorKind};
use crate::heap::Heap;
use crate::isolate::Isolate;
use crate::library::paths::text;
use crate::portable::{Channel, Failure, Portable, Sent};
use crate::symbol::Sym;
use crate::value::{Value, print};
use crate::vm::build::full;

const MODULE: &str = "ox/process";
const KEPT_LINES: usize = 4096;
const POLL: Duration = Duration::from_millis(20);

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

fn parse(heap: &Heap, name: &str, arguments: &[Value]) -> Result<Run, Error> {
    let program = text(heap, name, arguments[0])?.to_owned();
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

fn command(run: &Run, input: bool) -> Command {
    let mut command = Command::new(&run.program);
    command
        .args(&run.arguments)
        .stdin(if input { Stdio::piped() } else { Stdio::null() })
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
    command
}

fn execute(run: &Run) -> Result<Portable, Failure> {
    let failed = |error: &dyn std::fmt::Display| Failure {
        value: None,
        message: format!("Couldn't run {}: {error}", run.program),
    };
    let mut child = command(run, run.input.is_some())
        .spawn()
        .map_err(|error| failed(&error))?;
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

#[derive(Debug)]
struct Started {
    child: Mutex<Child>,
    input: Mutex<Option<ChildStdin>>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn stream(reader: impl Read + Send + 'static, lines: Arc<Channel>) {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            let Ok(line) = line else {
                break;
            };
            let item = Portable::String(line.into());
            loop {
                match lines.send(item.clone()) {
                    Sent::Done | Sent::Closed => break,
                    Sent::Full => thread::sleep(POLL),
                }
            }
        }
        lines.close();
    });
}

fn exited(started: &Started) -> Result<Portable, Failure> {
    loop {
        match locked(&started.child).try_wait() {
            Ok(Some(status)) => {
                return Ok(status
                    .code()
                    .map_or(Portable::Nil, |code| Portable::Int(i64::from(code))));
            }
            Ok(None) => thread::sleep(POLL),
            Err(error) => {
                return Err(Failure {
                    value: None,
                    message: error.to_string(),
                });
            }
        }
    }
}

fn start(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let run = parse(heap, "process/start", arguments)?;
    let mut child = command(&run, true)
        .spawn()
        .map_err(|error| Error::new(format!("Couldn't run {}: {error}", run.program)))?;
    let lines = Arc::new(Channel::new(KEPT_LINES));
    let errors = Arc::new(Channel::new(KEPT_LINES));
    if let Some(output) = child.stdout.take() {
        stream(output, Arc::clone(&lines));
    }
    if let Some(output) = child.stderr.take() {
        stream(output, Arc::clone(&errors));
    }
    let started = Arc::new(Started {
        input: Mutex::new(child.stdin.take()),
        child: Mutex::new(child),
    });
    let watched = Arc::clone(&started);
    let exit = heap.background(&format!("ox process/start {}", run.program), move || {
        exited(&watched)
    })?;
    let items = [
        Value::keyword("lines"),
        heap.channel(lines).map_err(full)?,
        Value::keyword("errors"),
        heap.channel(errors).map_err(full)?,
        Value::keyword("exit"),
        exit,
        Value::keyword("process"),
        heap.shared("process", started).map_err(full)?,
    ];
    heap.map_from(&items)
}

fn started<'a>(heap: &'a Heap, name: &str, process: Value) -> Result<&'a Started, Error> {
    heap.entries(process)
        .and_then(|entries| {
            entries.into_iter().find_map(|(key, value)| match key {
                Value::Keyword(key) if key.name() == "process" => Some(value),
                _ => None,
            })
        })
        .and_then(|value| heap.foreign_of::<Started>(value))
        .ok_or_else(|| wrong_for(heap, name, process))
}

fn wrong_for(heap: &Heap, name: &str, value: Value) -> Error {
    Error::new(format!(
        "{name} expects a process from process/start, got {}",
        print::write(heap, value)
    ))
    .of(ErrorKind::Type)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native_in(MODULE, "start", 2, None, start);
    isolate.define_native_in(MODULE, "write", 2, Some(2), |heap, arguments| {
        let text = print::display(heap, arguments[1]);
        let started = started(heap, "process/write", arguments[0])?;
        let mut input = locked(&started.input);
        let written = input.as_mut().is_some_and(|stdin| {
            stdin
                .write_all(text.as_bytes())
                .and_then(|()| stdin.flush())
                .is_ok()
        });
        Ok(Value::Bool(written))
    });
    isolate.define_native_in(MODULE, "close-input", 1, Some(1), |heap, arguments| {
        let started = started(heap, "process/close-input", arguments[0])?;
        locked(&started.input).take();
        Ok(Value::Nil)
    });
    isolate.define_native_in(MODULE, "kill", 1, Some(1), |heap, arguments| {
        let started = started(heap, "process/kill", arguments[0])?;
        let _ = locked(&started.child).kill();
        Ok(Value::Nil)
    });
    isolate.define_native_in(MODULE, "run", 2, None, |heap, arguments| {
        let run = parse(heap, "process/run", arguments)?;
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
