mod args;

use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;
use std::{env, fs};

use ox_lang::check::report::{self, Format};
use ox_lang::check::{Diagnostic, Severity};
use ox_lang::ide::ox_files;
use ox_lang::{Error, Isolate, read, testing};
use oxmacs_api as checker;

use args::Parsed;

#[global_allocator]
static ALLOCATOR: ox_perf::Counting = ox_perf::Counting;

struct Subcommand {
    name: &'static str,
    usage: &'static str,
    summary: &'static str,
    flags: &'static [&'static str],
    valued: &'static [&'static str],
    run: fn(&Parsed) -> Result<ExitCode, String>,
}

const SUBCOMMANDS: &[Subcommand] = &[
    Subcommand {
        name: "check",
        usage: "[--format human|json|github] [--strict] [--root DIR]... PATH...",
        summary: "find mistakes without running anything",
        flags: &["--strict"],
        valued: &["--format", "--root"],
        run: check,
    },
    Subcommand {
        name: "fmt",
        usage: "[--check] PATH...",
        summary: "format files, or list the ones that aren't",
        flags: &["--check"],
        valued: &[],
        run: fmt,
    },
    Subcommand {
        name: "test",
        usage: "[PATH]...",
        summary: "run the test blocks in files or folders",
        flags: &[],
        valued: &[],
        run: test,
    },
    Subcommand {
        name: "profile",
        usage: "FILE",
        summary: "run a file and show where the time went",
        flags: &[],
        valued: &[],
        run: profile,
    },
    Subcommand {
        name: "perf bench",
        usage: "FILE [NAME]... [--root DIR]...",
        summary: "benchmark the functions a file returns",
        flags: &[],
        valued: &["--root"],
        run: bench,
    },
    Subcommand {
        name: "perf sample",
        usage: "FILE NAME [--seconds N] [--root DIR]...",
        summary: "sample where one benchmark spends its time",
        flags: &[],
        valued: &["--seconds", "--root"],
        run: sample,
    },
    Subcommand {
        name: "lsp",
        usage: "",
        summary: "serve Ox to editors over the Language Server Protocol",
        flags: &[],
        valued: &[],
        run: language_server,
    },
];

fn line(shown: &str, summary: &str) -> String {
    if shown.len() < 23 {
        format!("  {shown:<23}{summary}\n")
    } else {
        format!("  {shown}\n  {:<23}{summary}\n", "")
    }
}

fn written(subcommand: &Subcommand) -> String {
    format!("ox {} {}", subcommand.name, subcommand.usage)
        .trim_end()
        .to_owned()
}

fn usage() -> String {
    let mut shown = "usage: ox [FILE]\n       ox SUBCOMMAND ...\n\n".to_owned();
    shown.push_str(&line("ox", "open a REPL"));
    shown.push_str(&line("ox FILE", "run a file"));
    for subcommand in SUBCOMMANDS {
        shown.push_str(&line(&written(subcommand), subcommand.summary));
    }
    shown.push_str(&line("ox --version", "print the version"));
    shown
}

fn chosen(arguments: &[OsString]) -> Option<(&'static Subcommand, usize)> {
    SUBCOMMANDS.iter().find_map(|subcommand| {
        let words: Vec<&str> = subcommand.name.split(' ').collect();
        let matches = arguments.len() >= words.len()
            && words
                .iter()
                .zip(arguments)
                .all(|(word, given)| given == *word);
        matches.then_some((subcommand, words.len()))
    })
}

