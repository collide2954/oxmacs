use std::path::PathBuf;

use crate::editor::{Editor, Effect, JobId};
use crate::input::KeySequence;
use crate::syntax::Language;
use crate::text::Buffer;

pub(super) const R_PROMPT: &str = "\u{1}>\u{2}\u{3}";
pub(super) const R_CONSOLE: &str = "*R:analysis*";
pub(super) const R_PLOTS: &str = "*R:analysis plots*";

pub(super) fn r_folder() -> PathBuf {
    std::env::temp_dir().join("analysis")
}

pub(super) fn press(editor: &mut Editor, notation: &str) {
    let sequence: KeySequence = notation.parse().unwrap();
    for key in sequence.keys() {
        editor.handle_key(*key);
    }
}

pub(super) fn type_text(editor: &mut Editor, text: &str) {
    for c in text.chars() {
        let notation = if c == ' ' {
            "SPC".to_owned()
        } else {
            c.to_string()
        };
        press(editor, &notation);
    }
}

pub(super) fn labels(editor: &Editor) -> Vec<String> {
    editor
        .completions()
        .into_iter()
        .map(|completion| completion.label)
        .collect()
}

pub(super) fn question(editor: &Editor) -> Option<String> {
    editor
        .question()
        .map(|question| question.prompt().to_owned())
}

pub(super) fn minibuffer_input(editor: &Editor) -> String {
    editor.minibuffer().unwrap().input().contents()
}

pub(super) fn lines(editor: &Editor) -> Vec<String> {
    editor
        .buffer()
        .contents()
        .lines()
        .map(str::to_owned)
        .collect()
}

pub(super) fn at_point(editor: &Editor) -> String {
    let buffer = editor.buffer();
    let line = buffer.line_of(buffer.point());
    buffer.text_in(buffer.point()..buffer.line_end(line))
}

pub(super) fn editor_with(text: &str, point: usize) -> Editor {
    let mut buffer = Buffer::with_text("t", text);
    buffer.set_point(point);
    Editor::with_buffer(buffer)
}

pub(super) fn point_after(text: &str, point: usize, notation: &str) -> usize {
    let mut editor = editor_with(text, point);
    press(&mut editor, notation);
    editor.buffer().point()
}

pub(super) fn contents_after(text: &str, point: usize, notation: &str) -> (String, usize) {
    let mut editor = editor_with(text, point);
    press(&mut editor, notation);
    (editor.buffer().contents(), editor.buffer().point())
}

pub(super) const R_FOUND: fn(Option<&str>) -> Option<PathBuf> = |_| Some(PathBuf::from("R-bin"));

pub(super) fn r_script(text: &str, point: usize) -> Editor {
    let mut editor = editor_with(text, point);
    editor.set_major_mode(Some(Language::R));
    editor.find_r = R_FOUND;
    editor.set_working_directory(r_folder());
    editor
}

pub(super) fn r_sessions(editor: &mut Editor) -> Vec<JobId> {
    editor
        .take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::StartSession(job) => Some(job.id),
            _ => None,
        })
        .collect()
}

pub(super) fn r_sent(editor: &mut Editor) -> Vec<String> {
    editor
        .take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Send(_, text) => Some(text),
            _ => None,
        })
        .collect()
}

pub(super) fn r_running(editor: &mut Editor) -> JobId {
    press(editor, "C-c C-z");
    let job = r_sessions(editor)[0];
    editor.session_output(job, R_PROMPT);
    press(editor, "C-c C-z");
    editor.take_effects();
    job
}
