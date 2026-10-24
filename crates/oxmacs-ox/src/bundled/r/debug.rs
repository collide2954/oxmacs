use std::path::PathBuf;

use oxmacs_core::editor::{Editor, JobId};

use super::{PROMPT, R, with_program};

const BROWSE: &str = "Browse[1]> ";

fn debugging() -> (tempfile::TempDir, R, JobId, PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let script = folder.path().join("analysis.R");
    std::fs::write(
        &script,
        "double <- function(x) {\n  y <- x * 2\n  y\n}\ndouble(2)\n",
    )
    .unwrap();
    let mut r = with_program(None, Editor::new());
    r.editor.open_file(&script);
    let job = r.running();
    (folder, r, job, script)
}

fn line(r: &R) -> usize {
    r.editor.buffer().line_of(r.editor.buffer().point())
}

#[test]
fn stepping_shows_where_r_is_and_function_keys_step() {
    let (_folder, mut r, job, script) = debugging();
    let path = script.display().to_string().replace('\\', "/");

    r.output(
        job,
        &format!("debugging in: double(2)\ndebug at {path}#2: y <- x * 2\n{BROWSE}"),
    );

    assert_eq!(r.editor.buffer().name(), "analysis.R");
    assert_eq!(line(&r), 1);
    assert_eq!(
        r.editor.message(),
        Some("Debugging: <f10> next  S-<f4> step in  S-<f6> finish  S-<f5> continue  S-<f8> stop")
    );
    r.press("<f10>");
    assert_eq!(r.sent(), ["n\n"]);
    r.output(job, &format!("debug at analysis.R#3: y\n{BROWSE}"));
    assert_eq!(line(&r), 2);
    for (keys, command) in [("S-<f4>", "s"), ("S-<f6>", "f"), ("S-<f5>", "c")] {
        r.press(keys);
        assert_eq!(r.sent(), [format!("{command}\n")], "{keys}");
        r.output(job, BROWSE);
    }
    r.press("S-<f8>");
    assert_eq!(r.sent(), ["Q\n"]);
    r.output(job, PROMPT);
    r.press("<f10>");
    assert_eq!(r.editor.message(), Some("R is not debugging"));
    assert_eq!(r.sent(), Vec::<String>::new());
}

#[test]
fn functions_are_flagged_for_debugging_from_the_script() {
    let (_folder, mut r, _job, _script) = debugging();
    r.editor.go_to_line(5, 2);

    r.press("C-c C-t d");
    assert_eq!(r.sent(), ["debugonce(double)\n"]);
    r.press("C-c C-t u");
    assert_eq!(r.sent(), ["undebug(double)\n"]);
}