fn refused(problem: &str, usages: &[String]) -> ExitCode {
    eprintln!("{problem}");
    for (index, shown) in usages.iter().enumerate() {
        let lead = if index == 0 { "usage:" } else { "      " };
        eprintln!("{lead} {shown}");
    }
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let arguments: Vec<OsString> = env::args_os().skip(1).collect();
    let Some(first) = arguments.first() else {
        let mut isolate = oxmacs_api::plain_isolate();
        isolate.add_root(".");
        return repl(&mut isolate);
    };
    if let Some((subcommand, words)) = chosen(&arguments) {
        let parsed = args::parse(&arguments[words..], subcommand.flags, subcommand.valued);
        return match parsed.and_then(|parsed| (subcommand.run)(&parsed)) {
            Ok(code) => code,
            Err(problem) => refused(
                &format!("ox {}: {problem}", subcommand.name),
                &[written(subcommand)],
            ),
        };
    }
    match first.to_str() {
        Some("--help" | "-h" | "help") => {
            print!("{}", usage());
            ExitCode::SUCCESS
        }
        Some("--version" | "-V") => {
            println!("ox {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--") if arguments.len() > 1 => run_path(Path::new(&arguments[1])),
        Some(word)
            if SUBCOMMANDS
                .iter()
                .any(|known| known.name.starts_with(&format!("{word} "))) =>
        {
            let usages: Vec<String> = SUBCOMMANDS
                .iter()
                .filter(|known| known.name.starts_with(&format!("{word} ")))
                .map(written)
                .collect();
            refused(&format!("ox {word}: which one?"), &usages)
        }
        Some(option) if option.starts_with('-') => refused(
            &format!("ox: unknown option {option}"),
            &["ox --help".to_owned()],
        ),
        _ => run_path(Path::new(first)),
    }
}

fn run_path(path: &Path) -> ExitCode {
    let mut isolate = oxmacs_api::plain_isolate();
    add_roots(&mut isolate, path);
    run_file(&mut isolate, path)
}

fn files(paths: &[OsString]) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    for path in paths {
        let path = Path::new(path);
        if !path.exists() {
            return Err(format!("{} doesn't exist", path.display()));
        }
        found.extend(ox_files(path, usize::MAX));
    }
    Ok(found)
}

fn some_paths(parsed: &Parsed) -> Result<Vec<PathBuf>, String> {
    if parsed.positional.is_empty() {
        return Err("give at least one PATH".to_owned());
    }
    files(&parsed.positional)
}

fn source_of(file: &Path) -> Result<String, String> {
    fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))
}

