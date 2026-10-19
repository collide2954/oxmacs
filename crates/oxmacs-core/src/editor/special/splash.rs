use std::fmt::Write;

use toml::Value;

use crate::command::{Command, Scope};
use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::text::Buffer;
use crate::theme::Highlight;

const NAME: &str = "*oxmacs*";
const EXPERIMENTAL_NAME: &str = "*oxmacs experimental*";
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplashAnimation {
    Plasma,
    Wave,
    Assemble,
}

impl SplashAnimation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Plasma => "plasma",
            Self::Wave => "color wave",
            Self::Assemble => "assemble",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Plasma => Self::Wave,
            Self::Wave => Self::Assemble,
            Self::Assemble => Self::Plasma,
        }
    }
}

#[derive(Debug)]
pub(in crate::editor) struct Splash {
    buttons: Vec<Button>,
    art: Option<SplashAnimation>,
}

#[derive(Debug)]
struct Button {
    line: usize,
    command: Command,
    key_width: usize,
}

impl Splash {
    pub(in crate::editor) fn art(&self) -> Option<SplashAnimation> {
        self.art
    }

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
        self.show_splash(None);
    }

    pub(in crate::editor) fn experimental_splash(&mut self) {
        self.show_splash(Some(SplashAnimation::Plasma));
    }

    pub(in crate::editor) fn experimental_features(&mut self, on: bool) {
        self.change_config("experimental-features", Some(Value::Boolean(on)));
        self.show_message(if on {
            "Experimental features are on; keep them with C-x C-s in M-x customize"
        } else {
            "Experimental features are off"
        });
    }

    pub fn splash_animation(&self) -> Option<SplashAnimation> {
        self.tiling()
            .windows()
            .into_iter()
            .filter_map(|window| self.pane_of(window))
            .find_map(|pane| pane.splash_art())
    }

    fn show_splash(&mut self, art: Option<SplashAnimation>) {
        let experimental = art.is_some();
        if let Some(index) = self.buffers.find_mode(
            |mode| matches!(mode, Mode::Splash(splash) if splash.art.is_some() == experimental),
        ) {
            self.select_buffer(index);
            return;
        }
        let (text, buttons) = self.splash_text(art);
        let name = if experimental {
            EXPERIMENTAL_NAME
        } else {
            NAME
        };
        let mut buffer = Buffer::with_text(name, &text);
        buffer.set_read_only(true);
        buffer.set_point(buffer.line_start(FIRST_BUTTON_LINE) + INDENT);
        self.add_buffer_with(buffer, Mode::Splash(Splash { buttons, art }));
    }

    fn splash_text(&self, art: Option<SplashAnimation>) -> (String, Vec<Button>) {
        let mut entries: Vec<(Command, String)> = BUTTONS
            .iter()
            .map(|(command, description)| (*command, (*description).to_owned()))
            .collect();
        if let Some(art) = art {
            entries.push((
                Command::CycleSplashAnimation,
                format!("Animation: {}", art.name()),
            ));
        }
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
            .map(|(command, _)| command.name().len())
            .max()
            .unwrap_or(0);
        let pad = " ".repeat(INDENT);
        let version = env!("CARGO_PKG_VERSION");
        let title = if art.is_some() {
            format!("oxmacs {version} · experimental")
        } else {
            format!("oxmacs {version}")
        };
        let mut text = format!("\n{pad}{title}\n{pad}{TAGLINE}\n\n");
        let mut buttons = Vec::new();
        for (index, ((command, description), key)) in entries.iter().zip(&keys).enumerate() {
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
        (text, buttons)
    }

    pub(in crate::editor) fn cycle_splash_animation(&mut self) {
        let Some(art) = self.buffers.splash(0).and_then(Splash::art) else {
            self.show_message("Only the experimental start screen animates");
            return;
        };
        let next = art.next();
        let (text, buttons) = self.splash_text(Some(next));
        if let Some((buffer, Mode::Splash(splash))) = self.buffers.entry_mut(0) {
            Page {
                text,
                styles: Vec::new(),
            }
            .fill(buffer);
            splash.art = Some(next);
            splash.buttons = buttons;
        }
        self.show_message(format!("Animation: {}", next.name()));
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
    use super::SplashAnimation;
    use crate::command::Command;
    use crate::display::LineStyle;
    use crate::editor::test_support::{at_point, lines, press, type_text};
    use crate::editor::{Cell, Clicks, Editor};
    use crate::theme::Highlight;

    fn shown() -> Editor {
        let mut editor = Editor::new();
        editor.about();
        editor
    }

    fn run(editor: &mut Editor, name: &str) {
        press(editor, "M-x");
        type_text(editor, name);
        press(editor, "RET");
    }

    fn experimental() -> Editor {
        let mut editor = Editor::new();
        run(&mut editor, "enable-experimental-features");
        run(&mut editor, "experimental-splash-screen");
        editor
    }

    #[test]
    fn experimental_commands_wait_until_they_are_enabled() {
        let mut editor = Editor::new();
        press(&mut editor, "M-x");
        type_text(&mut editor, "experimental-splash");
        assert!(
            editor
                .completions()
                .iter()
                .all(|completion| completion.label != "experimental-splash-screen")
        );
        press(&mut editor, "C-g");

        editor.execute(Command::ExperimentalSplashScreen);

        assert_eq!(editor.buffer().name(), "*scratch*");
        assert_eq!(
            editor.message(),
            Some(
                "experimental-splash-screen is experimental; M-x enable-experimental-features turns it on"
            )
        );
    }

    #[test]
    fn experimental_features_stay_on_for_the_session() {
        let mut editor = Editor::new();

        run(&mut editor, "enable-experimental-features");
        assert!(editor.config().experimental_features);
        assert_eq!(
            editor.message(),
            Some("Experimental features are on; keep them with C-x C-s in M-x customize")
        );

        run(&mut editor, "disable-experimental-features");
        assert!(!editor.config().experimental_features);
    }

    #[test]
    fn the_experimental_splash_animates_a_logo_above_the_keys() {
        let editor = experimental();

        assert_eq!(editor.buffer().name(), "*oxmacs experimental*");
        assert!(editor.buffer().is_read_only());
        assert_eq!(editor.pane().splash_art(), Some(SplashAnimation::Plasma));
        assert_eq!(editor.splash_animation(), Some(SplashAnimation::Plasma));
        let lines = lines(&editor);
        assert_eq!(
            lines[1],
            format!("   oxmacs {} · experimental", env!("CARGO_PKG_VERSION"))
        );
        assert!(at_point(&editor).starts_with("C-x C-f"));
        let cycle = lines
            .iter()
            .find(|line| line.trim_start().starts_with("a "))
            .unwrap();
        assert!(cycle.contains("cycle-splash-animation"), "{cycle}");
        assert!(cycle.ends_with("Animation: plasma"), "{cycle}");
    }

    #[test]
    fn a_cycles_through_the_three_animations() {
        let mut editor = experimental();

        for expected in [
            SplashAnimation::Wave,
            SplashAnimation::Assemble,
            SplashAnimation::Plasma,
            SplashAnimation::Wave,
        ] {
            press(&mut editor, "a");
            assert_eq!(editor.pane().splash_art(), Some(expected));
        }
        assert!(
            lines(&editor)
                .iter()
                .any(|line| line.ends_with("Animation: color wave"))
        );
        assert!(at_point(&editor).starts_with("C-x C-f"));
    }

    #[test]
    fn the_plain_splash_stays_separate_and_still() {
        let mut editor = experimental();

        editor.about();

        assert_eq!(editor.buffer().name(), "*oxmacs*");
        assert_eq!(editor.pane().splash_art(), None);
        press(&mut editor, "a");
        assert_eq!(
            editor.message(),
            Some("Only the experimental start screen animates")
        );
    }

    #[test]
    fn turning_experiments_off_stills_the_logo() {
        let mut editor = experimental();

        run(&mut editor, "disable-experimental-features");

        assert_eq!(editor.pane().splash_art(), None);
        assert_eq!(editor.splash_animation(), None);
        press(&mut editor, "a");
        assert!(editor.message().unwrap().contains("is experimental"));
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
