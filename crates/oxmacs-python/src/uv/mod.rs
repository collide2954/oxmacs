mod facts;
mod page;

use std::ops::Range;
use std::path::{Path, PathBuf};

use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::{Editor, JobEvent, JobSpec};
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::tools;
use facts::{Facts, Lock, Probe};
use page::Rendered;

const NAME: &str = "*uv*";
const OUTSIDE: &str = "No pyproject.toml here or above";

#[derive(Debug, Clone, Copy)]
enum Query {
    Lock,
    Newer,
}

#[derive(Debug)]
pub(crate) struct UvState {
    pub(crate) scope: ScopeId,
}

#[derive(Debug)]
pub(crate) struct Dashboard {
    scope: ScopeId,
    root: PathBuf,
    facts: Facts,
    rendered: Rendered,
}

impl AppletBuffer for Dashboard {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn line_style(&self, line: usize) -> LineStyle {
        self.rendered.styles.get(line).cloned().unwrap_or_default()
    }

    fn pills(&self, line: usize) -> Vec<(Range<usize>, Highlight)> {
        self.rendered.pills.get(line).cloned().unwrap_or_default()
    }

    fn directory(&self) -> Option<&Path> {
        Some(&self.root)
    }

    fn fixed_name(&self) -> bool {
        true
    }
}

pub(crate) fn open(editor: &mut Editor) {
    let Some(root) = tools::project_root(&editor.default_directory()) else {
        editor.show_message(OUTSIDE);
        return;
    };
    let Some(scope) = editor.applet_state::<UvState>().map(|state| state.scope) else {
        return;
    };
    let found = editor.find_applet_buffer::<Dashboard>(|_| true);
    let id = match found {
        Some(id) => id,
        None => editor.add_applet_buffer_with(NAME, |buffer| {
            buffer.set_read_only(true);
            Dashboard {
                scope,
                root: root.clone(),
                facts: Facts::default(),
                rendered: Rendered::default(),
            }
        }),
    };
    if let Some((_, dashboard)) = editor.applet_buffer_mut::<Dashboard>(id) {
        dashboard.root = root;
    }
    editor.show_buffer(id);
    reload(editor, id);
}

pub(crate) fn refresh(editor: &mut Editor) {
    let id = editor.buffer().id();
    if editor.applet_buffer::<Dashboard>(id).is_some() {
        reload(editor, id);
    }
}

fn reload(editor: &mut Editor, id: BufferId) {
    let Some(root) = editor
        .applet_buffer::<Dashboard>(id)
        .map(|(_, dashboard)| dashboard.root.clone())
    else {
        return;
    };
    let mut facts = facts::read(&root);
    match tools::uv(editor) {
        Ok(uv) => {
            if facts.lock == Lock::Checking {
                query(editor, id, &uv, &root, &["lock", "--check"], Query::Lock);
            }
            if facts.newer == Probe::Checking {
                let args = ["pip", "list", "--outdated", "--format", "json"];
                query(editor, id, &uv, &root, &args, Query::Newer);
            }
        }
        Err(error) => {
            if facts.lock == Lock::Checking {
                facts.lock = Lock::Failed(error.clone());
            }
            if facts.newer == Probe::Checking {
                facts.newer = Probe::Failed(error.clone());
            }
            editor.show_message(error);
        }
    }
    if let Some((_, dashboard)) = editor.applet_buffer_mut::<Dashboard>(id) {
        dashboard.facts = facts;
    }
    render(editor, id);
}

