use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::ExitCode;
use std::{env, fs};

use ox::span::line_column;
use ox::{Isolate, read};

fn main() -> ExitCode {
    let mut isolate = Isolate::new();
    match env::args_os().nth(1) {
        Some(path) => run_file(&mut isolate, Path::new(&path)),
        None => repl(&mut isolate),
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
    match (error.traceback.first(), error.span) {
        (Some(frame), _) => {
            let (line, column) = frame.line_column();
            eprintln!("{}:{line}:{column}: {}", frame.file(), error.message);
        }
        (None, Some(span)) => {
            let (line, column) = line_column(&source, span.start);
            eprintln!("{}:{line}:{column}: {}", path.display(), error.message);
        }
        (None, None) => eprintln!("{}: {}", path.display(), error.message),
    }
    if error.traceback.len() > 1 {
        for frame in &error.traceback {
            eprintln!("  in {frame}");
        }
    }
    ExitCode::FAILURE
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
