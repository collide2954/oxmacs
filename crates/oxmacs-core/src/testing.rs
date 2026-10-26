use crate::editor::Editor;
use crate::input::KeySequence;
use crate::text::Buffer;

pub fn press(editor: &mut Editor, notation: &str) {
    let sequence: KeySequence = notation.parse().unwrap();
    for key in sequence.keys() {
        editor.handle_key(*key);
    }
}

pub fn type_text(editor: &mut Editor, text: &str) {
    for c in text.chars() {
        let notation = if c == ' ' {
            "SPC".to_owned()
        } else {
            c.to_string()
        };
        press(editor, &notation);
    }
}

pub fn labels(editor: &Editor) -> Vec<String> {
    editor
        .completions()
        .into_iter()
        .map(|completion| completion.label)
        .collect()
}

pub fn question(editor: &Editor) -> Option<String> {
    editor
        .question()
        .map(|question| question.prompt().to_owned())
}

pub fn minibuffer_input(editor: &Editor) -> String {
    editor.minibuffer().unwrap().input().contents()
}

pub fn lines(editor: &Editor) -> Vec<String> {
    editor
        .buffer()
        .contents()
        .lines()
        .map(str::to_owned)
        .collect()
}

pub fn at_point(editor: &Editor) -> String {
    let buffer = editor.buffer();
    let line = buffer.line_of(buffer.point());
    buffer.text_in(buffer.point()..buffer.line_end(line))
}

pub fn editor_with(text: &str, point: usize) -> Editor {
    let mut buffer = Buffer::with_text("t", text);
    buffer.set_point(point);
    Editor::with_buffer(buffer)
}

pub fn point_after(text: &str, point: usize, notation: &str) -> usize {
    let mut editor = editor_with(text, point);
    press(&mut editor, notation);
    editor.buffer().point()
}

pub fn contents_after(text: &str, point: usize, notation: &str) -> (String, usize) {
    let mut editor = editor_with(text, point);
    press(&mut editor, notation);
    (editor.buffer().contents(), editor.buffer().point())
}
