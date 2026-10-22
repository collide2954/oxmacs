use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs};

use ox::span::line_column;
use ox::value::Value;
use ox::{Error, Isolate, read};

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    match args.next() {
        Some(first) if first == "test" => run_tests(&args.map(PathBuf::from).collect::<Vec<_>>()),
        Some(path) => {
            let path = PathBuf::from(path);
            let mut isolate = Isolate::new();
            isolate.add_root(path.parent().unwrap_or(Path::new(".")));
            run_file(&mut isolate, &path)
        }
        None => {
            let mut isolate = Isolate::new();
            isolate.add_root(".");
            repl(&mut isolate)
        }
    }
}

fn place(error: &Error, path: &Path, source: &str) -> String {
    match (error.traceback.first(), error.span) {
        (Some(frame), _) => {
            let (line, column) = frame.line_column();
            format!("{}:{line}:{column}", frame.file())
        }
        (None, Some(span)) => {
            let (line, column) = line_column(source, span.start);
            format!("{}:{line}:{column}", path.display())
        }
        (None, None) => path.display().to_string(),
    }
}

fn run_file(isolate: &mut Isolate, path: &Path) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let Err(error) = isolate.eval_named(&path.display().to_string(), &source) else {
        return ExitCode::SUCCESS;
    };
    eprintln!("{}: {}", place(&error, path, &source), error.message);
    if error.traceback.len() > 1 {
        for frame in &error.traceback {
            eprintln!("  in {frame}");
        }
    }
    ExitCode::FAILURE
}

fn collect(path: &Path, found: &mut Vec<PathBuf>) {
    if !path.is_dir() {
        found.push(path.to_owned());
        return;
    }
    for entry in fs::read_dir(path).into_iter().flatten().flatten() {
        let inner = entry.path();
        if inner.is_dir() {
            collect(&inner, found);
        } else if inner.extension().is_some_and(|extension| extension == "ox") {
            found.push(inner);
        }
    }
}

fn report(name: &str, error: &Error, path: &Path, source: &str) {
    println!("FAIL  {name}");
    println!("      {}", place(error, path, source));
    for line in error.message.lines() {
        println!("      {line}");
    }
}

fn field(isolate: &Isolate, map: Value, key: &str) -> Value {
    isolate
        .heap()
        .entries(map)
        .unwrap_or_default()
        .into_iter()
        .find(|(found, _)| *found == Value::keyword(key))
        .map_or(Value::Nil, |(_, value)| value)
}

fn run_tests(paths: &[PathBuf]) -> ExitCode {
    let mut files = Vec::new();
    let wanted: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.to_vec()
    };
    for path in &wanted {
        collect(path, &mut files);
    }
    files.sort();
    let (mut total, mut failed) = (0_usize, 0_usize);
    for path in files {
        let source = fs::read_to_string(&path).unwrap_or_default();
        let mut isolate = Isolate::new();
        isolate.add_root(path.parent().unwrap_or(Path::new(".")));
        if let Err(error) = isolate.eval_named(&path.display().to_string(), &source) {
            total += 1;
            failed += 1;
            report(&path.display().to_string(), &error, &path, &source);
            continue;
        }
        let Ok(tests) = isolate.eval("(vec @registered-tests)") else {
            continue;
        };
        for test in isolate.heap().elements(tests).unwrap_or_default() {
            let name = isolate.write(field(&isolate, test, "name"));
            let name = name.trim_matches('"').to_owned();
            let run = field(&isolate, test, "run");
            total += 1;
            match isolate.call(run, &[]) {
                Ok(_) => println!("ok    {name}"),
                Err(error) => {
                    failed += 1;
                    report(&name, &error, &path, &source);
                }
            }
        }
    }
    match (total, failed) {
        (1, 0) => println!("1 test passed"),
        (_, 0) => println!("{total} tests passed"),
        _ => println!("{total} tests, {failed} failed"),
    }
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn prompt(text: &str) {
    print!("{text}");
    let _ = io::stdout().flush();
}

fn repl(isolate: &mut Isolate) -> ExitCode {
    let mut lines = io::stdin().lock().lines();
    let mut pending = String::new();
    loop {
        prompt(if pending.is_empty() { "ox> " } else { "... " });
        let Some(Ok(line)) = lines.next() else {
            println!();
            return ExitCode::SUCCESS;
        };
        pending.push_str(&line);
        pending.push('\n');
        if pending.trim().is_empty() {
            pending.clear();
            continue;
        }
        if read::read(&pending).1.iter().any(|error| error.incomplete) {
            continue;
        }
        match isolate.eval(&pending) {
            Ok(value) => println!("{}", isolate.write(value)),
            Err(error) => eprintln!("error: {}", error.message),
        }
        pending.clear();
    }
}
