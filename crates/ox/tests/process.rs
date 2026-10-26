use std::fs;
use std::path::Path;

fn evaluated(source: &str) -> String {
    let mut isolate = ox_library::isolate();
    let source = format!("(module (import (ox process))) {source}");
    match isolate.eval(&source) {
        Ok(value) => isolate.write(value),
        Err(error) => format!("error: {}", error.message),
    }
}

fn literal(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

fn ox() -> String {
    literal(Path::new(env!("CARGO_BIN_EXE_ox")))
}

#[test]
fn programs_run_in_a_folder_with_their_own_environment() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("hello.ox"),
        "(module (import (ox process))) (println \"in script\") (print (process/env \"GREETING\"))",
    )
    .unwrap();

    let ran = evaluated(&format!(
        "(let [ran (await (process/run {} [\"hello.ox\"] :cwd {} :env {{GREETING: \"hello\"}}))] [ran.status ran.stdout ran.stderr])",
        ox(),
        literal(dir.path())
    ));

    assert_eq!(ran, r#"[0 "in script\nhello" ""]"#);
}

#[test]
fn input_reaches_the_program() {
    let ran = evaluated(&format!(
        "(get (await (process/run {} [] :input \"(+ 1 2)\\n\")) :stdout)",
        ox()
    ));

    assert_eq!(ran, r#""ox> 3\nox> \n""#);
}

#[test]
fn failures_resolve_with_their_status() {
    let dir = tempfile::tempdir().unwrap();

    let ran = evaluated(&format!(
        "(let [ran (await (process/run {} [\"missing.ox\"] :cwd {}))] [ran.status (> (count ran.stderr) 0)])",
        ox(),
        literal(dir.path())
    ));

    assert_eq!(ran, "[1 true]");
}

#[test]
fn missing_programs_raise() {
    let ran = evaluated(
        "(try (await (process/run \"no-such-program-for-ox\" [\"x\"])) (catch error e e.message))",
    );

    assert!(
        ran.starts_with("\"Couldn't run no-such-program-for-ox: "),
        "{ran}"
    );
}

#[test]
fn the_environment_can_be_read() {
    assert_eq!(
        evaluated("[(string? (process/env \"PATH\")) (process/env \"OX_SURELY_NOT_SET\")]"),
        "[true nil]"
    );
    assert_eq!(
        evaluated("(process/run \"x\" \"not a vector\")"),
        "error: process/run expects a vector of arguments, got \"not a vector\""
    );
}
