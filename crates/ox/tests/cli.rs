use std::fs;
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn ox() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ox"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

fn repl(input: &str) -> Output {
    let mut child = ox()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn files_run_and_print() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hello.ox");
    fs::write(
        &path,
        "(def (square x) (* x x))\n(println (square 7))\n(print \"done\")",
    )
    .unwrap();

    let output = ox().arg(&path).output().unwrap();

    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "49\ndone");
}

#[test]
fn errors_name_the_file_line_and_column() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.ox");
    fs::write(&path, "(def x 1)\n  (/ x 0)").unwrap();

    let output = ox().arg(&path).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stderr),
        format!("{}:2:3: Division by zero\n", path.display())
    );
}

#[test]
fn errors_inside_functions_print_each_frame() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deep.ox");
    fs::write(&path, "(def (f)\n  (/ 1 0))\n(f)\n").unwrap();

    let output = ox().arg(&path).output().unwrap();

    let file = path.display();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stderr),
        format!(
            "{file}:2:3: Division by zero\n  in f at {file}:2:3\n  in top level at {file}:3:1\n"
        )
    );
}

#[test]
fn missing_files_are_reported() {
    let output = ox().arg("no-such-file.ox").output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).starts_with("no-such-file.ox: "),
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn the_repl_prints_each_result_and_waits_for_closing_brackets() {
    let output = repl("(+ 1 2)\n(def x 5)\n\n(* x\n x)\n\"s\"\n(/ 1 0)\n");

    assert!(output.status.success());
    assert_eq!(
        text(&output.stdout),
        "ox> 3\nox> x\nox> ox> ... 25\nox> \"s\"\nox> ox> \n"
    );
    assert_eq!(text(&output.stderr), "error: Division by zero\n");
}
