use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

use oxmacs_core::editor::{
    Diagnostic, Editor, FormatRequest, Formatter, LintLine, LintRun, Linter, Outcome, Severity,
};
use serde::Deserialize;

use crate::tools;

const ERRORS: &[&str] = &["F821", "E902", "E999"];

pub(crate) const FORMAT: Formatter = Formatter {
    name: "ruff",
    format,
    regions: true,
};

pub(crate) const LINT: Linter = Linter {
    start,
    read,
    complete: |outcome| matches!(outcome, Outcome::Exited(Some(0 | 1))),
};

#[derive(Deserialize)]
struct Reported {
    code: Option<String>,
    message: String,
    filename: String,
    location: Spot,
    end_location: Spot,
}

#[derive(Deserialize)]
struct Spot {
    row: usize,
    column: usize,
}

fn format(editor: &Editor, request: &FormatRequest<'_>) -> Result<String, String> {
    let program = tools::ruff(editor)?;
    let mut command = Command::new(program);
    command
        .args(format_args(request.path))
        .current_dir(request.directory);
    pipe(command, request.text).map_err(|error| format!("ruff: {error}"))
}

fn format_args(path: &Path) -> Vec<OsString> {
    vec![
        "format".into(),
        "--stdin-filename".into(),
        path.as_os_str().to_owned(),
        "-".into(),
    ]
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_: &mut Command) {}

fn pipe(mut command: Command, text: &str) -> Result<String, String> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut command);
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot run {program}: {error}"))?;
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| format!("cannot write to {program}"))?;
    let output = thread::scope(|scope| {
        scope.spawn(move || input.write_all(text.as_bytes()));
        child.wait_with_output()
    })
    .map_err(|error| error.to_string())?;
    if output.status.success() {
        return String::from_utf8(output.stdout).map_err(|error| error.to_string());
    }
    let errors = String::from_utf8_lossy(&output.stderr);
    Err(errors
        .lines()
        .find(|line| !line.trim().is_empty())
        .map_or_else(|| output.status.to_string(), |line| line.trim().to_owned()))
}

fn start(editor: &Editor, path: &Path) -> Option<LintRun> {
    let program = tools::ruff(editor).ok()?;
    let folder = path.parent()?;
    let mut args: Vec<String> = ["check", "--output-format=json-lines", "--no-fix"]
        .map(str::to_owned)
        .to_vec();
    let root = tools::project_root(folder).unwrap_or_else(|| {
        args.push(path.display().to_string());
        folder.to_owned()
    });
    Some(LintRun {
        root,
        program: program.display().to_string(),
        args,
        label: "Ruff",
        name: "ruff check".to_owned(),
    })
}

