use std::fmt::Write;

use crate::command::Command;
use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::text::Buffer;
use crate::theme::Highlight;

const NAME: &str = "*mx*";
const TAGLINE: &str = "An Emacs-inspired editor, written in Rust.";
const INDENT: usize = 3;
const TITLE_LINE: usize = 1;
const FIRST_BUTTON_LINE: usize = 4;
const BUTTONS: &[(Command, &str)] = &[
    (Command::FindFile, "Open a file"),
    (Command::Dired, "Browse a directory"),
    (Command::SwitchToBuffer, "Switch buffers"),
    (Command::ExecuteExtendedCommand, "Run any command"),
    (Command::SaveBuffer, "Save"),
    (Command::SaveBuffersKillMx, "Quit"),
];

#[derive(Debug)]
pub(super) struct Splash {
    buttons: Vec<Button>,
}

#[derive(Debug)]
struct Button {
    line: usize,
    command: Command,
    key_width: usize,
}

impl Splash {
    fn button(&self, line: usize) -> Option<&Button> {
        self.buttons.iter().find(|button| button.line == line)
    }

    pub(super) fn has_button(&self, line: usize) -> bool {
        self.button(line).is_some()
    }

    pub(super) fn line_style(&self, line: usize) -> LineStyle {
        let spans = if line == TITLE_LINE {
            vec![(INDENT..usize::MAX, Highlight::Heading)]
        } else if let Some(button) = self.button(line) {
            vec![(INDENT..INDENT + button.key_width, Highlight::Link)]
        } else {
            Vec::new()
        };
        LineStyle {
            background: None,
            spans,
        }
    }
}

impl Editor {
    pub fn about(&mut self) {
        if let Some(index) =
            (0..self.buffers.len()).find(|index| self.buffers.splash(*index).is_some())
        {
            self.select_buffer(index);
            return;
        }
        let keys: Vec<String> = BUTTONS
            .iter()
            .map(|(command, _)| {
                self.keymap
                    .where_is(*command)
                    .map(|keys| keys.to_string())
                    .unwrap_or_default()
            })
            .collect();
        let key_width = keys
            .iter()
            .map(|key| key.chars().count())
            .max()
            .unwrap_or(0);
        let name_width = BUTTONS
            .iter()
            .map(|(command, _)| command.name().len())
            .max()
            .unwrap_or(0);
        let pad = " ".repeat(INDENT);
        let mut text = format!(
            "\n{pad}mx {}\n{pad}{TAGLINE}\n\n",
            env!("CARGO_PKG_VERSION")
        );
        let mut buttons = Vec::new();
        for (index, ((command, description), key)) in BUTTONS.iter().zip(&keys).enumerate() {
            let name = command.name();
            let _ = writeln!(
                text,
                "{pad}{key:<key_width$}   {name:<name_width$}  {description}"
            );
            buttons.push(Button {
                line: FIRST_BUTTON_LINE + index,
                command: *command,
                key_width: key.chars().count(),
            });
        }
        let mut buffer = Buffer::with_text(NAME, &text);
        buffer.set_read_only(true);
        buffer.set_point(buffer.line_start(FIRST_BUTTON_LINE) + INDENT);
        self.add_buffer_with(buffer, Mode::Splash(Splash { buttons }));
    }

    pub(super) fn forward_button(&mut self, forward: bool) {
        let line = self.point_line();
        let Some(splash) = self.buffers.splash(0) else {
            return;
        };
        let lines: Vec<usize> = splash.buttons.iter().map(|button| button.line).collect();
        let target = if forward {
            lines
                .iter()
                .find(|button| **button > line)
                .or(lines.first())
        } else {
            lines
                .iter()
                .rev()
                .find(|button| **button < line)
                .or(lines.last())
        };
        if let Some(&target) = target {
            let buffer = self.buffers.current_mut();
            buffer.set_point(buffer.line_start(target) + INDENT);
        }
    }

