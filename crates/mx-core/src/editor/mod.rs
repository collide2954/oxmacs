mod clipboard;
mod editing;
mod minibuffer;
mod movement;
mod selection;
#[cfg(test)]
mod test_support;

pub use clipboard::Effect;
pub use minibuffer::{Choice, Completion, Minibuffer};

use std::mem;

use crate::command::Command;
use crate::config::Config;
use crate::display::{self, Viewport};
use crate::input::{Key, KeyCode, KeySequence, Keymap, Lookup, Modifiers, bindings};
use crate::text::{Buffer, motion};
use crate::theme::{self, Appearance, Theme};

const DEFAULT_ROWS: usize = 24;

#[derive(Debug)]
pub struct Editor {
    buffer: Buffer,
    minibuffer: Option<Minibuffer>,
    keymap: Keymap,
    minibuffer_keymap: Keymap,
    pending: Vec<Key>,
    last_key: Option<Key>,
    last_command: Option<Command>,
    command_history: Vec<Command>,
    goal_column: Option<usize>,
    shift_selection: bool,
    effects: Vec<Effect>,
    clipboard_history: Vec<String>,
    line_copy: Option<String>,
    message: Option<String>,
    quit_requested: bool,
    viewport: Viewport,
    tab_width: usize,
    appearance: Appearance,
    configured_theme: Option<&'static Theme>,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    pub fn new() -> Self {
        Self::with_buffer(Buffer::new("*scratch*"))
    }

    pub fn with_buffer(buffer: Buffer) -> Self {
        Self {
            buffer,
            minibuffer: None,
            keymap: bindings::global(),
            minibuffer_keymap: bindings::minibuffer(),
            pending: Vec::new(),
            last_key: None,
            last_command: None,
            command_history: Vec::new(),
            goal_column: None,
            shift_selection: false,
            effects: Vec::new(),
            clipboard_history: Vec::new(),
            line_copy: None,
            message: None,
            quit_requested: false,
            viewport: Viewport::new(DEFAULT_ROWS),
            tab_width: Config::default().tab_width,
            appearance: Appearance::Light,
            configured_theme: None,
        }
    }

    pub fn apply_config(&mut self, config: &Config) {
        self.tab_width = config.tab_width;
        self.configured_theme = config.theme.as_deref().and_then(theme::find);
    }

    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.appearance = appearance;
    }

    pub fn theme(&self) -> &'static Theme {
        self.configured_theme
            .unwrap_or_else(|| theme::for_appearance(self.appearance))
    }

    pub fn show_message(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn should_quit(&self) -> bool {
        self.quit_requested
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn resize_window(&mut self, rows: usize) {
        self.viewport.resize(rows, self.point_line());
    }

    fn point_line(&self) -> usize {
        self.buffer.line_of(self.buffer.point())
    }

    fn current_mut(&mut self) -> &mut Buffer {
        match &mut self.minibuffer {
            Some(minibuffer) => &mut minibuffer.input,
            None => &mut self.buffer,
        }
    }

    pub fn tab_width(&self) -> usize {
        self.tab_width
    }

    pub fn point_display_column(&self) -> usize {
        let point = self.buffer.point();
        let line = self.buffer.line_text(self.point_line());
        display::display_column(&line, self.buffer.column(point), self.tab_width)
    }

    pub fn mode_line(&self) -> String {
        format!(
            "{}   L{} C{}",
            self.buffer.name(),
            self.point_line() + 1,
            self.point_display_column()
        )
    }

    pub fn handle_key(&mut self, mut key: Key) {
        self.message = None;
        if let Some(minibuffer) = &mut self.minibuffer {
            minibuffer.note = None;
        }
        if self.quits_pending_sequence(key) {
            self.dispatch(key, Command::KeyboardQuit, false);
            return;
        }
        self.pending.push(key);
        let mut lookup = self.lookup(&self.pending);
        if lookup == Lookup::Unbound
            && let Some(meta) = self.fold_escape_into_meta()
        {
            key = meta;
            lookup = self.lookup(&self.pending);
        }
        match lookup {
            Lookup::Prefix => self.echo_pending(),
            Lookup::Unbound if is_escape(key) => self.echo_pending(),
            Lookup::Command(command) => self.dispatch(key, command, false),
            Lookup::Unbound if self.pending.len() == 1 => self.handle_unbound_key(key),
            Lookup::Unbound => self.report_undefined(),
        }
    }

    fn handle_unbound_key(&mut self, key: Key) {
        if let Some(command) = self.shifted_motion(key) {
            self.dispatch(key, command, true);
        } else if inserts_itself(key) {
            self.dispatch(key, Command::SelfInsertCommand, false);
        } else {
            self.report_undefined();
        }
    }

    fn shifted_motion(&self, key: Key) -> Option<Command> {
        match self.lookup(&[key.unshifted()?]) {
            Lookup::Command(command) if command.is_motion() => Some(command),
            _ => None,
        }
    }

    fn report_undefined(&mut self) {
        let keys = KeySequence::from(mem::take(&mut self.pending));
        self.message = Some(format!("{keys} is undefined"));
    }

    fn quits_pending_sequence(&self, key: Key) -> bool {
        let after_escape = self.pending.last().copied().is_some_and(is_escape);
        !self.pending.is_empty() && (is_keyboard_quit(key) || (is_escape(key) && after_escape))
    }

    fn fold_escape_into_meta(&mut self) -> Option<Key> {
        let [.., escape, key] = self.pending[..] else {
            return None;
        };
        if !is_escape(escape) || key.modifiers().contains(Modifiers::META) {
            return None;
        }
        let meta = Key::new(key.code(), key.modifiers() | Modifiers::META);
        self.pending.truncate(self.pending.len() - 2);
        self.pending.push(meta);
        Some(meta)
    }

    fn echo_pending(&mut self) {
        let keys = KeySequence::from(self.pending.clone());
        self.message = Some(format!("{keys}-"));
    }

    fn lookup(&self, keys: &[Key]) -> Lookup {
        if self.minibuffer.is_some() {
            match self.minibuffer_keymap.lookup(keys) {
                Lookup::Unbound => {}
                found => return found,
            }
        }
        self.keymap.lookup(keys)
    }

    fn dispatch(&mut self, key: Key, command: Command, shifted: bool) {
        self.pending.clear();
        self.last_key = Some(key);
        self.run(command, shifted);
    }

    pub fn execute(&mut self, command: Command) {
        self.run(command, false);
    }

    fn run(&mut self, command: Command, shifted: bool) {
        let goal_column = self.goal_column.take();
        let previous = self.last_command.replace(command);
        if command.is_motion() {
            self.prepare_motion_selection(shifted);
        }
        match command {
            Command::SelfInsertCommand => self.self_insert(),
            Command::Newline => self.current_mut().insert("\n"),
            Command::DeleteChar => self.current_mut().delete_forward(1),
            Command::DeleteBackwardChar => self.current_mut().delete_backward(1),
            Command::ForwardChar => self.move_point(|b, p| motion::forward_char(b, p, 1)),
            Command::BackwardChar => self.move_point(|_, p| motion::backward_char(p, 1)),
            Command::NextLine => self.move_vertically(1, goal_column),
            Command::PreviousLine => self.move_vertically(-1, goal_column),
            Command::MoveBeginningOfLine => self.move_point(motion::beginning_of_line),
            Command::MoveEndOfLine => self.move_point(motion::end_of_line),
            Command::ForwardWord => self.move_point(motion::forward_word),
            Command::BackwardWord => self.move_point(motion::backward_word),
            Command::BeginningOfBuffer => self.move_point(|_, _| 0),
            Command::EndOfBuffer => self.move_point(|b, _| b.len_chars()),
            Command::SetMarkCommand => self.set_mark_command(previous),
            Command::ExchangePointAndMark => self.exchange_point_and_mark(),
            Command::MarkWholeBuffer => self.mark_whole_buffer(),
            Command::Copy => self.copy(false),
            Command::Cut => self.copy(true),
            Command::Paste => self.request_paste(),
            Command::PasteFromHistory => self.paste_from_history(),
            Command::ExecuteExtendedCommand => self.execute_extended_command(),
            Command::ExitMinibuffer => self.exit_minibuffer(),
            Command::MinibufferNext => self.minibuffer_next(),
            Command::MinibufferPrevious => self.minibuffer_previous(),
            Command::MinibufferComplete => self.minibuffer_complete(),
            Command::KeyboardQuit => self.keyboard_quit(),
            Command::SaveBuffersKillMx => self.quit_requested = true,
        }
        self.reset_selection(command);
        self.viewport.reveal(self.point_line());
    }
}

fn is_keyboard_quit(key: Key) -> bool {
    key.code() == KeyCode::Char('g') && key.modifiers() == Modifiers::CONTROL
}

fn is_escape(key: Key) -> bool {
    key.code() == KeyCode::Escape && key.modifiers().is_empty()
}

fn inserts_itself(key: Key) -> bool {
    let chords = Modifiers::CONTROL | Modifiers::META | Modifiers::SUPER;
    matches!(key.code(), KeyCode::Char(c) if !c.is_control()) && !key.modifiers().intersects(chords)
}

#[cfg(test)]
mod tests {
    use super::test_support::{editor_with, press};
    use super::*;

    #[test]
    fn starts_with_empty_scratch_buffer() {
        let editor = Editor::new();

        assert_eq!(editor.buffer().name(), "*scratch*");
        assert_eq!(editor.buffer().contents(), "");
    }

    #[test]
    fn unbound_keys_report_undefined_and_insert_nothing() {
        let mut editor = Editor::new();

        press(&mut editor, "C-q");

        assert_eq!(editor.buffer().contents(), "");
        assert_eq!(editor.message(), Some("C-q is undefined"));
    }

    #[test]
    fn next_key_clears_the_message() {
        let mut editor = Editor::new();

        press(&mut editor, "C-q a");

        assert_eq!(editor.message(), None);
    }

    #[test]
    fn prefix_keys_are_echoed_while_pending() {
        let mut editor = Editor::new();

        press(&mut editor, "C-x");

        assert_eq!(editor.message(), Some("C-x-"));
    }

    #[test]
    fn keyboard_quit_reports_quit() {
        let mut editor = Editor::new();

        press(&mut editor, "C-g");

        assert_eq!(editor.message(), Some("Quit"));
    }

    #[test]
    fn keyboard_quit_aborts_a_pending_prefix() {
        let mut editor = Editor::new();

        press(&mut editor, "C-x C-g");
        assert_eq!(editor.message(), Some("Quit"));

        press(&mut editor, "a");
        assert_eq!(editor.buffer().contents(), "a");
    }

    #[test]
    fn escape_twice_quits_like_c_g() {
        let mut editor = editor_with("hello", 0);

        press(&mut editor, "C-SPC C-f ESC ESC");

        assert_eq!(editor.message(), Some("Quit"));
        assert_eq!(editor.buffer().selection(), None);
    }

    #[test]
    fn escape_twice_closes_the_minibuffer() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x a ESC ESC");

        assert!(editor.minibuffer().is_none());
    }

    #[test]
    fn escape_twice_aborts_a_pending_prefix() {
        let mut editor = Editor::new();

        press(&mut editor, "C-x ESC");
        assert_eq!(editor.message(), Some("C-x ESC-"));

        press(&mut editor, "ESC");
        assert_eq!(editor.message(), Some("Quit"));

        press(&mut editor, "a");
        assert_eq!(editor.buffer().contents(), "a");
    }

    #[test]
    fn escape_acts_as_a_meta_prefix() {
        let mut editor = editor_with("alpha beta", 0);

        press(&mut editor, "ESC f");
        assert_eq!(editor.buffer().point(), 5);

        press(&mut editor, "ESC <");
        assert_eq!(editor.buffer().point(), 0);

        press(&mut editor, "ESC x");
        assert!(editor.minibuffer().is_some());
    }

    #[test]
    fn escape_with_an_unbound_key_reports_the_meta_key() {
        let mut editor = Editor::new();

        press(&mut editor, "ESC q");

        assert_eq!(editor.message(), Some("M-q is undefined"));
    }

    #[test]
    fn quit_sequence_requests_exit() {
        let mut editor = Editor::new();
        assert!(!editor.should_quit());

        press(&mut editor, "C-x C-c");

        assert!(editor.should_quit());
    }

    fn numbered_lines(count: usize) -> String {
        (0..count)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn moving_point_out_of_the_window_scrolls_it() {
        let mut editor = editor_with(&numbered_lines(30), 0);
        editor.resize_window(10);

        press(&mut editor, "M->");
        assert_eq!(editor.viewport().top(), 24);

        press(&mut editor, "M-<");
        assert_eq!(editor.viewport().top(), 0);
    }

    #[test]
    fn resizing_the_window_keeps_point_visible() {
        let mut editor = editor_with(&numbered_lines(30), 0);
        editor.resize_window(20);
        press(
            &mut editor,
            "C-n C-n C-n C-n C-n C-n C-n C-n C-n C-n C-n C-n",
        );
        assert_eq!(editor.viewport().top(), 0);

        editor.resize_window(5);

        assert!(editor.viewport().lines().contains(&12));
    }

    #[test]
    fn mode_line_shows_buffer_name_line_and_display_column() {
        let editor = editor_with("ab\n\tcd", 5);

        assert_eq!(editor.mode_line(), "t   L2 C9");
    }

    #[test]
    fn theme_follows_os_appearance_when_not_configured() {
        let mut editor = Editor::new();
        assert_eq!(editor.theme().name, "adwaita");

        editor.set_appearance(Appearance::Dark);

        assert_eq!(editor.theme().name, "adwaita-dark");
    }

    #[test]
    fn configured_theme_overrides_os_appearance() {
        let mut editor = Editor::new();
        editor.apply_config(&Config::parse("theme = \"adwaita\"").unwrap());

        editor.set_appearance(Appearance::Dark);

        assert_eq!(editor.theme().name, "adwaita");
    }

    #[test]
    fn config_sets_tab_width() {
        let mut editor = editor_with("\tx", 1);

        editor.apply_config(&Config::parse("tab-width = 4").unwrap());

        assert_eq!(editor.tab_width(), 4);
        assert_eq!(editor.point_display_column(), 4);
    }

    #[test]
    fn shown_messages_appear_in_the_echo_area() {
        let mut editor = Editor::new();

        editor.show_message("config.toml: line 2: oops");

        assert_eq!(editor.message(), Some("config.toml: line 2: oops"));
    }
}
