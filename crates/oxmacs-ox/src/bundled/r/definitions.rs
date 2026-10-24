use std::fs;

use oxmacs_core::editor::{Editor, JobId};

use super::{PROMPT, R, ox_text, script, with_program};

fn line(r: &mut R, text: &str, name: &str) -> String {
    r.eval(&format!(
        "(module (import (r definition))) (definition/line {} {name:?})",
        ox_text(text)
    ))
}

#[test]
fn assignments_at_the_top_level_come_first() {
    let mut r = script("x\n", 0);
    let text = "f <- function(x) {\n  helper <- 2\n}\nhelper <- function() 1\n";

    assert_eq!(line(&mut r, text, "helper"), "3");
    assert_eq!(line(&mut r, "  helper <<- 3\n", "helper"), "0");
    assert_eq!(line(&mut r, "x = 1\n", "x"), "0");
    assert_eq!(line(&mut r, "`odd name` <- 1\n", "odd name"), "0");
}

#[test]
fn named_arguments_and_comparisons_are_not_definitions() {
    let mut r = script("x\n", 0);

    assert_eq!(line(&mut r, "plot(\n  data = x\n)\n", "data"), "nil");
    assert_eq!(line(&mut r, "x == 1\n", "x"), "nil");
    assert_eq!(line(&mut r, "xy <- 1\n", "x"), "nil");
    assert_eq!(line(&mut r, "x$y <- 1\n", "x"), "nil");
}

#[test]
fn m_dot_jumps_to_a_definition_in_the_buffer_and_m_comma_returns() {
    let mut r = script("add <- function(a, b) a + b\nx <- add(1, 2)\n", 33);

    r.press("M-.");
    assert_eq!(r.editor.buffer().point(), 0);

    r.press("M-,");
    assert_eq!(r.editor.buffer().point(), 33);
}

#[test]
fn definitions_are_found_in_the_projects_other_files() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join(".git")).unwrap();
    fs::create_dir_all(root.path().join("R")).unwrap();
    fs::write(
        root.path().join("R").join("utils.R"),
        "# helpers\n\nscale_to <- function(x, to) x / max(x) * to\n",
    )
    .unwrap();
    let script = root.path().join("analysis.R");
    fs::write(&script, "y <- scale_to(1:3, 10)\n").unwrap();
    let mut r = with_program(None, Editor::new());
    r.editor.open_file(&script);
    r.press("C-f C-f C-f C-f C-f C-f");

    r.press("M-.");

    assert_eq!(r.editor.buffer().name(), "utils.R");
    assert_eq!(r.editor.buffer().line_of(r.editor.buffer().point()), 2);
    r.press("M-,");
    assert_eq!(r.editor.buffer().name(), "analysis.R");
    assert_eq!(r.editor.buffer().point(), 6);
}

fn asking() -> (R, JobId) {
    let mut r = script("fit <- lm(y ~ x)\n", 8);
    let job = r.running();
    r.press("M-.");
    (r, job)
}

fn definition(r: &mut R, job: JobId, payload: &str) {
    r.output(job, &format!("\u{1}definition\u{2}{payload}\u{3}{PROMPT}"));
}

#[test]
fn the_session_shows_the_source_of_other_functions() {
    let (mut r, job) = asking();
    assert_eq!(r.sent(), ["getOption(\"oxmacs\")$definition(\"lm\")\n"]);

    definition(
        &mut r,
        job,
        "lm\tsource\tfunction (formula, data)\n{\n    NULL\n}",
    );

    assert_eq!(r.editor.buffer().name(), "*R source: lm*");
    assert_eq!(r.lines()[0], "function (formula, data)");
    assert!(r.editor.buffer().is_read_only());
    assert_eq!(r.eval("(buffer-language)"), "\"R\"");
    r.press("M-,");
    assert_eq!(r.editor.buffer().point(), 8);
}

#[test]
fn the_session_can_point_at_a_sourced_file() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("model.R");
    fs::write(&file, "\n\nfit_model <- function(d) lm(y ~ x, d)\n").unwrap();
    let (mut r, job) = asking();

    definition(&mut r, job, &format!("lm\tfile\t{}\t3", file.display()));

    assert_eq!(r.editor.buffer().name(), "model.R");
    assert_eq!(r.editor.buffer().line_of(r.editor.buffer().point()), 2);
}

#[test]
fn unknown_names_say_so() {
    let (mut r, job) = asking();

    definition(&mut r, job, "lm\t");

    assert_eq!(r.editor.message(), Some("No definition of lm found"));

    let mut r = script("zzz(1)\n", 0);
    r.press("M-.");
    assert_eq!(
        r.editor.message(),
        Some("No definition of zzz in open R buffers or the project")
    );
}
