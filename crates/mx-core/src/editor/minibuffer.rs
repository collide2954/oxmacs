use crate::command::Command;
use crate::editor::Editor;
use crate::text::Buffer;

#[derive(Debug)]
pub struct Minibuffer {
    prompt: String,
    pub(super) input: Buffer,
    pub(super) note: Option<String>,
}

impl Minibuffer {
    fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            input: Buffer::new(" *Minibuf*"),
            note: None,
        }
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn input(&self) -> &Buffer {
        &self.input
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }
}

impl Editor {
    pub fn minibuffer(&self) -> Option<&Minibuffer> {
        self.minibuffer.as_ref()
    }

    pub(super) fn execute_extended_command(&mut self) {
        if self.minibuffer.is_some() {
            self.show_message("Command attempted to use minibuffer while in minibuffer");
            return;
        }
        self.minibuffer = Some(Minibuffer::new("M-x "));
    }

    pub(super) fn exit_minibuffer(&mut self) {
        let Some(minibuffer) = self.minibuffer.as_mut() else {
            return;
        };
        match minibuffer.input.contents().trim().parse::<Command>() {
            Ok(command) => {
                self.minibuffer = None;
                self.execute(command);
            }
            Err(_) => minibuffer.note = Some("[No match]".to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, press, type_text};

    fn input(editor: &Editor) -> String {
        editor.minibuffer().unwrap().input().contents()
    }

    #[test]
    fn m_x_opens_the_minibuffer_with_a_prompt() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");

        let minibuffer = editor.minibuffer().unwrap();
        assert_eq!(minibuffer.prompt(), "M-x ");
        assert_eq!(minibuffer.input().contents(), "");
    }

    #[test]
    fn typing_goes_to_the_minibuffer_not_the_buffer() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "end");

        assert_eq!(input(&editor), "end");
        assert_eq!(editor.buffer().contents(), "");
    }

    #[test]
    fn editing_keys_work_in_the_minibuffer() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "abc");
        press(&mut editor, "DEL C-a");

        assert_eq!(input(&editor), "ab");
        assert_eq!(editor.minibuffer().unwrap().input().point(), 0);
    }

    #[test]
    fn return_runs_the_named_command_in_the_buffer() {
        let mut editor = editor_with("hello", 0);

        press(&mut editor, "M-x");
        type_text(&mut editor, "end-of-buffer");
        press(&mut editor, "RET");

        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.buffer().point(), 5);
    }

    #[test]
    fn unknown_commands_keep_the_minibuffer_open() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "nope");
        press(&mut editor, "RET");

        let minibuffer = editor.minibuffer().unwrap();
        assert_eq!(minibuffer.input().contents(), "nope");
        assert_eq!(minibuffer.note(), Some("[No match]"));
    }

    #[test]
    fn the_next_key_clears_the_note() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "nope");
        press(&mut editor, "RET DEL");

        assert_eq!(editor.minibuffer().unwrap().note(), None);
    }

    #[test]
    fn keyboard_quit_closes_the_minibuffer() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x a C-g");

        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.message(), Some("Quit"));
    }

    #[test]
    fn m_x_inside_the_minibuffer_is_refused() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x a M-x");

        assert_eq!(input(&editor), "a");
        assert_eq!(
            editor.message(),
            Some("Command attempted to use minibuffer while in minibuffer")
        );
    }
}
