use std::ffi::OsString;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs};

use ox::check::report::{self, Format};
use ox::check::{Diagnostic, Severity};
use ox::error::ErrorKind;
use ox::span::line_column;
use ox::value::Value;
use ox::{Error, Isolate, read};
use oxmacs_ox::checker;

const TEST_TASKS: std::time::Duration = std::time::Duration::from_secs(10);

#[global_allocator]
static ALLOCATOR: ox_perf::Counting = ox_perf::Counting;

const USAGE: &str = "usage: ox [FILE]
       ox SUBCOMMAND ...

  ox                     open a REPL
  ox FILE                run a file
  ox check PATH...       find mistakes without running anything
  ox fmt [--check] PATH...
                         format files, or list the ones that aren't
  ox test PATH...        run the test blocks in files or folders
  ox profile FILE        run a file and show where the time went
  ox perf bench FILE     benchmark the functions a file returns
  ox perf sample FILE NAME
                         sample where one benchmark spends its time
  ox lsp                 serve Ox to editors over the Language Server Protocol
  ox --version           print the version
";

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    match args.next() {
        Some(first) if first == "--help" || first == "-h" || first == "help" => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(first) if first == "--version" || first == "-V" => {
            println!("ox {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(first) if first == "test" => run_tests(&args.map(PathBuf::from).collect::<Vec<_>>()),
        Some(first) if first == "check" => check_command(args.collect()),
        Some(first) if first == "fmt" => fmt_command(args.collect()),
        Some(first) if first == "lsp" => language_server(),
        Some(first) if first == "perf" => {
            ox_perf::main(args.map(|argument| argument.to_string_lossy().into_owned()))
        }
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
            add_roots(&mut isolate, &path);
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
        let mut isolate = isolate_for(file, &roots);
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

fn language_server() -> ExitCode {
    let options = ox_lsp::Options {
        flavor: |path| {
            if checker::editor_code(path) {
                ox_lsp::Flavor::Editor
            } else {
                ox_lsp::Flavor::Plain
            }
        },
        editor: oxmacs_ox::checking_isolate,
        roots: checker::check_roots,
    };
    match ox_lsp::serve(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ox lsp: {error}");
            ExitCode::FAILURE
        }
    }
}

const FMT_USAGE: &str = "usage: ox fmt [--check] PATH...";

fn fmt_command(arguments: Vec<OsString>) -> ExitCode {
    let mut check = false;
    let mut files = Vec::new();
    for argument in arguments {
        if argument == "--check" {
            check = true;
        } else {
            ox_files(Path::new(&argument), &mut files);
        }
    }
    if files.is_empty() {
        eprintln!("{FMT_USAGE}");
        return ExitCode::from(2);
    }
    let mut changed = 0;
    let mut failed = false;
    for file in &files {
        let source = match fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("{}: {error}", file.display());
                failed = true;
                continue;
            }
        };
        let forms = isolate_for(file, &[]).forms_for(&file.display().to_string(), &source);
        let formatted = match ox::format::format_with(&source, &forms) {
            Ok(formatted) => formatted,
            Err(problem) => {
                eprintln!("{}:{problem}", file.display());
                failed = true;
                continue;
            }
        };
        if formatted == source {
            continue;
        }
        changed += 1;
        if check {
            println!("{}", file.display());
        } else if let Err(error) = fs::write(file, formatted) {
            eprintln!("{}: {error}", file.display());
            failed = true;
        }
    }
    if failed {
        return ExitCode::from(2);
    }
    if check {
        return if changed > 0 {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }
    let plural = if files.len() == 1 { "" } else { "s" };
    println!("Formatted {changed} of {} file{plural}", files.len());
    ExitCode::SUCCESS
}

fn profile_file(path: &Path) -> ExitCode {
    let mut isolate = Isolate::new();
    add_roots(&mut isolate, path);
    isolate.start_profiling();
    let code = run_file(&mut isolate, path);
    if let Some(profile) = isolate.stop_profiling() {
        print!("{}", profile.report());
    }
    code
}

fn isolate_for(file: &Path, roots: &[PathBuf]) -> Isolate {
    let mut isolate = if checker::editor_code(file) {
        oxmacs_ox::checking_isolate()
    } else {
        Isolate::new()
    };
    for root in checker::check_roots(file).iter().chain(roots) {
        isolate.add_root(root);
    }
    isolate
}

fn add_roots(isolate: &mut Isolate, path: &Path) {
    for root in checker::check_roots(path) {
        isolate.add_root(root);
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
        let unhandled = isolate.finish_tasks(None);
        for error in &unhandled {
            eprintln!("{}: {}", place(error, path, &source), error.message);
        }
        return if unhandled.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    };
    eprintln!("{}: {}", place(&error, path, &source), error.message);
    if error.traceback.len() > 1 {
        for frame in &error.traceback {
            eprintln!("  in {frame}");
        }
    }
    ExitCode::FAILURE
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
        ox_files(path, &mut files);
    }
    files.retain(|file| file.file_name().is_none_or(|name| name != "plugin.ox"));
    files.sort();
    let (mut total, mut failed) = (0_usize, 0_usize);
    for path in files {
        let source = fs::read_to_string(&path).unwrap_or_default();
        let mut isolate = if checker::editor_code(&path) {
            oxmacs_ox::checking_isolate()
        } else {
            Isolate::new()
        };
        add_roots(&mut isolate, &path);
        let file = path.display().to_string();
        if let Err(error) = isolate.eval_named(&file, &source) {
            if error.kind == ErrorKind::Refused {
                println!("skip  {}: it needs the editor", path.display());
                continue;
            }
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
            if isolate.file_of(run).is_some_and(|defined| defined != file) {
                continue;
            }
            total += 1;
            let ran = isolate.call(run, &[]).and_then(|_| {
                match isolate.finish_tasks(Some(TEST_TASKS)).into_iter().next() {
                    Some(error) => Err(error),
                    None => Ok(()),
                }
            });
            match ran {
                Ok(()) => println!("ok    {name}"),
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