fn query(editor: &mut Editor, id: BufferId, uv: &Path, root: &Path, args: &[&str], kind: Query) {
    let mut output: Vec<String> = Vec::new();
    let spec = JobSpec {
        program: uv.display().to_string(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        directory: root.to_owned(),
        env: Vec::new(),
        label: "uv",
        name: format!("uv {}", args.join(" ")),
        buffer: None,
        interactive: false,
    };
    editor.start_job(spec, move |editor, _, event| match event {
        JobEvent::Line(line) | JobEvent::Output(line) => output.push(line.to_owned()),
        JobEvent::Finished(outcome) => {
            let Some((_, dashboard)) = editor.applet_buffer_mut::<Dashboard>(id) else {
                return;
            };
            match kind {
                Query::Lock => dashboard.facts.lock = facts::lock(outcome, &output),
                Query::Newer => dashboard.facts.newer = facts::newer(outcome, &output),
            }
            render(editor, id);
        }
    });
}

fn render(editor: &mut Editor, id: BufferId) {
    let home = editor.home().map(Path::to_path_buf);
    let Some((buffer, dashboard)) = editor.applet_buffer_mut::<Dashboard>(id) else {
        return;
    };
    let line = buffer.line_of(buffer.point());
    dashboard.rendered = page::render(&dashboard.facts, home.as_deref());
    buffer.set_text(&dashboard.rendered.text);
    let last = dashboard.rendered.rows.len().saturating_sub(1);
    let start = buffer.line_start(line.min(last));
    buffer.set_point(start);
}

#[cfg(test)]
pub(crate) mod testing {
    use std::fs;
    use std::path::PathBuf;

    use oxmacs_core::config::Config;
    use oxmacs_core::editor::{Editor, Effect, Job, Outcome};

    use crate::testing::{installed, press};

    pub(crate) const PYPROJECT: &str =
        "[project]\nname = \"demo\"\ndependencies = [\"requests>=2.31\", \"numpy\"]\n";

    pub(crate) const LOCK: &str = "version = 1\n\n[[package]]\nname = \"demo\"\nversion = \"0.1.0\"\nsource = { editable = \".\" }\n\n[[package]]\nname = \"numpy\"\nversion = \"2.1.0\"\n\n[[package]]\nname = \"requests\"\nversion = \"2.32.3\"\n";

    pub(crate) struct Project {
        pub(crate) _dir: tempfile::TempDir,
        pub(crate) root: PathBuf,
        pub(crate) uv: PathBuf,
        pub(crate) editor: Editor,
    }

    pub(crate) fn project(files: &[(&str, &str)]) -> Project {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("demo");
        fs::create_dir_all(root.join("src")).unwrap();
        for (name, text) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        let uv = dir
            .path()
            .join(format!("uv{}", std::env::consts::EXE_SUFFIX));
        fs::write(&uv, "").unwrap();
        let file = root.join("src").join("app.py");
        fs::write(&file, "x = 1\n").unwrap();
        let mut editor = installed();
        editor.set_home_directory(None);
        editor
            .apply_config(&Config::parse(&format!("[python]\nuv = '{}'\n", uv.display())).unwrap());
        editor.open_file(&file);
        Project {
            _dir: dir,
            root,
            uv,
            editor,
        }
    }

    pub(crate) fn full() -> Project {
        project(&[
            ("pyproject.toml", PYPROJECT),
            ("uv.lock", LOCK),
            (".venv/pyvenv.cfg", "version_info = 3.12.4\n"),
        ])
    }

    pub(crate) fn spawned(editor: &mut Editor) -> Vec<Job> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Spawn(job) => Some(job),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn finish(editor: &mut Editor, job: &Job, code: i32, output: &[&str]) {
        for line in output {
            editor.job_output(job.id, line);
        }
        editor.job_finished(job.id, &Outcome::Exited(Some(code)));
    }

    pub(crate) fn opened(project: &mut Project) -> Vec<Job> {
        press(&mut project.editor, "C-c C-u");
        spawned(&mut project.editor)
    }

    pub(crate) fn line_with<'a>(lines: &'a [String], start: &str) -> Option<&'a str> {
        lines
            .iter()
            .find(|line| line.starts_with(start))
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::theme::Highlight;

    use super::testing::{PYPROJECT, finish, full, line_with, opened, project, spawned};
    use crate::testing::{lines, press};

    #[test]
    fn c_c_c_u_shows_the_project_and_checks_it_with_uv() {
        let mut project = full();

        let jobs = opened(&mut project);

        let editor = &project.editor;
        assert_eq!(editor.buffer().name(), "*uv*");
        assert!(editor.buffer().is_read_only());
        assert_eq!(editor.default_directory(), project.root);
        let shown = lines(editor);
        assert_eq!(line_with(&shown, "Project:"), Some("Project:  demo"));
        assert_eq!(line_with(&shown, "Python:"), Some("Python:   3.12.4"));
        assert_eq!(line_with(&shown, "Lock:"), Some("Lock:     checking…"));
        assert!(shown.contains(&"  requests>=2.31".to_owned()), "{shown:?}");
        assert!(shown.contains(&"  numpy     2.1.0".to_owned()), "{shown:?}");
        let commands: Vec<Vec<String>> = jobs.iter().map(|job| job.args.clone()).collect();
        assert_eq!(
            commands,
            [
                vec!["lock", "--check"],
                vec!["pip", "list", "--outdated", "--format", "json"]
            ]
        );
        assert!(
            jobs.iter()
                .all(|job| job.program == project.uv.display().to_string())
        );
        assert!(jobs.iter().all(|job| job.directory == project.root));
    }

    #[test]
    fn the_lock_check_and_newer_versions_fill_in() {
        let mut project = full();
        let jobs = opened(&mut project);

        finish(&mut project.editor, &jobs[0], 0, &[]);
        finish(
            &mut project.editor,
            &jobs[1],
            0,
            &[
                r#"[{"name":"requests","version":"2.32.3","latest_version":"2.33.0","latest_filetype":"wheel"}]"#,
            ],
        );

        let shown = lines(&project.editor);
        assert_eq!(line_with(&shown, "Lock:"), Some("Lock:     up to date"));
        let at = shown
            .iter()
            .position(|line| line == "Lock:     up to date")
            .unwrap();
        assert_eq!(
            project.editor.pane().line_pills(at),
            [(10..20, Highlight::Success)]
        );
        assert!(shown.contains(&"Newer (1)".to_owned()), "{shown:?}");
        assert!(
            shown.contains(&"  requests  2.32.3 → 2.33.0".to_owned()),
            "{shown:?}"
        );
    }

    #[test]
    fn a_stale_lock_is_marked() {
        let mut project = full();
        let jobs = opened(&mut project);

        finish(
            &mut project.editor,
            &jobs[0],
            1,
            &["error: The lockfile at `uv.lock` needs to be updated, but `--check` was provided."],
        );

        assert_eq!(
            line_with(&lines(&project.editor), "Lock:"),
            Some("Lock:     out of date")
        );
    }

    #[test]
    fn g_reads_the_project_again() {
        let mut project = full();
        opened(&mut project);
        fs::write(
            project.root.join("pyproject.toml"),
            PYPROJECT.replace("\"numpy\"", "\"numpy\", \"httpx\""),
        )
        .unwrap();

        press(&mut project.editor, "g");

        assert!(lines(&project.editor).contains(&"  httpx".to_owned()));
        assert_eq!(spawned(&mut project.editor).len(), 2);
    }

    #[test]
    fn without_a_lock_or_a_venv_there_is_nothing_to_check() {
        let mut project = project(&[("pyproject.toml", PYPROJECT)]);

        let jobs = opened(&mut project);

        let shown = lines(&project.editor);
        assert_eq!(line_with(&shown, "Lock:"), Some("Lock:     no uv.lock"));
        assert_eq!(line_with(&shown, "Python:"), Some("Python:   no .venv"));
        assert_eq!(jobs, []);
    }

    #[test]
    fn without_uv_the_files_still_show() {
        let mut project = full();
        project.editor.apply_config(
            &oxmacs_core::config::Config::parse("[python]\nuv = \"oxmacs-no-such-uv\"\n").unwrap(),
        );

        let jobs = opened(&mut project);

        assert_eq!(jobs, []);
        assert_eq!(
            project.editor.message(),
            Some("[python] uv not found: oxmacs-no-such-uv")
        );
        let shown = lines(&project.editor);
        assert_eq!(
            line_with(&shown, "Lock:"),
            Some("Lock:     [python] uv not found: oxmacs-no-such-uv")
        );
        assert_eq!(line_with(&shown, "Project:"), Some("Project:  demo"));
    }

    #[test]
    fn outside_a_project_there_is_no_dashboard() {
        let mut project = project(&[]);

        let jobs = opened(&mut project);

        assert_eq!(jobs, []);
        assert_eq!(project.editor.buffer().name(), "app.py");
        assert_eq!(
            project.editor.message(),
            Some("No pyproject.toml here or above")
        );
    }

    #[test]
    fn c_c_c_u_again_reuses_the_dashboard() {
        let mut project = full();
        opened(&mut project);
        press(&mut project.editor, "C-x b RET");

        opened(&mut project);

        assert_eq!(project.editor.buffer().name(), "*uv*");
        assert_eq!(
            project
                .editor
                .buffer_names()
                .iter()
                .filter(|name| **name == "*uv*")
                .count(),
            1
        );
    }
}
