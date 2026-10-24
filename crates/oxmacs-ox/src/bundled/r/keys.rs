use oxmacs_core::editor::{Editor, Effect};
use oxmacs_core::syntax::Language;
use oxmacs_core::text::Buffer;

use super::{CONSOLE, script, with_program};

#[test]
fn meta_minus_inserts_an_assignment_arrow() {
    let mut r = script("x", 1);

    r.press("M--");

    assert_eq!(r.contents(), "x <- ");
    assert_eq!(r.editor.buffer().point(), 5);
}

#[test]
fn existing_spaces_are_reused() {
    let mut r = script("x  1", 2);

    r.press("M--");

    assert_eq!(r.contents(), "x <- 1");
    assert_eq!(r.editor.buffer().point(), 5);
}

#[test]
fn control_shift_m_inserts_the_native_pipe() {
    let mut r = script("df", 2);

    r.press("C-S-m");

    assert_eq!(r.contents(), "df |> ");
}

#[test]
fn the_shortcuts_belong_to_r_buffers() {
    let mut buffer = Buffer::with_text("t", "x");
    buffer.set_point(1);
    let mut editor = Editor::with_buffer(buffer);
    editor.set_major_mode(Some(Language::RUST));
    let mut r = with_program(None, editor);

    r.press("M--");

    assert_eq!(r.contents(), "x");
}

#[test]
fn read_only_r_buffers_refuse_the_shortcuts() {
    let mut r = script("x", 1);
    r.press("C-x C-q");

    r.press("M--");

    assert_eq!(r.contents(), "x");
    assert_eq!(r.editor.message(), Some("Buffer is read-only: t"));
}

#[test]
fn control_return_starts_r_sends_the_statement_and_steps() {
    let mut r = script("x <- 1\n\ny <- df |>\n  head()\n", 2);

    r.press("C-RET");

    let effects = r.editor.take_effects();
    assert!(matches!(effects.first(), Some(Effect::StartSession(_))));
    assert!(matches!(&effects[1], Effect::Send(_, code) if code == "x <- 1\n"));
    assert_eq!(r.editor.buffer().name(), "t");
    assert_eq!(r.editor.buffer().point(), 8);
    assert_eq!(r.editor.tiling().windows().len(), 2);

    r.press("C-RET");

    assert_eq!(r.sent(), ["y <- df |>\n  head()\n"]);
    assert_eq!(r.editor.buffer().point(), r.editor.buffer().len_chars());
}

#[test]
fn r_is_started_only_once() {
    let mut r = script("a\nb\n", 0);

    r.press("C-RET");
    assert_eq!(r.started().len(), 1);
    r.press("C-RET");

    assert_eq!(r.started(), Vec::new());
}

#[test]
fn a_selection_is_sent_as_it_is_and_point_stays() {
    let mut r = script("a <- 1\nb <- 2\n", 0);
    r.running();
    r.press("C-SPC C-e");

    r.press("C-RET");

    assert_eq!(r.sent(), ["a <- 1\n"]);
    assert_eq!(r.editor.buffer().point(), 6);
}

#[test]
fn control_c_control_c_sends_the_function_or_the_paragraph() {
    let text = "f <- function(x) {\n  x + 1\n}\n\na <- 1\nb <- 2\n\nc <- 3\n";
    let mut r = script(text, 22);
    r.running();

    r.press("C-c C-c");
    assert_eq!(r.sent(), ["f <- function(x) {\n  x + 1\n}\n"]);
    assert_eq!(r.editor.buffer().point(), 30);

    r.press("C-n C-c C-c");
    assert_eq!(r.sent(), ["a <- 1\nb <- 2\n"]);
    assert_eq!(r.editor.buffer().point(), 45);
}

#[test]
fn nothing_to_send_says_so() {
    let mut r = script("x <- 1\n\n", 7);
    r.running();

    r.press("C-RET");

    assert_eq!(r.editor.message(), Some("No R code here"));
    assert_eq!(r.sent(), Vec::<String>::new());
}

#[test]
fn control_c_control_z_moves_between_script_and_console() {
    let mut r = script("x\n", 0);

    r.press("C-c C-z");
    assert_eq!(r.editor.buffer().name(), CONSOLE);
    assert_eq!(r.started().len(), 1);

    r.press("C-c C-z");
    assert_eq!(r.editor.buffer().name(), "t");

    r.press("C-c C-z");
    assert_eq!(r.editor.buffer().name(), CONSOLE);
    assert_eq!(r.started(), Vec::new());
}
