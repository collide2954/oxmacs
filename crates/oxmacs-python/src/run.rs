use std::path::PathBuf;

use oxmacs_core::editor::{Editor, ErrorAt};

use crate::tools;

const FILE: &str = "File \"";

pub(crate) fn traceback(line: &str) -> Option<ErrorAt> {
    let rest = line.trim_start().strip_prefix(FILE)?;
    let (path, rest) = rest.split_once("\", line ")?;
    if path.is_empty() || path.starts_with('<') {
        return None;
    }
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let start = line.len() - line.trim_start().len() + FILE.len();
    Some(ErrorAt {
        path: path.to_owned(),
        row: digits.parse().ok()?,
        column: 1,
        link: start..start + path.len(),
    })
}

fn quoted(word: &str) -> String {
    if word.is_empty() || word.contains(char::is_whitespace) {
        format!("\"{word}\"")
    } else {
        word.to_owned()
    }
}

fn prepared(editor: &mut Editor) -> Option<(String, PathBuf)> {
    match tools::interpreter(editor) {
        Ok(python) => {
            let folder = editor.default_directory();
            let root = tools::project_root(&folder).unwrap_or(folder);
            Some((quoted(&python.display().to_string()), root))
        }
        Err(error) => {
            editor.show_message(error);
            None
        }
    }
}

pub(crate) fn run_file(editor: &mut Editor) {
    let Some(file) = editor
        .buffer()
        .file()
        .map(|file| file.path.display().to_string())
    else {
        editor.show_message("This buffer is not visiting a file");
        return;
    };
    if let Some((python, root)) = prepared(editor) {
        editor.compile_in(format!("{python} {}", quoted(&file)), root);
    }
}

pub(crate) fn pytest(editor: &mut Editor) {
    if let Some((python, root)) = prepared(editor) {
        editor.compile_in(format!("{python} -m pytest --tb=native"), root);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use oxmacs_core::config::Config;
    use oxmacs_core::editor::{Editor, Effect, Job, Outcome};
    use oxmacs_core::theme::Highlight;

    use super::traceback;
    use crate::testing::{installed, press, type_text};

    fn found(line: &str) -> Option<(String, usize, std::ops::Range<usize>)> {
        traceback(line).map(|at| (at.path, at.row, at.link))
    }

    #[test]
    fn traceback_lines_point_at_their_file_and_line() {
        assert_eq!(
            found("  File \"C:\\work\\app.py\", line 12, in <module>"),
            Some(("C:\\work\\app.py".to_owned(), 12, 8..22))
        );
        assert_eq!(
            found("    File \"src/app.py\", line 3"),
            Some(("src/app.py".to_owned(), 3, 10..20))
        );
        assert_eq!(found("  File \"<frozen runpy>\", line 198, in _run"), None);
        assert_eq!(found("NameError: name 'y' is not defined"), None);
        assert_eq!(found("print(\"File \\\"a.py\\\", line 1\")"), None);
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

    fn venv_python(project: &Path) -> PathBuf {
        let folder = if cfg!(windows) {
            project.join(".venv").join("Scripts")
        } else {
            project.join(".venv").join("bin")
        };
        fs::create_dir_all(&folder).unwrap();
        let python = folder.join(format!("python{}", std::env::consts::EXE_SUFFIX));
        fs::write(&python, "").unwrap();
        python
    }

    fn project(name: &str) -> (tempfile::TempDir, PathBuf, PathBuf, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(name);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("pyproject.toml"), "[project]\n").unwrap();
        let python = venv_python(&root);
        let file = root.join("src").join("app.py");
        fs::write(&file, "x = 1\nprint(y)\n").unwrap();
        let mut editor = installed();
        editor.open_file(&file);
        (dir, root, python, editor)
    }

    #[test]
    fn c_c_c_r_runs_the_file_with_the_projects_python() {
        let (_dir, root, python, mut editor) = project("demo");

        press(&mut editor, "C-c C-r");

        let job = spawned(&mut editor).pop().unwrap();
        assert_eq!(job.program, python.display().to_string());
        assert_eq!(
            job.args,
            [root.join("src").join("app.py").display().to_string()]
        );
        assert_eq!(job.directory, root);
        assert_eq!(editor.tiling().windows().len(), 2);
    }

    #[test]
    fn c_c_c_t_runs_pytest_for_the_project() {
        let (_dir, root, python, mut editor) = project("demo");

        press(&mut editor, "C-c C-t");

        let job = spawned(&mut editor).pop().unwrap();
        assert_eq!(job.program, python.display().to_string());
        assert_eq!(job.args, ["-m", "pytest", "--tb=native"]);
        assert_eq!(job.directory, root);
    }

    #[test]
    fn paths_with_spaces_stay_whole() {
        let (_dir, root, python, mut editor) = project("my project");

        press(&mut editor, "C-c C-r");

        let job = spawned(&mut editor).pop().unwrap();
        assert_eq!(job.program, python.display().to_string());
        assert_eq!(
            job.args,
            [root.join("src").join("app.py").display().to_string()]
        );
    }

    #[test]
    fn tracebacks_in_the_output_are_clickable() {
        let (_dir, root, _python, mut editor) = project("demo");
        press(&mut editor, "C-c C-r");
        let job = spawned(&mut editor).pop().unwrap();
        let file = root.join("src").join("app.py");
        let traced = format!("  File \"{}\", line 2, in <module>", file.display());
        for text in [
            "Traceback (most recent call last):",
            &traced,
            "NameError: name 'y' is not defined",
        ] {
            editor.job_output(job.id, text);
        }
        editor.job_finished(job.id, &Outcome::Exited(Some(1)));
        let window = editor.tiling().windows()[1];
        let link = 8..8 + file.display().to_string().chars().count();
        assert_eq!(
            editor.pane_of(window).unwrap().line_style(4).spans,
            [(link, Highlight::Link)]
        );

        press(&mut editor, "M-g n");

        assert_eq!(
            editor.buffer().file().map(|file| file.path.clone()),
            Some(file)
        );
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 1);
    }

    #[test]
    fn buffers_without_a_file_have_nothing_to_run() {
        let mut editor = installed();
        press(&mut editor, "M-x");
        type_text(&mut editor, "python-mode");
        press(&mut editor, "RET");

        press(&mut editor, "C-c C-r");

        assert_eq!(editor.message(), Some("This buffer is not visiting a file"));
        assert_eq!(spawned(&mut editor), []);
    }

    #[test]
    fn a_missing_interpreter_is_named() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("script.py");
        fs::write(&file, "x = 1\n").unwrap();
        let mut editor = installed();
        editor.apply_config(
            &Config::parse("[python]\ninterpreter = \"oxmacs-no-such-python\"\n").unwrap(),
        );
        editor.open_file(&file);

        press(&mut editor, "C-c C-t");

        assert_eq!(
            editor.message(),
            Some("[python] interpreter not found: oxmacs-no-such-python")
        );
        assert_eq!(spawned(&mut editor), []);
    }
}