    pub(super) fn push_button(&mut self) {
        let line = self.point_line();
        let command = self
            .buffers
            .splash(0)
            .and_then(|splash| splash.button(line))
            .map(|button| button.command);
        match command {
            Some(command) => self.execute(command),
            None => self.show_message("No button here"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::display::LineStyle;
    use crate::editor::test_support::{at_point, lines, press, type_text};
    use crate::editor::{Cell, Clicks, Editor};
    use crate::theme::Highlight;

    fn shown() -> Editor {
        let mut editor = Editor::new();
        editor.about();
        editor
    }

    fn line_of(editor: &Editor, key: &str) -> usize {
        lines(editor)
            .iter()
            .position(|line| line.trim_start().starts_with(key))
            .unwrap()
    }

    #[test]
    fn about_mx_shows_a_read_only_splash() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "about-mx");
        press(&mut editor, "RET");

        assert_eq!(editor.buffer().name(), "*mx*");
        assert!(editor.buffer().is_read_only());
        assert!(!editor.buffer().is_modified());
        let lines = lines(&editor);
        assert_eq!(lines[1], format!("   mx {}", env!("CARGO_PKG_VERSION")));
        assert_eq!(lines[2], "   An Emacs-inspired editor, written in Rust.");
    }

    #[test]
    fn the_cheatsheet_takes_keys_from_the_keymap() {
        let lines = lines(&shown());

        for (key, command, description) in [
            ("C-x C-f", "find-file", "Open a file"),
            ("C-x d", "dired", "Browse a directory"),
            ("C-x b", "switch-to-buffer", "Switch buffers"),
            ("M-x", "execute-extended-command", "Run any command"),
            ("C-x C-s", "save-buffer", "Save"),
            ("C-x C-c", "save-buffers-kill-mx", "Quit"),
        ] {
            let line = lines
                .iter()
                .find(|line| line.trim_start().starts_with(key))
                .unwrap_or_else(|| panic!("{key} missing"));
            let words: Vec<&str> = line.split("  ").filter(|word| !word.is_empty()).collect();
            assert_eq!(
                words.iter().map(|word| word.trim()).collect::<Vec<_>>(),
                [key, command, description]
            );
        }
    }

    #[test]
    fn point_starts_on_the_first_key() {
        assert!(at_point(&shown()).starts_with("C-x C-f"));
    }

    #[test]
    fn n_and_p_move_between_keys_and_wrap() {
        let mut editor = shown();

        press(&mut editor, "n");
        assert!(at_point(&editor).starts_with("C-x d"));

        press(&mut editor, "p p");
        assert!(at_point(&editor).starts_with("C-x C-c"));

        press(&mut editor, "TAB");
        assert!(at_point(&editor).starts_with("C-x C-f"));
    }

    #[test]
    fn return_runs_the_command_on_the_line() {
        let mut editor = shown();

        press(&mut editor, "n RET");

        assert_eq!(editor.minibuffer().unwrap().prompt(), "Dired (directory): ");
    }

    #[test]
    fn clicking_a_key_line_runs_it() {
        let mut editor = shown();
        let line = line_of(&editor, "C-x b");

        editor.click(
            Cell {
                row: line,
                column: 20,
            },
            Clicks::Single,
            false,
        );

        assert_eq!(editor.minibuffer().unwrap().prompt(), "Switch to buffer: ");
    }

    #[test]
    fn clicking_elsewhere_only_moves_point() {
        let mut editor = shown();

        editor.click(Cell { row: 2, column: 5 }, Clicks::Single, false);

        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 2);
    }

    #[test]
    fn q_buries_the_splash() {
        let mut editor = shown();

        press(&mut editor, "q");

        assert_eq!(editor.buffer_names(), ["*scratch*", "*mx*"]);
    }

    #[test]
    fn showing_the_splash_again_reuses_it() {
        let mut editor = shown();
        press(&mut editor, "q");

        editor.about();

        assert_eq!(editor.buffer_names(), ["*mx*", "*scratch*"]);
    }

    #[test]
    fn the_splash_cannot_be_saved_written_or_made_writable() {
        let mut editor = shown();

        press(&mut editor, "C-x C-s");
        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.message(), Some("(No changes need to be saved)"));

        press(&mut editor, "C-x C-w");
        assert!(editor.minibuffer().is_none());
        assert_eq!(
            editor.message(),
            Some("This buffer can't be written to a file")
        );

        press(&mut editor, "C-x C-q");
        assert!(editor.buffer().is_read_only());
        assert_eq!(editor.message(), Some("This buffer stays read-only"));
    }

    #[test]
    fn the_title_and_keys_are_highlighted() {
        let editor = shown();
        let line = line_of(&editor, "C-x C-f");

        assert_eq!(
            editor.line_style(1).spans,
            [(3..usize::MAX, Highlight::Heading)]
        );
        assert_eq!(editor.line_style(line).spans, [(3..10, Highlight::Link)]);
        assert_eq!(editor.line_style(2), LineStyle::default());
    }
}