fn check(parsed: &Parsed) -> Result<ExitCode, String> {
    let format = match parsed.value("--format") {
        None => Format::Human,
        Some(name) => name.to_str().and_then(Format::named).ok_or_else(|| {
            format!(
                "--format is human, json or github, not {}",
                name.to_string_lossy()
            )
        })?,
    };
    let strict = parsed.flag("--strict");
    let roots: Vec<PathBuf> = parsed.values("--root").map(PathBuf::from).collect();
    let files = some_paths(parsed)?;
    let mut checked = Vec::new();
    for file in &files {
        let source = source_of(file)?;
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
    Ok(if errors > 0 || (strict && warnings > 0) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn language_server(parsed: &Parsed) -> Result<ExitCode, String> {
    if !parsed.positional.is_empty() {
        return Err("takes no arguments".to_owned());
    }
    let options = ox_lsp::Options {
        flavor: |path| {
            if checker::editor_code(path) {
                ox_lsp::Flavor::Editor
            } else {
                ox_lsp::Flavor::Plain
            }
        },
        plain: oxmacs_api::plain_isolate,
        editor: oxmacs_api::editor_isolate,
        roots: checker::check_roots,
    };
    match ox_lsp::serve(&options) {
        Ok(()) => Ok(ExitCode::SUCCESS),
        Err(error) => {
            eprintln!("ox lsp: {error}");
            Ok(ExitCode::FAILURE)
        }
    }
}

fn fmt(parsed: &Parsed) -> Result<ExitCode, String> {
    let check = parsed.flag("--check");
    let files = some_paths(parsed)?;
    let mut changed = 0;
    let mut failed = false;
    for file in &files {
        let source = match source_of(file) {
            Ok(source) => source,
            Err(problem) => {
                eprintln!("{problem}");
                failed = true;
                continue;
            }
        };
        let forms = isolate_for(file, &[]).forms_for(&file.display().to_string(), &source);
        let formatted = match ox_lang::format::format_with(&source, &forms) {
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
        return Ok(ExitCode::from(2));
    }
    if check {
        return Ok(if changed > 0 {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        });
    }
    let plural = if files.len() == 1 { "" } else { "s" };
    println!("Formatted {changed} of {} file{plural}", files.len());
    Ok(ExitCode::SUCCESS)
}

fn one_file(parsed: &Parsed) -> Result<PathBuf, String> {
    match parsed.positional.as_slice() {
        [file] => Ok(PathBuf::from(file)),
        _ => Err("give one FILE".to_owned()),
    }
}

fn profile(parsed: &Parsed) -> Result<ExitCode, String> {
    let path = one_file(parsed)?;
    let mut isolate = oxmacs_api::plain_isolate();
    add_roots(&mut isolate, &path);
    isolate.start_profiling();
    let code = run_file(&mut isolate, &path);
    if let Some(profile) = isolate.stop_profiling() {
        print!("{}", profile.report());
    }
    Ok(code)
}

fn printed(report: Result<String, String>) -> ExitCode {
    match report {
        Ok(report) => {
            print!("{report}");
            ExitCode::SUCCESS
        }
        Err(problem) => {
            eprintln!("{problem}");
            ExitCode::FAILURE
        }
    }
}

fn bench(parsed: &Parsed) -> Result<ExitCode, String> {
    let [file, names @ ..] = parsed.positional.as_slice() else {
        return Err("give a FILE".to_owned());
    };
    let names: Vec<String> = names
        .iter()
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    let roots: Vec<PathBuf> = parsed.values("--root").map(PathBuf::from).collect();
    Ok(printed(ox_perf::bench(Path::new(file), &names, &roots)))
}

fn sample(parsed: &Parsed) -> Result<ExitCode, String> {
    let [file, name] = parsed.positional.as_slice() else {
        return Err("give a FILE and a NAME".to_owned());
    };
    let seconds = match parsed.value("--seconds") {
        None => 4.0,
        Some(given) => given
            .to_str()
            .and_then(|text| text.parse::<f64>().ok())
            .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
            .ok_or_else(|| format!("--seconds needs a number, got {}", given.to_string_lossy()))?,
    };
    let roots: Vec<PathBuf> = parsed.values("--root").map(PathBuf::from).collect();
    Ok(printed(ox_perf::sample(
        Path::new(file),
        &name.to_string_lossy(),
        &roots,
        Duration::from_secs_f64(seconds),
    )))
}

fn isolate_for(file: &Path, roots: &[PathBuf]) -> Isolate {
    let mut isolate = oxmacs_api::isolate_for(file);
    for root in roots {
        isolate.add_root(root);
    }
    isolate
}

fn add_roots(isolate: &mut Isolate, path: &Path) {
    for root in checker::check_roots(path) {
        isolate.add_root(root);
    }
}

fn place(error: &Error, file: &str, source: &str) -> String {
    error.place(Some((file, source))).unwrap_or_default()
}

fn run_file(isolate: &mut Isolate, path: &Path) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let file = path.display().to_string();
    let Err(error) = isolate.eval_named(&file, &source) else {
        let unhandled = isolate.finish_tasks(None);
        for error in &unhandled {
            eprintln!("{}: {}", place(error, &file, &source), error.message);
        }
        return if unhandled.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    };
    let mut shown = format!("{}: {}", place(&error, &file, &source), error.message);
    if error.traceback.len() > 1 {
        for frame in &error.traceback {
            let _ = write!(shown, "\n  in {frame}");
        }
    }
    eprintln!("{shown}");
    ExitCode::FAILURE
}

fn test(parsed: &Parsed) -> Result<ExitCode, String> {
    let paths = if parsed.positional.is_empty() {
        vec![OsString::from(".")]
    } else {
        parsed.positional.clone()
    };
    let mut files = files(&paths)?;
    files.retain(|file| file.file_name().is_none_or(|name| name != "plugin.ox"));
    files.sort();
    let mut outcomes = Vec::new();
    for path in files {
        let source = source_of(&path)?;
        let mut isolate = oxmacs_api::isolate_for(&path);
        let file = path.display().to_string();
        let ran = testing::run_file(&mut isolate, &file, &source);
        for line in testing::report(&ran, &file, &source) {
            println!("{line}");
        }
        outcomes.extend(ran);
    }
    println!("{}", testing::summary(&outcomes));
    Ok(if testing::failed(&outcomes) == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
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
