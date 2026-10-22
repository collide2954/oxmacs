use oxmacs_core::editor::Editor;

use crate::Ox;
use crate::testing::{press, settle};

const INIT: &str = "(def (spin n) (if (> n 0) (spin (- n 1)) :done))
(def (work) (spin 30000))
";

fn started() -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("init.ox"), INIT).unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn run(editor: &mut Editor, ox: &mut Ox, command: &str) {
    let id = editor.command_named(command).unwrap();
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

fn profiled() -> (tempfile::TempDir, Editor, Ox) {
    let (home, mut editor, mut ox) = started();
    run(&mut editor, &mut ox, "ox-profiler-start");
    assert_eq!(
        editor.message(),
        Some("Profiling Ox; M-x ox-profiler-stop shows where the time went")
    );
    ox.eval(&mut editor, "(work)").unwrap();
    ox.tick(&mut editor);
    run(&mut editor, &mut ox, "ox-profiler-stop");
    (home, editor, ox)
}

fn line_with(editor: &Editor, text: &str) -> usize {
    let shown = lines(editor);
    shown
        .iter()
        .position(|line| line.contains(text))
        .unwrap_or_else(|| panic!("no line with {text}: {shown:#?}"))
}

#[test]
fn profiling_from_the_editor_reports_by_plugin_with_editor_ticks() {
    let (_home, mut editor, mut ox) = profiled();

    assert_eq!(editor.buffer().name(), "*ox profile*");
    let shown = lines(&editor);
    assert!(shown[0].starts_with("Ox profile  "), "{shown:#?}");
    assert!(
        shown.iter().any(|line| line.starts_with("Slowest ticks")),
        "{shown:#?}"
    );
    assert!(
        shown.iter().any(|line| line.contains(" tick at ")),
        "{shown:#?}"
    );
    assert!(
        shown.iter().any(|line| line.contains("▾ user")),
        "{shown:#?}"
    );
    assert!(
        shown
            .iter()
            .any(|line| line.contains("spin") && line.ends_with("init.ox:1")),
        "{shown:#?}"
    );
    run(&mut editor, &mut ox, "ox-profiler-stop");
    assert_eq!(editor.message(), Some("The profiler isn't running"));
}

#[test]
fn the_report_folds_switches_to_a_flat_table_and_visits_functions() {
    let (_home, mut editor, mut ox) = profiled();
    let user = line_with(&editor, "▾ user");

    ox.eval(&mut editor, &format!("(goto-line {user})"))
        .unwrap();
    press(&mut editor, &mut ox, "TAB");
    assert!(lines(&editor).iter().any(|line| line.contains("▸ user")));
    assert!(!lines(&editor).iter().any(|line| line.contains("spin")));
    press(&mut editor, &mut ox, "TAB");
    assert!(lines(&editor).iter().any(|line| line.contains("spin")));

    press(&mut editor, &mut ox, "f");
    assert!(
        lines(&editor)
            .iter()
            .any(|line| line.starts_with("Functions by self time"))
    );
    let spin = line_with(&editor, "spin");
    ox.eval(&mut editor, &format!("(goto-line {spin})"))
        .unwrap();
    press(&mut editor, &mut ox, "RET");

    assert_eq!(editor.buffer().name(), "init.ox");
    let buffer = editor.buffer();
    assert_eq!(buffer.line_of(buffer.point()), 0);
}

#[test]
fn reports_can_be_taken_while_profiling_continues() {
    let (_home, mut editor, mut ox) = started();
    run(&mut editor, &mut ox, "ox-profiler-report");
    assert_eq!(
        editor.message(),
        Some("Start the profiler with M-x ox-profiler-start")
    );
    run(&mut editor, &mut ox, "ox-profiler-start");
    ox.eval(&mut editor, "(work)").unwrap();

    run(&mut editor, &mut ox, "ox-profiler-report");
    assert_eq!(editor.buffer().name(), "*ox profile*");
    assert!(lines(&editor).iter().any(|line| line.contains("spin")));
    press(&mut editor, &mut ox, "g");
    assert!(lines(&editor).iter().any(|line| line.contains("spin")));
    assert_eq!(
        ox.eval(&mut editor, "(some? (profiler-snapshot))").unwrap(),
        "true"
    );

    run(&mut editor, &mut ox, "ox-profiler-stop");
    press(&mut editor, &mut ox, "g");
    assert_eq!(
        editor.message(),
        Some("The profiler isn't running; this report is final")
    );
}
