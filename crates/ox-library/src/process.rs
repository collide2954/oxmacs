use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::text;
use ox_lang::Isolate;
use ox_lang::error::{Error, ErrorKind};
use ox_lang::heap::Heap;
use ox_lang::native::name_of;
use ox_lang::portable::{Channel, Failure, Portable, Sent};
use ox_lang::value::{Value, print};
use oxmacs_process::{Process, Started};

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

fn wrong(heap: &Heap, expected: &str, value: Value) -> Error {
    Error::new(format!(
        "process/run expects {expected}, got {}",
        print::brief(heap, value)
    ))
    .of(ErrorKind::Type)
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

fn command(run: &Run) -> Command {
    let mut command = Command::new(&run.program);
    command.args(&run.arguments);
    if let Some(directory) = &run.directory {
        command.current_dir(directory);
    }
    for (name, value) in &run.environment {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    command
}

fn execute(run: &Run) -> Result<Portable, Failure> {
    let failed = |error: &dyn std::fmt::Display| Failure {
        value: None,
        message: format!("Couldn't run {}: {error}", run.program),
    };
    let output = oxmacs_process::run(&mut command(run), run.input.as_deref().map(str::as_bytes))
        .map_err(|error| failed(&error))?;
    let status = output
        .status
        .code()
        .map_or(Portable::Nil, |code| Portable::Int(i64::from(code)));
    let decoded = |bytes: &[u8]| Portable::String(String::from_utf8_lossy(bytes).into());
    Ok(Portable::Map(vec![
        (Portable::keyword("status"), status),
        (Portable::keyword("stdout"), decoded(&output.stdout)),
        (Portable::keyword("stderr"), decoded(&output.stderr)),
    ]))
}

fn stream(reader: impl Read + Send + 'static, lines: Arc<Channel>) {
    thread::spawn(move || {
        oxmacs_process::read_lines(reader, |line| {
            let item = Portable::String(line.into());
            while matches!(lines.send(item.clone()), Sent::Full) {
                thread::sleep(POLL);
            }
            true
        });
        lines.close();
    });
}

fn exited(process: &Process) -> Result<Portable, Failure> {
    let status = process.wait().map_err(|error| Failure {
        value: None,
        message: error.to_string(),
    })?;
    Ok(status
        .code()
        .map_or(Portable::Nil, |code| Portable::Int(i64::from(code))))
}

fn start(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let run = parse(heap, "process/start", arguments)?;
    let mut command = command(&run);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let Started {
        process,
        stdout,
        stderr,
    } = oxmacs_process::spawn(&mut command)
        .map_err(|error| Error::new(format!("Couldn't run {}: {error}", run.program)))?;
    if let Some(input) = run.input {
        let _ = process.send(input);
        process.close_input();
    }
    let lines = Arc::new(Channel::new(KEPT_LINES));
    let errors = Arc::new(Channel::new(KEPT_LINES));
    if let Some(output) = stdout {
        stream(output, Arc::clone(&lines));
    }
    if let Some(output) = stderr {
        stream(output, Arc::clone(&errors));
    }
    let watched = process.clone();
    let exit = heap.background(&format!("ox process/start {}", run.program), move || {
        exited(&watched)
    })?;
    let items = [
        Value::keyword("lines"),
        heap.channel(lines)?,
        Value::keyword("errors"),
        heap.channel(errors)?,
        Value::keyword("exit"),
        exit,
        Value::keyword("process"),
        heap.shared("process", Arc::new(process))?,
    ];
    heap.map_from(&items)
}

fn started<'a>(heap: &'a Heap, name: &str, process: Value) -> Result<&'a Process, Error> {
    heap.entries(process)
        .and_then(|entries| {
            entries.into_iter().find_map(|(key, value)| match key {
                Value::Keyword(key) if key.name() == "process" => Some(value),
                _ => None,
            })
        })
        .and_then(|value| heap.foreign_of::<Process>(value))
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
            let process = started(heap, "process/write", arguments[0])?;
            Ok(Value::Bool(process.send(text).is_ok()))
        });
    isolate
        .native(MODULE, "close-input")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            started(heap, "process/close-input", arguments[0])?.close_input();
            Ok(Value::Nil)
        });
    isolate
        .native(MODULE, "kill")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            started(heap, "process/kill", arguments[0])?.kill();
            Ok(Value::Nil)
        });
    isolate
        .native(MODULE, "stop")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            started(heap, "process/stop", arguments[0])?.terminate();
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
                Ok(value) => heap.string(&value).map_err(Error::from),
                Err(_) => Ok(Value::Nil),
            }
        });
    isolate
        .native(MODULE, "which")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let program = text(heap, "process/which", arguments[0])?.to_owned();
            match oxmacs_process::which(&program) {
                Some(path) => heap
                    .string(&path.display().to_string())
                    .map_err(Error::from),
                None => Ok(Value::Nil),
            }
        });
    isolate
        .native(MODULE, "os")
        .arity(0, Some(0))
        .outside()
        .function(|_, _| Ok(Value::keyword(std::env::consts::OS)));
}
