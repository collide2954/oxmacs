mod facts;
mod page;

use std::ops::Range;
use std::path::{Path, PathBuf};

use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::{Editor, JobEvent, JobSpec};
use oxmacs_core::external::tools::split_command;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::tools;
use facts::{Facts, Installation, Lock, Probe};
use page::{Last, Rendered, Row};

const NAME: &str = "*uv*";
const OUTSIDE: &str = "No pyproject.toml here or above";
const NO_PACKAGE: &str = "No package on this line";
const NO_PYTHON: &str = "No Python version on this line";

#[derive(Debug, Clone, Copy)]
enum Query {
    Lock,
    Newer,
    Pythons,
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
    last: Option<Last>,
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
                last: None,
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

fn current(editor: &Editor) -> Option<BufferId> {
    let id = editor.buffer().id();
    editor.applet_buffer::<Dashboard>(id).map(|_| id)
}

fn package_at(dashboard: &Dashboard, line: usize) -> Option<String> {
    match dashboard.rendered.rows.get(line)? {
        Row::Required(index) => dashboard
            .facts
            .project
            .required
            .get(*index)
            .map(|requirement| facts::requirement_name(requirement)),
        Row::Locked(index) => dashboard
            .facts
            .locked
            .get(*index)
            .map(|(name, _)| name.clone()),
        Row::Newer(index) => match &dashboard.facts.newer {
            Probe::Known(newer) => newer.get(*index).map(|package| package.name.clone()),
            Probe::Checking | Probe::Failed(_) => None,
        },
        Row::Info | Row::Blank | Row::Heading | Row::Python(_) | Row::Output => None,
    }
}

fn with_package(editor: &mut Editor, args: impl FnOnce(String) -> Vec<String>) {
    let Some(id) = current(editor) else {
        return;
    };
    let line = editor.buffer().line_of(editor.buffer().point());
    let package = editor
        .applet_buffer::<Dashboard>(id)
        .and_then(|(_, dashboard)| package_at(dashboard, line));
    match package {
        Some(package) => act(editor, id, args(package)),
        None => editor.show_message(NO_PACKAGE),
    }
}

fn words(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

pub(crate) fn add(editor: &mut Editor) {
    let Some(id) = current(editor) else {
        return;
    };
    editor.prompt("Add dependency: ", move |editor, answer| {
        let requested = split_command(&answer);
        if !requested.is_empty() {
            act(editor, id, [words(&["add"]), requested].concat());
        }
    });
}

pub(crate) fn remove(editor: &mut Editor) {
    with_package(editor, |package| words(&["remove", &package]));
}

pub(crate) fn upgrade(editor: &mut Editor) {
    with_package(editor, |package| {
        words(&["lock", "--upgrade-package", &package])
    });
}

fn python_at(dashboard: &Dashboard, line: usize) -> Option<&Installation> {
    let Row::Python(index) = dashboard.rendered.rows.get(line)? else {
        return None;
    };
    match &dashboard.facts.pythons {
        Probe::Known(found) => found.get(*index),
        Probe::Checking | Probe::Failed(_) => None,
    }
}

fn with_python(editor: &mut Editor, args: impl FnOnce(&Installation) -> Vec<String>) {
    let Some(id) = current(editor) else {
        return;
    };
    let line = editor.buffer().line_of(editor.buffer().point());
    let chosen = editor
        .applet_buffer::<Dashboard>(id)
        .and_then(|(_, dashboard)| python_at(dashboard, line))
        .map(args);
    match chosen {
        Some(args) => act(editor, id, args),
        None => editor.show_message(NO_PYTHON),
    }
}

pub(crate) fn install(editor: &mut Editor) {
    with_python(editor, |python| words(&["python", "install", &python.key]));
}

pub(crate) fn pin(editor: &mut Editor) {
    with_python(editor, |python| {
        words(&["python", "pin", &python.request()])
    });
}

pub(crate) fn sync(editor: &mut Editor) {
    if let Some(id) = current(editor) {
        act(editor, id, words(&["sync"]));
    }
}

pub(crate) fn lock(editor: &mut Editor) {
    if let Some(id) = current(editor) {
        act(editor, id, words(&["lock"]));
    }
}

fn act(editor: &mut Editor, id: BufferId, args: Vec<String>) {
    let Some((_, dashboard)) = editor.applet_buffer::<Dashboard>(id) else {
        return;
    };
    if let Some(running) = dashboard
        .last
        .as_ref()
        .filter(|last| last.outcome.is_none())
    {
        let message = format!("{} is still running", running.command);
        editor.show_message(message);
        return;
    }
    let root = dashboard.root.clone();
    let uv = match tools::uv(editor) {
        Ok(uv) => uv,
        Err(error) => {
            editor.show_message(error);
            return;
        }
    };
    let command = format!("uv {}", args.join(" "));
    if let Some((_, dashboard)) = editor.applet_buffer_mut::<Dashboard>(id) {
        dashboard.last = Some(Last {
            command: command.clone(),
            output: Vec::new(),
            outcome: None,
        });
    }
    render(editor, id);
    let spec = JobSpec {
        program: uv.display().to_string(),
        args,
        directory: root,
        env: Vec::new(),
        label: "uv",
        name: command,
        buffer: None,
        interactive: false,
    };
    editor.start_job(spec, move |editor, _, event| {
        let Some(last) = editor
            .applet_buffer_mut::<Dashboard>(id)
            .and_then(|(_, dashboard)| dashboard.last.as_mut())
        else {
            return;
        };
        let finished = match event {
            JobEvent::Line(line) | JobEvent::Output(line) => {
                last.output.push(line.to_owned());
                false
            }
            JobEvent::Finished(outcome) => {
                last.outcome = Some(outcome.clone());
                true
            }
        };
        if finished {
            reload(editor, id);
        } else {
            render(editor, id);
        }
    });
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
            query(editor, id, &uv, &root, &["python", "list"], Query::Pythons);
        }
        Err(error) => {
            if facts.lock == Lock::Checking {
                facts.lock = Lock::Failed(error.clone());
            }
            if facts.newer == Probe::Checking {
                facts.newer = Probe::Failed(error.clone());
            }
            facts.pythons = Probe::Failed(error.clone());
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
                Query::Pythons => {
                    dashboard.facts.pythons = facts::installations(outcome, &output);
                }
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
    dashboard.rendered = page::render(&dashboard.facts, dashboard.last.as_ref(), home.as_deref());
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
    use crate::testing::{lines, press, type_text};

    fn ready() -> super::testing::Project {
        let mut project = full();
        let jobs = opened(&mut project);
        for job in &jobs {
            finish(&mut project.editor, job, 0, &["[]"]);
        }
        project
    }

    fn on_line(editor: &mut oxmacs_core::editor::Editor, text: &str) {
        let at = lines(editor)
            .iter()
            .position(|line| line == text)
            .unwrap_or_else(|| panic!("{text} missing"));
        editor.go_to_line(at + 1, 1);
    }

    fn arguments(jobs: &[oxmacs_core::editor::Job]) -> Vec<Vec<String>> {
        jobs.iter().map(|job| job.args.clone()).collect()
    }

    #[test]
    fn a_adds_a_dependency_and_shows_what_uv_says() {
        let mut project = ready();

        press(&mut project.editor, "a");
        assert_eq!(
            project
                .editor
                .minibuffer()
                .map(|prompt| prompt.prompt().to_owned()),
            Some("Add dependency: ".to_owned())
        );
        type_text(&mut project.editor, "httpx>=0.27");
        press(&mut project.editor, "RET");

        let jobs = spawned(&mut project.editor);
        assert_eq!(arguments(&jobs), [vec!["add", "httpx>=0.27"]]);
        assert_eq!(jobs[0].program, project.uv.display().to_string());
        assert_eq!(jobs[0].directory, project.root);
        assert_eq!(
            lines(&project.editor).last().map(String::as_str),
            Some("uv add httpx>=0.27  running…")
        );
        project
            .editor
            .job_output(jobs[0].id, "Resolved 5 packages in 12ms");
        assert_eq!(
            lines(&project.editor).last().map(String::as_str),
            Some("  Resolved 5 packages in 12ms")
        );
        project
            .editor
            .job_finished(jobs[0].id, &oxmacs_core::editor::Outcome::Exited(Some(0)));
        let shown = lines(&project.editor);
        let at = shown
            .iter()
            .position(|line| line == "uv add httpx>=0.27  done")
            .unwrap();
        assert_eq!(
            project.editor.pane().line_pills(at),
            [(20..24, Highlight::Success)]
        );
        assert_eq!(spawned(&mut project.editor).len(), 3);
    }

    #[test]
    fn d_removes_and_u_upgrades_the_package_at_point() {
        let mut project = ready();

        on_line(&mut project.editor, "  requests>=2.31");
        press(&mut project.editor, "d");
        let removed = spawned(&mut project.editor);
        finish(&mut project.editor, &removed[0], 0, &[]);
        spawned(&mut project.editor);
        on_line(&mut project.editor, "  numpy     2.1.0");
        press(&mut project.editor, "u");

        assert_eq!(arguments(&removed), [vec!["remove", "requests"]]);
        assert_eq!(
            arguments(&spawned(&mut project.editor)),
            [vec!["lock", "--upgrade-package", "numpy"]]
        );
    }

    #[test]
    fn keys_off_a_package_say_so() {
        let mut project = ready();

        press(&mut project.editor, "M-< d");

        assert_eq!(project.editor.message(), Some("No package on this line"));
        assert_eq!(spawned(&mut project.editor), []);
    }

    #[test]
    fn s_syncs_and_l_locks_one_at_a_time() {
        let mut project = ready();

        press(&mut project.editor, "s");
        let synced = spawned(&mut project.editor);
        press(&mut project.editor, "l");

        assert_eq!(arguments(&synced), [vec!["sync"]]);
        assert_eq!(project.editor.message(), Some("uv sync is still running"));
        assert_eq!(spawned(&mut project.editor), []);
        finish(&mut project.editor, &synced[0], 2, &["error: boom"]);
        spawned(&mut project.editor);
        let shown = lines(&project.editor);
        assert!(shown.contains(&"uv sync  exit 2".to_owned()), "{shown:?}");
        assert!(shown.contains(&"  error: boom".to_owned()), "{shown:?}");
        press(&mut project.editor, "l");
        assert_eq!(arguments(&spawned(&mut project.editor)), [vec!["lock"]]);
    }

    fn with_pythons() -> super::testing::Project {
        let mut project = full();
        let jobs = opened(&mut project);
        for job in &jobs {
            let listed: &[&str] = if job.args[0] == "python" {
                &[
                    "cpython-3.13.0-windows-x86_64-none    <download available>",
                    "pypy-3.10.14-windows-x86_64-none      C:\\pypy\\python.exe",
                ]
            } else {
                &["[]"]
            };
            finish(&mut project.editor, job, 0, listed);
        }
        project
    }

    #[test]
    fn python_versions_are_listed_and_i_installs_the_one_at_point() {
        let mut project = with_pythons();
        let shown = lines(&project.editor);
        assert!(
            shown.contains(&"Python versions (2)".to_owned()),
            "{shown:?}"
        );

        on_line(
            &mut project.editor,
            "  cpython-3.13.0-windows-x86_64-none  available",
        );
        press(&mut project.editor, "i");

        assert_eq!(
            arguments(&spawned(&mut project.editor)),
            [vec![
                "python",
                "install",
                "cpython-3.13.0-windows-x86_64-none"
            ]]
        );
    }

    #[test]
    fn p_pins_the_python_at_point() {
        let mut project = with_pythons();

        on_line(
            &mut project.editor,
            "  pypy-3.10.14-windows-x86_64-none    installed  C:\\pypy\\python.exe",
        );
        press(&mut project.editor, "p");

        assert_eq!(
            arguments(&spawned(&mut project.editor)),
            [vec!["python", "pin", "pypy@3.10.14"]]
        );
    }

    #[test]
    fn i_and_p_off_a_python_line_say_so() {
        let mut project = with_pythons();

        for key in ["M-< i", "p"] {
            press(&mut project.editor, key);

            assert_eq!(
                project.editor.message(),
                Some("No Python version on this line"),
                "{key}"
            );
        }
        assert_eq!(spawned(&mut project.editor), []);
    }

    #[test]
    fn actions_need_uv() {
        let mut project = ready();
        project.editor.apply_config(
            &oxmacs_core::config::Config::parse("[python]\nuv = \"oxmacs-no-such-uv\"\n").unwrap(),
        );

        press(&mut project.editor, "s");

        assert_eq!(
            project.editor.message(),
            Some("[python] uv not found: oxmacs-no-such-uv")
        );
        assert_eq!(spawned(&mut project.editor), []);
    }

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
                vec!["pip", "list", "--outdated", "--format", "json"],
                vec!["python", "list"]
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
        assert_eq!(spawned(&mut project.editor).len(), 3);
    }

    #[test]
    fn without_a_lock_or_a_venv_only_python_versions_are_checked() {
        let mut project = project(&[("pyproject.toml", PYPROJECT)]);

        let jobs = opened(&mut project);

        let shown = lines(&project.editor);
        assert_eq!(line_with(&shown, "Lock:"), Some("Lock:     no uv.lock"));
        assert_eq!(line_with(&shown, "Python:"), Some("Python:   no .venv"));
        assert_eq!(arguments(&jobs), [vec!["python", "list"]]);
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
