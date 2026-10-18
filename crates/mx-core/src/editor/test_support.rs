use crate::editor::Editor;
use crate::input::KeySequence;
use crate::text::Buffer;

pub(super) fn press(editor: &mut Editor, notation: &str) {
    let sequence: KeySequence = notation.parse().unwrap();
    for key in sequence.keys() {
        editor.handle_key(*key);
    }
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