fn read(line: &str, root: &Path) -> LintLine {
    let Some(reported) = line
        .starts_with('{')
        .then(|| serde_json::from_str::<Reported>(line).ok())
        .flatten()
    else {
        return LintLine::Other;
    };
    let severity = match reported.code.as_deref() {
        Some(code) if !ERRORS.contains(&code) => Severity::Warning,
        _ => Severity::Error,
    };
    let message = match &reported.code {
        Some(code) => format!("{} [{code}]", reported.message),
        None => reported.message,
    };
    LintLine::Diagnostic(Diagnostic {
        path: root.join(&reported.filename),
        start: (reported.location.row, reported.location.column),
        end: (reported.end_location.row, reported.end_location.column),
        severity,
        message,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use oxmacs_core::config::Config;
    use oxmacs_core::editor::{Diagnostic, Editor, Effect, Job, LintLine, Outcome, Severity};

    use super::*;
    use crate::testing::{installed, press, type_text};

    #[test]
    fn ruff_formats_from_standard_input_under_the_files_name() {
        assert_eq!(
            format_args(Path::new("src/app.py")),
            ["format", "--stdin-filename", "src/app.py", "-"]
                .map(OsString::from)
                .to_vec()
        );
    }

    fn echo() -> Command {
        if cfg!(windows) {
            let mut command = Command::new("findstr");
            command.arg("^");
            command
        } else {
            Command::new("cat")
        }
    }

    #[test]
    fn text_goes_through_the_program_and_comes_back() {
        assert_eq!(pipe(echo(), "x = 1\n"), Ok("x = 1\n".to_owned()));
        let missing = pipe(Command::new("oxmacs-no-such-ruff"), "x");
        assert!(
            missing
                .as_ref()
                .is_err_and(|error| error.starts_with("cannot run oxmacs-no-such-ruff")),
            "{missing:?}"
        );
    }

    fn configured(ruff: &Path) -> Config {
        Config::parse(&format!("[python]\nruff = '{}'\n", ruff.display())).unwrap()
    }

    #[test]
    fn c_c_c_f_formats_python_with_ruff() {
        let mut editor = installed();
        editor.apply_config(&configured(Path::new("oxmacs-no-such-ruff")));
        press(&mut editor, "M-x");
        type_text(&mut editor, "python-mode");
        press(&mut editor, "RET");
        type_text(&mut editor, "x=1");

        press(&mut editor, "C-c C-f");

        assert_eq!(editor.buffer().contents(), "x=1");
        assert_eq!(
            editor.message(),
            Some("[python] ruff not found: oxmacs-no-such-ruff")
        );
    }

    #[test]
    #[ignore = "needs ruff on PATH or from M-x list-tools"]
    fn ruff_formats_python_for_real() {
        let mut editor = installed();
        press(&mut editor, "M-x");
        type_text(&mut editor, "python-mode");
        press(&mut editor, "RET");
        type_text(&mut editor, "x=[1,2]");

        press(&mut editor, "C-c C-f");

        assert_eq!(editor.buffer().contents(), "x = [1, 2]\n");
        assert_eq!(editor.message(), Some("Formatted with ruff"));
    }

    fn reported(code: &str, message: &str) -> String {
        format!(
            r#"{{"cell":null,"code":{code},"end_location":{{"column":10,"row":1}},"filename":"src/app.py","fix":null,"location":{{"column":8,"row":1}},"message":"{message}","noqa_row":1,"url":null}}"#
        )
    }

    #[test]
    fn ruff_results_become_diagnostics() {
        let root = Path::new("root");

        assert_eq!(
            read(&reported("\"F401\"", "`os` imported but unused"), root),
            LintLine::Diagnostic(Diagnostic {
                path: root.join("src/app.py"),
                start: (1, 8),
                end: (1, 10),
                severity: Severity::Warning,
                message: "`os` imported but unused [F401]".to_owned(),
            })
        );
        let LintLine::Diagnostic(syntax) = read(&reported("null", "SyntaxError: oops"), root)
        else {
            panic!("no diagnostic");
        };
        assert_eq!(syntax.severity, Severity::Error);
        assert_eq!(syntax.message, "SyntaxError: oops");
        let LintLine::Diagnostic(undefined) = read(&reported("\"F821\"", "Undefined name"), root)
        else {
            panic!("no diagnostic");
        };
        assert_eq!(undefined.severity, Severity::Error);
        assert_eq!(read("Found 1 error.", root), LintLine::Other);
    }

    fn spawned(editor: &mut Editor) -> Vec<Job> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Spawn(job) => Some(job),
                _ => None,
            })
            .collect()
    }

    fn fake_ruff(folder: &Path) -> PathBuf {
        let path = folder.join(format!("ruff{}", std::env::consts::EXE_SUFFIX));
        fs::write(&path, "").unwrap();
        path
    }

    fn saved(editor: &mut Editor, file: &Path) {
        editor.open_file(file);
        press(editor, "C-e");
        type_text(editor, " ");
        press(editor, "C-x C-s");
    }

    #[test]
    fn saving_lints_the_project_with_ruff() {
        let project = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let ruff = fake_ruff(tools.path());
        fs::write(project.path().join("pyproject.toml"), "[project]\n").unwrap();
        fs::create_dir(project.path().join("src")).unwrap();
        let file = project.path().join("src").join("app.py");
        fs::write(&file, "import os\n").unwrap();
        let mut editor = installed();
        editor.apply_config(&configured(&ruff));

        saved(&mut editor, &file);

        let jobs = spawned(&mut editor);
        assert_eq!(jobs.len(), 1, "{jobs:?}");
        assert_eq!(jobs[0].program, ruff.display().to_string());
        assert_eq!(
            jobs[0].args,
            ["check", "--output-format=json-lines", "--no-fix"]
        );
        assert_eq!(jobs[0].directory, project.path());
        assert_eq!(editor.pane().status().lint.as_deref(), Some("Ruff"));
        editor.job_output(
            jobs[0].id,
            &reported("\"F401\"", "`os` imported but unused"),
        );
        editor.job_finished(jobs[0].id, &Outcome::Exited(Some(1)));
        assert_eq!(editor.pane().status().lint.as_deref(), Some("W1"));
        press(&mut editor, "C-a C-f C-f C-f C-f C-f C-f C-f C-f");
        assert_eq!(
            editor.diagnostic_at_point(),
            Some("`os` imported but unused [F401]")
        );
    }

    #[test]
    fn loose_files_are_linted_on_their_own() {
        let folder = tempfile::tempdir().unwrap();
        let tools = tempfile::tempdir().unwrap();
        let ruff = fake_ruff(tools.path());
        let file = folder.path().join("script.py");
        fs::write(&file, "x = 1\n").unwrap();
        let mut editor = installed();
        editor.apply_config(&configured(&ruff));

        saved(&mut editor, &file);

        let jobs = spawned(&mut editor);
        assert_eq!(
            jobs[0].args,
            [
                "check".to_owned(),
                "--output-format=json-lines".to_owned(),
                "--no-fix".to_owned(),
                file.display().to_string()
            ]
        );
        assert_eq!(jobs[0].directory, folder.path());
    }

    #[test]
    fn without_ruff_saving_lints_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("script.py");
        fs::write(&file, "x = 1\n").unwrap();
        let mut editor = installed();
        editor.apply_config(&configured(Path::new("oxmacs-no-such-ruff")));

        saved(&mut editor, &file);

        assert_eq!(spawned(&mut editor), []);
    }
}
