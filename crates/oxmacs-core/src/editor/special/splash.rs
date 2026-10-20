use std::fmt::Write;

use crate::command::{Command, CommandId, Scope};
use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
#[cfg(feature = "animated-splash")]
use crate::editor::special::splash_art::{self, SplashAnimation};
use crate::text::Buffer;
use crate::theme::Highlight;

const NAME: &str = "*oxmacs*";
const TAGLINE: &str = "An Emacs-inspired editor, written in Rust.";
const INDENT: usize = 3;
const TITLE_LINE: usize = 1;
const FIRST_BUTTON_LINE: usize = 4;
const BUTTONS: &[(Command, &str)] = &[
    (Command::FindFile, "Open a file"),
    (Command::Dired, "Browse a directory"),
    (Command::SwitchToBuffer, "Switch buffers"),
    (Command::IsearchForward, "Search"),
    (Command::ExecuteExtendedCommand, "Run any command"),
    (Command::DescribeCommand, "Explain a command"),
    (Command::SaveBuffer, "Save"),
    (Command::SaveBuffersKillOxmacs, "Quit"),
];

#[derive(Debug)]
pub(in crate::editor) struct Splash {
    pub(super) buttons: Vec<Button>,
    #[cfg(feature = "animated-splash")]
    pub(in crate::editor) art: SplashAnimation,
}

#[derive(Debug)]
pub(super) struct Button {
    line: usize,
    command: CommandId,
    key_width: usize,
}

impl Splash {
    fn button(&self, line: usize) -> Option<&Button> {
        self.buttons.iter().find(|button| button.line == line)
    }

