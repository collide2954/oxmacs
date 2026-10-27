use std::path::PathBuf;

use oxmacs_core::editor::{Editor, JobId, JobSpec, Outcome, ProcessStatus};

use crate::Ox;
use crate::testing::{press, question, settle, typed};

const MEBIBYTE: u64 = 1024 * 1024;

fn started(columns: usize) -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    let mut editor = Editor::new();
    editor.resize_pane(editor.selected_window(), 30, columns);
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn job(
    editor: &mut Editor,
    program: &str,
    args: &[&str],
    label: &'static str,
    name: &str,
) -> JobId {
    editor.start_job(
        JobSpec {
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            directory: PathBuf::from("work"),
            env: vec![("RUST_LOG".to_owned(), "debug".to_owned())],
            label,
            name: name.to_owned(),
            buffer: None,
            interactive: false,
            input: None,
        },
        |_, _, _| {},
    )
}

fn listed(editor: &mut Editor, ox: &mut Ox) {
    let id = editor.command_named("list-processes").unwrap();
    editor.execute(id);
    settle(editor, ox);
}

fn lines(editor: &Editor) -> Vec<String> {
    editor
        .buffer()
        .contents()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn line_with(editor: &Editor, text: &str) -> String {
    let shown = lines(editor);
    shown
        .iter()
        .find(|line| line.contains(text))
        .cloned()
        .unwrap_or_else(|| panic!("no line with {text}: {shown:#?}"))
}

fn two_jobs(editor: &mut Editor) -> (JobId, JobId) {
    let r = job(
        editor,
        "/usr/lib/R/bin/Rterm",
        &["--ess"],
        "R session",
        "*R:analysis*",
    );
    let lint = job(editor, "cargo", &["check"], "lint", "cargo check");
    editor.job_started_as(r, 4242);
    editor.job_started_as(lint, 4343);
    editor.job_finished(lint, &Outcome::Exited(Some(101)));
    (r, lint)
}

#[test]
fn the_list_shows_running_and_finished_processes_with_their_usage() {
    let (_home, mut editor, mut ox) = started(400);
    two_jobs(&mut editor);

    listed(&mut editor, &mut ox);
    editor.processes_tick(&[(4242, 1.5, 85 * MEBIBYTE)]);
    settle(&mut editor, &mut ox);

    assert_eq!(editor.buffer().name(), "*Processes*");
    assert!(editor.wants_process_usage());
    let titles = line_with(&editor, "Name");
    for title in [
        "Kind", "Status", "PID", "Time", "CPU", "Memory", "History", "Details",
    ] {
        assert!(titles.contains(title), "{title} in {titles}");
    }
    let r = line_with(&editor, "*R:analysis*");
    for part in [
        "R session",
        "running",
        "4242",
        "1.5%",
        "85M",
        "█",
        "Rterm --ess",
    ] {
        assert!(r.contains(part), "{part} in {r}");
    }
    let lint = line_with(&editor, "cargo check");
    for part in ["lint", "exit 101", "4343"] {
        assert!(lint.contains(part), "{part} in {lint}");
    }
}

#[test]
fn narrow_windows_keep_the_most_useful_columns() {
    let (_home, mut editor, mut ox) = started(400);
    two_jobs(&mut editor);
    listed(&mut editor, &mut ox);

    editor.resize_pane(editor.selected_window(), 20, 41);
    editor.prepare_display();
    settle(&mut editor, &mut ox);

    let width = editor.buffer().id();
    let width = editor.buffer_width(width).unwrap();
    let shown = lines(&editor);
    assert!(
        shown.iter().all(|line| line.chars().count() <= width),
        "{width} {shown:#?}"
    );
    let titles = line_with(&editor, "Name");
    assert!(
        titles.contains("Status") && titles.contains("CPU"),
        "{titles}"
    );
    assert!(
        !titles.contains("Details") && !titles.contains("Kind"),
        "{titles}"
    );
}

#[test]
fn k_asks_then_stops_and_capital_k_kills() {
    let (_home, mut editor, mut ox) = started(400);
    let build = job(&mut editor, "cargo", &["build"], "compile", "cargo build");
    editor.job_started_as(build, 99);
    listed(&mut editor, &mut ox);
    editor.take_effects();
    let row = lines(&editor)
        .iter()
        .position(|line| line.contains("cargo build"))
        .unwrap();
    editor.go_to_line(row + 1, 1);

    press(&mut editor, &mut ox, "k");
    assert_eq!(
        question(&editor),
        Some("Stop cargo build? (y or n) ".to_owned())
    );
    press(&mut editor, &mut ox, "y");
    assert_eq!(editor.job_status(build), Some(ProcessStatus::Stopping));
    assert!(line_with(&editor, "cargo build").contains("stopping"));
    press(&mut editor, &mut ox, "K");
    assert_eq!(
        question(&editor),
        Some("Kill cargo build? (y or n) ".to_owned())
    );
}

#[test]
fn tab_shows_the_command_folder_status_and_environment() {
    let (_home, mut editor, mut ox) = started(400);
    two_jobs(&mut editor);
    listed(&mut editor, &mut ox);
    let row = lines(&editor)
        .iter()
        .position(|line| line.contains("cargo check"))
        .unwrap();
    editor.go_to_line(row + 1, 1);

    press(&mut editor, &mut ox, "TAB");

    assert!(line_with(&editor, "Command").ends_with("cargo check"));
    assert!(line_with(&editor, "Folder").ends_with("work"));
    assert!(
        line_with(&editor, "exit 101 after")
            .trim_start()
            .starts_with("Status")
    );
    assert!(line_with(&editor, "Environment").ends_with("RUST_LOG=debug"));
    press(&mut editor, &mut ox, "TAB");
    assert!(
        !lines(&editor)
            .iter()
            .any(|line| line.contains("Environment"))
    );
}

#[test]
fn sorting_and_filtering_rearrange_the_rows() {
    let (_home, mut editor, mut ox) = started(400);
    two_jobs(&mut editor);
    listed(&mut editor, &mut ox);

    press(&mut editor, &mut ox, "s");
    assert_eq!(editor.message(), Some("Sorted by name"));
    let order = |editor: &Editor, first: &str, second: &str| {
        let shown = lines(editor);
        let at = |text: &str| shown.iter().position(|line| line.contains(text));
        at(first) < at(second)
    };
    assert!(order(&editor, "*R:analysis*", "cargo check"));
    press(&mut editor, &mut ox, "S");
    assert!(order(&editor, "cargo check", "*R:analysis*"));
    assert!(line_with(&editor, "Name").contains('▼'));

    press(&mut editor, &mut ox, "/");
    typed(&mut editor, &mut ox, "check");
    press(&mut editor, &mut ox, "RET");

    assert!(line_with(&editor, "Filter").contains("check"));
    assert!(
        !lines(&editor)
            .iter()
            .any(|line| line.contains("*R:analysis*"))
    );
}

#[test]
fn nothing_running_says_so_and_q_stops_watching() {
    let (_home, mut editor, mut ox) = started(400);

    listed(&mut editor, &mut ox);
    assert!(lines(&editor).iter().any(|line| line == "Nothing running"));
    assert!(editor.wants_process_usage());

    press(&mut editor, &mut ox, "q");

    assert!(!editor.wants_process_usage());
    assert_ne!(editor.buffer().name(), "*Processes*");
}
