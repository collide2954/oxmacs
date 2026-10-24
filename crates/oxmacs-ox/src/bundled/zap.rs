use std::ops::Range;
use std::time::{Duration, Instant};

use oxmacs_core::editor::Editor;
use oxmacs_core::syntax::Language;
use oxmacs_core::text::Buffer;
use oxmacs_core::theme::Highlight;

use crate::Ox;
use crate::testing::{home_with, press, settle, typed};

const LONG: &str = "(use-plugin zap :config {duration: 60000})";

fn started(init: &str, files: &[(&str, &str)], editor: Editor) -> (tempfile::TempDir, Editor, Ox) {
    let mut all = vec![("init.ox", init)];
    all.extend_from_slice(files);
    let home = home_with(&all);
    let mut editor = editor;
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn visiting(init: &str, name: &str, text: &str) -> (tempfile::TempDir, Editor, Ox) {
    let (home, mut editor, mut ox) = started(init, &[(name, text)], Editor::new());
    editor.open_file(&home.path().join(name));
    let end = editor.buffer().len_chars();
    editor.buffer_mut().set_point(end);
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn lit(editor: &Editor, line: usize, at: Instant) -> Vec<(Range<usize>, Highlight)> {
    editor
        .pane()
        .line_flashes(line, at)
        .into_iter()
        .map(|(range, face, _)| (range, face))
        .collect()
}

#[test]
fn zap_is_off_until_use_plugin_turns_it_on() {
    let (_home, mut editor, mut ox) = visiting("", "scratch.ox", "(+ 1 2)");
    let before = Instant::now();
    press(&mut editor, &mut ox, "C-x C-e");
    assert_eq!(lit(&editor, 0, before), []);

    let (_home, mut editor, mut ox) = visiting("(use-plugin zap)", "scratch.ox", "(+ 1 2)");
    let before = Instant::now();
    press(&mut editor, &mut ox, "C-x C-e");
    assert_eq!(lit(&editor, 0, before), [(0..7, Highlight::Region)]);
}

#[test]
fn failed_evaluations_flash_in_the_error_color() {
    let (_home, mut editor, mut ox) = visiting(LONG, "scratch.ox", "(def a 1)\n(/ a 0)\n(def b 2)");
    let before = Instant::now();

    press(&mut editor, &mut ox, "C-c C-k");

    assert_eq!(lit(&editor, 0, before), [(0..9, Highlight::Region)]);
    assert_eq!(lit(&editor, 1, before), [(0..7, Highlight::Removed)]);
    assert_eq!(lit(&editor, 2, before), []);
}

#[test]
fn flashes_fade_out_over_the_duration_setting() {
    let (_home, mut editor, mut ox) = visiting("(use-plugin zap)", "scratch.ox", "(+ 1 2)");
    let before = Instant::now();
    press(&mut editor, &mut ox, "C-x C-e");
    let after = Instant::now();
    assert!(editor.pane().flashing(before + Duration::from_millis(499)));
    assert!(!editor.pane().flashing(after + Duration::from_millis(500)));

    let (_home, mut editor, mut ox) = visiting(
        "(use-plugin zap :config {duration: 2000})",
        "scratch.ox",
        "(+ 1 2)",
    );
    let before = Instant::now();
    press(&mut editor, &mut ox, "C-x C-e");
    let after = Instant::now();
    assert!(editor.pane().flashing(before + Duration::from_millis(1999)));
    assert!(!editor.pane().flashing(after + Duration::from_millis(2000)));
}

#[test]
fn the_repl_flashes_its_input() {
    let (_home, mut editor, mut ox) = started(LONG, &[], Editor::new());
    press(&mut editor, &mut ox, "M-x");
    typed(&mut editor, &mut ox, "ox-repl");
    press(&mut editor, &mut ox, "RET");
    let before = Instant::now();

    typed(&mut editor, &mut ox, "(/ 1 0)");
    press(&mut editor, &mut ox, "RET");

    assert_eq!(lit(&editor, 1, before), [(4..11, Highlight::Removed)]);
}

#[test]
fn other_plugins_zap_code_of_their_own() {
    let (_home, mut editor, mut ox) = started(LONG, &[], Editor::new());
    editor.buffer_mut().insert_at(0, "abc def");
    let before = Instant::now();

    ox.eval(&mut editor, "(zap 0 3) (zap 4 7 :error true)")
        .unwrap();
    settle(&mut editor, &mut ox);

    assert_eq!(
        lit(&editor, 0, before),
        [(0..3, Highlight::Region), (4..7, Highlight::Removed)]
    );
}

#[test]
fn regions_sent_to_any_console_flash() {
    let (_home, mut editor, mut ox) = started(LONG, &[], Editor::new());
    ox.eval(
        &mut editor,
        r#"(console-start "*shell*" :program "shell")
(defcommand send-region "Send the region to the shell"
  (let [[start end] (region)]
    (deactivate-mark)
    (console-send "*shell*" (text-between start end))))"#,
    )
    .unwrap();
    settle(&mut editor, &mut ox);
    editor.buffer_mut().insert_at(0, "echo one\necho two\n");
    editor.buffer_mut().set_point(9);
    press(&mut editor, &mut ox, "C-SPC C-e");
    let before = Instant::now();

    press(&mut editor, &mut ox, "M-x");
    typed(&mut editor, &mut ox, "send-region");
    press(&mut editor, &mut ox, "RET");

    assert_eq!(lit(&editor, 0, before), []);
    assert_eq!(lit(&editor, 1, before), [(0..8, Highlight::Region)]);
}

fn r_script(name: &str, text: &str) -> (tempfile::TempDir, Editor, Ox) {
    let home = home_with(&[("R-bin", "")]);
    let program = home.path().join("R-bin").display().to_string();
    let init =
        format!("(use-plugin r :config {{program: {program:?} language-server: false}})\n{LONG}");
    std::fs::write(home.path().join("init.ox"), init).unwrap();
    std::fs::write(home.path().join(name), text).unwrap();
    let mut editor = Editor::with_buffer(Buffer::new("*scratch*"));
    editor.set_working_directory(super::r::folder());
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    editor.open_file(&home.path().join(name));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

#[test]
fn r_statements_flash_as_they_are_sent() {
    let (_home, mut editor, mut ox) = r_script("cars.R", "x <- 1\ny <- 2\n");
    assert_eq!(editor.language(), Some(Language::R));
    let before = Instant::now();

    press(&mut editor, &mut ox, "C-RET");

    assert_eq!(lit(&editor, 0, before), [(0..6, Highlight::Region)]);
    assert_eq!(lit(&editor, 1, before), []);
}

#[test]
fn r_markdown_chunks_flash_their_code() {
    let (_home, mut editor, mut ox) = r_script(
        "report.Rmd",
        "```{r}\nx <- 1\n```\n\nText.\n\n```{r}\n#| echo: false\ny <- 2\n```\n",
    );
    editor.go_to_line(2, 1);
    let before = Instant::now();

    press(&mut editor, &mut ox, "C-c C-a");

    assert_eq!(lit(&editor, 1, before), [(0..6, Highlight::Region)]);
    assert_eq!(lit(&editor, 4, before), []);
    assert_eq!(lit(&editor, 7, before), []);
    assert_eq!(lit(&editor, 8, before), [(0..6, Highlight::Region)]);
}
