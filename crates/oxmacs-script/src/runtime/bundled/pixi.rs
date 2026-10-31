use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;

use crate::Ox;
use crate::testing::{Runner, press, settle, typed};

fn started_in(directory: &Path) -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    fs::write(home.path().join("init.ox"), "(use-plugin pixi)").unwrap();
    let mut editor = Editor::new();
    editor.set_home_directory(None);
    editor.set_working_directory(directory.to_path_buf());
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    (home, editor, ox)
}

fn evaluated(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
    match ox.eval(editor, source) {
        Ok(shown) => shown,
        Err(error) => format!("error: {}", error.message),
    }
}

fn run(editor: &mut Editor, ox: &mut Ox, command: &str) {
    let id = editor.command_named(command).unwrap();
    editor.execute(id);
    settle(editor, ox);
}

fn pump(editor: &mut Editor, ox: &mut Ox, done: impl Fn(&Editor) -> bool) -> bool {
    let mut runner = Runner::new();
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        runner.step(editor);
        ox.tick(editor);
        if done(editor) {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    false
}

fn shown_in(editor: &Editor, name: &str) -> String {
    editor
        .buffer_named(name)
        .and_then(|id| editor.buffer_by_id(id))
        .map(|buffer| buffer.contents())
        .unwrap_or_default()
}

#[test]
fn projects_are_found_by_their_pixi_toml_or_a_pyproject_with_tool_pixi() {
    let dir = tempfile::tempdir().unwrap();
    let toml = dir.path().join("a");
    fs::create_dir_all(toml.join("src")).unwrap();
    fs::write(toml.join("pixi.toml"), "[workspace]\n").unwrap();
    let pyproject = dir.path().join("b");
    fs::create_dir(&pyproject).unwrap();
    fs::write(
        pyproject.join("pyproject.toml"),
        "[project]\nname = \"b\"\n\n[tool.pixi.workspace]\nchannels = [\"conda-forge\"]\n",
    )
    .unwrap();
    let plain = dir.path().join("c");
    fs::create_dir(&plain).unwrap();
    fs::write(plain.join("pyproject.toml"), "[project]\nname = \"c\"\n").unwrap();
    let (_home, mut editor, mut ox) = started_in(dir.path());
    settle(&mut editor, &mut ox);

    let mut root = |folder: &Path| {
        evaluated(
            &mut editor,
            &mut ox,
            &format!(
                "(module (import (pixi project :as project))) (project/root-of {:?})",
                folder.display().to_string()
            ),
        )
    };
    assert_eq!(
        root(&toml.join("src")),
        format!("{:?}", toml.display().to_string())
    );
    assert_eq!(
        root(&pyproject),
        format!("{:?}", pyproject.display().to_string())
    );
    assert_eq!(root(&plain), "nil");
}

#[test]
fn outside_a_project_the_commands_say_how_to_make_one() {
    let dir = tempfile::tempdir().unwrap();
    let (_home, mut editor, mut ox) = started_in(dir.path());
    settle(&mut editor, &mut ox);

    for command in ["pixi", "pixi-run", "pixi-environment", "pixi-add"] {
        editor.show_message("");
        run(&mut editor, &mut ox, command);
        assert_eq!(
            editor.message(),
            Some("This folder isn't in a pixi project; pixi init makes one"),
            "{command}"
        );
    }
}

fn pixi_program() -> Option<PathBuf> {
    oxmacs_process::which("pixi").or_else(|| {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
        let name = if cfg!(windows) { "pixi.exe" } else { "pixi" };
        let installed = PathBuf::from(home).join(".pixi").join("bin").join(name);
        installed.is_file().then_some(installed)
    })
}

#[test]
#[ignore = "runs pixi, which fetches conda-forge's index"]
fn programs_in_a_pixi_project_get_its_environment_and_its_tasks_run_there() {
    let Some(pixi) = pixi_program() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("demo");
    let made = Command::new(&pixi)
        .args(["init", "--format", "pixi"])
        .arg(&project)
        .env("PIXI_NO_TELEMETRY", "1")
        .output()
        .unwrap();
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    let manifest = project.join("pixi.toml");
    let added = Command::new(&pixi)
        .args([
            "task",
            "add",
            "hello",
            "echo hello from pixi",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    fs::write(project.join("notes.txt"), "x").unwrap();
    let (_home, mut editor, mut ox) = started_in(dir.path());
    settle(&mut editor, &mut ox);

    editor.open_file(&project.join("notes.txt"));
    let active = pump(&mut editor, &mut ox, |_| {
        oxmacs_process::environment_for(&project)
            .iter()
            .any(|(key, _)| key == "CONDA_PREFIX")
    });
    assert!(active, "{:?}", editor.message());
    let prefix = oxmacs_process::environment_for(&project.join("src"))
        .into_iter()
        .find(|(key, _)| key == "PIXI_PROJECT_NAME")
        .map(|(_, value)| value);
    assert_eq!(prefix.as_deref(), Some("demo"));
    let labelled = pump(&mut editor, &mut ox, |editor| {
        editor
            .mode_line_segments()
            .iter()
            .any(|segment| segment.contains("pixi:default"))
    });
    assert!(labelled, "{:?}", editor.mode_line_segments());

    let id = editor.command_named("pixi-run").unwrap();
    editor.execute(id);
    assert!(pump(&mut editor, &mut ox, |editor| editor
        .minibuffer()
        .is_some()));
    typed(&mut editor, &mut ox, "hello");
    press(&mut editor, &mut ox, "RET");
    let ran = pump(&mut editor, &mut ox, |editor| {
        shown_in(editor, "*pixi run*").contains("hello from pixi")
    });
    assert!(ran, "{}", shown_in(&editor, "*pixi run*"));

    let id = editor.command_named("pixi").unwrap();
    editor.execute(id);
    let listed = pump(&mut editor, &mut ox, |editor| {
        let page = shown_in(editor, "*pixi*");
        page.contains("hello") && page.contains("default")
    });
    assert!(listed, "{}", shown_in(&editor, "*pixi*"));
}
