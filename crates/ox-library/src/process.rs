use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use crate::text;
use ox_lang::Isolate;
use ox_lang::build::full;
use ox_lang::error::{Error, ErrorKind};
use ox_lang::heap::Heap;
use ox_lang::portable::{Channel, Failure, Portable, Sent};
use ox_lang::symbol::Sym;
use ox_lang::value::{Value, print};

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
        print::brief(heap, value)
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
    let [cwd, env, input] = [arguments[2], arguments[3], arguments[4]];
    if cwd != Value::Nil {
        run.directory = Some(PathBuf::from(text(heap, name, cwd)?));
    }
    if env != Value::Nil {
        run.environment = environment(heap, env)?;
    }
    if input != Value::Nil {
        run.input = Some(text(heap, name, input)?.to_owned());
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
        print::brief(heap, value)
    ))
    .of(ErrorKind::Type)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "start")
        .arity(2, Some(2))
        .keywords(&["cwd", "env", "input"])
        .outside()
        .function(start);
    isolate
        .native(MODULE, "write")
        .arity(2, Some(2))
        .outside()
        .function(|heap, arguments| {
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
    isolate
        .native(MODULE, "close-input")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let started = started(heap, "process/close-input", arguments[0])?;
            locked(&started.input).take();
            Ok(Value::Nil)
        });
    isolate
        .native(MODULE, "kill")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let started = started(heap, "process/kill", arguments[0])?;
            let _ = locked(&started.child).kill();
            Ok(Value::Nil)
        });
    isolate
        .native(MODULE, "run")
        .arity(2, Some(2))
        .keywords(&["cwd", "env", "input"])
        .outside()
        .function(|heap, arguments| {
            let run = parse(heap, "process/run", arguments)?;
            let label = format!("ox process/run {}", run.program);
            heap.background(&label, move || execute(&run))
        });
    isolate
        .native(MODULE, "env")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let name = text(heap, "process/env", arguments[0])?.to_owned();
            match std::env::var(&name) {
                Ok(value) => heap.string(&value).map_err(full),
                Err(_) => Ok(Value::Nil),
            }
        });
    isolate
        .native(MODULE, "which")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let program = text(heap, "process/which", arguments[0])?.to_owned();
            match which(&program) {
                Some(path) => heap.string(&path.display().to_string()).map_err(full),
                None => Ok(Value::Nil),
            }
        });
    isolate
        .native(MODULE, "os")
        .arity(0, Some(0))
        .outside()
        .function(|_, _| Ok(Value::keyword(std::env::consts::OS)));
}

fn which(program: &str) -> Option<PathBuf> {
    let names = |directory: &std::path::Path| {
        let mut names = vec![directory.join(program)];
        if cfg!(windows) {
            names.insert(0, directory.join(format!("{program}.exe")));
        }
        names
    };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .flat_map(|directory| names(&directory))
        .find(|candidate| candidate.is_file())
}
