use std::ffi::OsString;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs};

use ox::check::report::{self, Format};
use ox::check::{Diagnostic, Severity};
use ox::span::line_column;
use ox::value::Value;
use ox::{Error, Isolate, read};

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    match args.next() {
        Some(first) if first == "test" => run_tests(&args.map(PathBuf::from).collect::<Vec<_>>()),
        Some(first) if first == "check" => check_command(args.collect()),
        Some(first) if first == "profile" => {
            if let Some(path) = args.next() {
                profile_file(&PathBuf::from(path))
            } else {
                eprintln!("usage: ox profile FILE");
                ExitCode::FAILURE
            }
        }
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

const CHECK_USAGE: &str =
    "usage: ox check [--format human|json|github] [--strict] [--root DIR]... PATH...";

fn ox_files(path: &Path, into: &mut Vec<PathBuf>) {
    if !path.is_dir() {
        into.push(path.to_path_buf());
        return;
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            ox_files(&entry, into);
        } else if entry.extension().is_some_and(|extension| extension == "ox") {
            into.push(entry);
        }
    }
}

fn check_command(arguments: Vec<OsString>) -> ExitCode {
    let mut format = Format::Human;
    let mut strict = false;
    let mut roots = Vec::new();
    let mut files = Vec::new();
    let mut given = arguments.into_iter();
    while let Some(argument) = given.next() {
        match argument.to_str() {
            Some("--strict") => strict = true,
            Some("--format") => {
                let Some(named) = given.next().and_then(|name| Format::named(name.to_str()?))
                else {
                    eprintln!("{CHECK_USAGE}");
                    return ExitCode::from(2);
                };
                format = named;
            }
            Some("--root") => {
                let Some(root) = given.next() else {
                    eprintln!("{CHECK_USAGE}");
                    return ExitCode::from(2);
                };
                roots.push(PathBuf::from(root));
            }
            _ => ox_files(Path::new(&argument), &mut files),
        }
    }
    if files.is_empty() {
        eprintln!("{CHECK_USAGE}");
        return ExitCode::from(2);
    }
    let mut checked = Vec::new();
    for file in &files {
        let source = match fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("{}: {error}", file.display());
                return ExitCode::from(2);
            }
        };
        let plugin = file
            .parent()
            .is_some_and(|folder| folder.join("plugin.ox").is_file());
        let mut isolate = if plugin || file.file_name().is_some_and(|name| name == "init.ox") {
            oxmacs_ox::checking_isolate()
        } else {
            Isolate::new()
        };
        if let Some(folder) = file.parent() {
            isolate.add_root(folder);
            if folder.join("plugin.ox").is_file()
                && let Some(plugins) = folder.parent()
            {
                isolate.add_root(plugins);
            }
        }
        for root in &roots {
            isolate.add_root(root);
        }
        let name = file.display().to_string();
        let found = isolate.check(&name, &source);
        checked.push((name, source, found));
    }
    let findings: Vec<(&str, &str, &Diagnostic)> = checked
        .iter()
        .flat_map(|(name, source, found)| {
            found
                .iter()
                .map(move |diagnostic| (name.as_str(), source.as_str(), diagnostic))
        })
        .collect();
    let errors = findings
        .iter()
        .filter(|(_, _, diagnostic)| diagnostic.severity == Severity::Error)
        .count();
    let warnings = findings.len() - errors;
    match format {
        Format::Human => {
            for (name, source, diagnostic) in &findings {
                println!("{}", report::human(name, source, diagnostic));
            }
            println!("{}", report::summary(files.len(), errors, warnings));
        }
        Format::Json => println!("{}", report::json(&findings)),
        Format::Github => {
            for (name, source, diagnostic) in &findings {
                print!("{}", report::github(name, source, diagnostic));
            }
        }
    }
    if errors > 0 || (strict && warnings > 0) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn profile_file(path: &Path) -> ExitCode {
    let mut isolate = Isolate::new();
    isolate.add_root(path.parent().unwrap_or(Path::new(".")));
    isolate.start_profiling();
    let code = run_file(&mut isolate, path);
    if let Some(profile) = isolate.stop_profiling() {
        print!("{}", profile.report());
    }
    code
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