    pub(in crate::editor) fn has_button(&self, line: usize) -> bool {
        self.button(line).is_some()
    }

    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
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
        if let Some(index) = self
            .buffers
            .find_mode(|mode| matches!(mode, Mode::Splash(_)))
        {
            self.select_buffer(index);
            return;
        }
        #[cfg(feature = "animated-splash")]
        let extras = splash_art::extras(SplashAnimation::default());
        #[cfg(not(feature = "animated-splash"))]
        let extras = Vec::new();
        let (text, buttons) = self.splash_text(&extras);
        let mut buffer = Buffer::with_text(NAME, &text);
        buffer.set_read_only(true);
        buffer.set_point(buffer.line_start(FIRST_BUTTON_LINE) + INDENT);
        let splash = Splash {
            buttons,
            #[cfg(feature = "animated-splash")]
            art: SplashAnimation::default(),
        };
        self.add_buffer_with(buffer, Mode::Splash(splash));
    }

    pub fn set_splash_entry(&mut self, command: CommandId, description: impl Into<String>) {
        let description = description.into();
        match self
            .splash_entries
            .iter_mut()
            .find(|(known, _)| *known == command)
        {
            Some(entry) => entry.1 = description,
            None => self.splash_entries.push((command, description)),
        }
        self.refill_splash();
    }

    pub(super) fn refill_splash(&mut self) {
        let Some(index) = self
            .buffers
            .find_mode(|mode| matches!(mode, Mode::Splash(_)))
        else {
            return;
        };
        #[cfg(feature = "animated-splash")]
        let extras = self
            .buffers
            .splash(index)
            .map(|splash| splash_art::extras(splash.art))
            .unwrap_or_default();
        #[cfg(not(feature = "animated-splash"))]
        let extras = Vec::new();
        let (text, buttons) = self.splash_text(&extras);
        if let Some((buffer, Mode::Splash(splash))) = self.buffers.entry_mut(index) {
            Page {
                text,
                styles: Vec::new(),
            }
            .fill(buffer);
            splash.buttons = buttons;
        }
    }

    pub(super) fn splash_text(&self, extras: &[(Command, String)]) -> (String, Vec<Button>) {
        let mut entries: Vec<(CommandId, String)> = BUTTONS
            .iter()
            .map(|(command, description)| ((*command).into(), (*description).to_owned()))
            .collect();
        entries.extend(
            extras
                .iter()
                .map(|(command, description)| ((*command).into(), description.clone())),
        );
        entries.extend(self.splash_entries.iter().cloned());
        let keys: Vec<String> = entries
            .iter()
            .map(|(command, _)| {
                self.keymaps
                    .get(Scope::Global)
                    .where_is(*command)
                    .or_else(|| self.keymaps.get(Scope::Splash).where_is(*command))
                    .map(|keys| keys.to_string())
                    .unwrap_or_default()
            })
            .collect();
        let key_width = keys
            .iter()
            .map(|key| key.chars().count())
            .max()
            .unwrap_or(0);
        let name_width = entries
            .iter()
            .map(|(command, _)| self.commands.name(*command).len())
            .max()
            .unwrap_or(0);
        let pad = " ".repeat(INDENT);
        let mut text = format!(
            "\n{pad}oxmacs {}\n{pad}{TAGLINE}\n\n",
            env!("CARGO_PKG_VERSION")
        );
        let mut buttons = Vec::new();
        for (index, ((command, description), key)) in entries.iter().zip(&keys).enumerate() {
            let name = self.commands.name(*command);
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
        (text, buttons)
    }

    pub(in crate::editor) fn forward_button(&mut self, forward: bool) {
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

    pub(in crate::editor) fn push_button(&mut self) {
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
    use crate::command::{CommandSpec, Scope};
    use crate::display::LineStyle;
    use crate::editor::test_support::{at_point, lines, press, type_text};
    use crate::editor::{Cell, Clicks, Editor};
    use crate::theme::Highlight;

    fn shown() -> Editor {
        let mut editor = Editor::new();
        editor.about();
        editor
    }

    #[test]
    fn the_start_screen_points_to_command_help() {
        let editor = shown();

        assert!(
            lines(&editor)
                .iter()
                .any(|line| line.contains("C-h f") && line.contains("Explain a command"))
        );
    }

    fn line_of(editor: &Editor, key: &str) -> usize {
        lines(editor)
            .iter()
            .position(|line| line.trim_start().starts_with(key))
            .unwrap()
    }

    #[test]
    fn about_oxmacs_shows_a_read_only_splash() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "about-oxmacs");
        press(&mut editor, "RET");

        assert_eq!(editor.buffer().name(), "*oxmacs*");
        assert!(editor.buffer().is_read_only());
        assert!(!editor.buffer().is_modified());
        let lines = lines(&editor);
        assert_eq!(lines[1], format!("   oxmacs {}", env!("CARGO_PKG_VERSION")));
        assert_eq!(lines[2], "   An Emacs-inspired editor, written in Rust.");
    }

    #[test]
    fn the_cheatsheet_takes_keys_from_the_keymap() {
        let lines = lines(&shown());

        for (key, command, description) in [
            ("C-x C-f", "find-file", "Open a file"),
            ("C-x d", "dired", "Browse a directory"),
            ("C-x b", "switch-to-buffer", "Switch buffers"),
            ("C-s", "isearch-forward", "Search"),
            ("M-x", "execute-extended-command", "Run any command"),
            ("C-x C-s", "save-buffer", "Save"),
            ("C-x C-c", "save-buffers-kill-oxmacs", "Quit"),
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
        let last = if cfg!(feature = "animated-splash") {
            "a "
        } else {
            "C-x C-c"
        };
        assert!(at_point(&editor).starts_with(last));

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

        assert_eq!(editor.buffer_names(), ["*scratch*", "*oxmacs*"]);
    }

    #[test]
    fn showing_the_splash_again_reuses_it() {
        let mut editor = shown();
        press(&mut editor, "q");

        editor.about();

        assert_eq!(editor.buffer_names(), ["*oxmacs*", "*scratch*"]);
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

    fn wave(editor: &mut Editor) {
        editor.show_message("Hello");
    }

    #[test]
    fn applets_add_lines_that_run_their_commands_and_can_reword_them() {
        let mut editor = Editor::new();
        let command = editor.add_command(CommandSpec {
            name: "wave-hello",
            summary: "Wave hello",
            help: None,
            scope: Scope::Splash.into(),
            run: wave,
        });
        editor.bind(Scope::Splash, "w", command).unwrap();
        editor.set_splash_entry(command, "Wave");
        editor.about();

        let line = line_of(&editor, "w ");
        assert!(lines(&editor)[line].contains("wave-hello"));
        assert!(lines(&editor)[line].ends_with("Wave"));
        assert_eq!(editor.line_style(line).spans, [(3..4, Highlight::Link)]);

        press(&mut editor, "n");
        editor.set_splash_entry(command, "Wave again");
        assert!(lines(&editor)[line].ends_with("Wave again"));
        assert!(at_point(&editor).starts_with("C-x d"));

        editor.click(
            Cell {
                row: line,
                column: 20,
            },
            Clicks::Single,
            false,
        );
        assert_eq!(editor.message(), Some("Hello"));
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
