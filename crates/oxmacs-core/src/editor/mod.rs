mod buffers;
mod clipboard;
mod comments;
mod compile;
mod dired;
mod disk;
mod doctor;
mod editing;
mod effect;
mod files;
mod format;
mod imenu;
mod indent;
mod isearch;
mod markdown;
mod minibuffer;
mod modes;
mod mouse;
mod movement;
mod pairs;
mod pane;
mod preview;
mod question;
mod read_only;
mod replace;
mod selection;
mod splash;
mod structure;
#[cfg(test)]
mod test_support;
mod themes;
mod tiling;
mod undo;
mod windows;

pub use compile::{Job, JobId, Outcome};
pub use effect::Effect;
pub use isearch::Search;
pub use minibuffer::{Choice, Completion, Minibuffer};
pub use mouse::{Cell, Clicks};
pub use pane::{Pane, ScreenRow};
pub use question::Question;
pub use replace::Replace;
pub use tiling::{Axis, Rect, Tiling, WindowId};

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::{env, mem};

use buffers::{BufferList, Mode};
use dired::Kind;
use isearch::Direction as SearchDirection;
use windows::Direction;

use crate::command::{Command, Scope};
use crate::config::{self, Config, OnDiskChange};
use crate::dired::Mark;
use crate::display::{Layout, LineStyle, Viewport};
use crate::input::{Key, KeyCode, KeySequence, Keymap, Lookup, Modifiers, bindings};
use crate::syntax::Language;
use crate::text::{Buffer, motion};
use crate::theme::{self, Appearance, Highlight, Theme};
use crate::tools::{self, Probe};

const DEFAULT_ROWS: usize = 24;

#[derive(Debug, Clone, Copy)]
struct Settings {
    delete_to_trash: bool,
    format_on_save: bool,
}

impl From<&Config> for Settings {
    fn from(config: &Config) -> Self {
        Self {
            delete_to_trash: config.delete_to_trash,
            format_on_save: config.format_on_save,
        }
    }
}

#[derive(Debug)]
pub struct Editor {
    buffers: BufferList,
    windows: windows::Windows,
    minibuffer: Option<Minibuffer>,
    search: Option<Search>,
    replace: Option<Replace>,
    last_searches: [Option<String>; 2],
    last_replacements: [Option<(String, String)>; 2],
    keymap: Keymap,
    minibuffer_keymap: Keymap,
    dired_keymap: Keymap,
    splash_keymap: Keymap,
    doctor_keymap: Keymap,
    rust_keymap: Keymap,
    markdown_keymap: Keymap,
    compilation_keymap: Keymap,
    search_keymap: Keymap,
    pending: Vec<Key>,
    last_key: Option<Key>,
    last_command: Option<Command>,
    command_history: Vec<Command>,
    goal_column: Option<usize>,
    shift_selection: bool,
    typed_in_step: usize,
    drag_anchor: Option<usize>,
    primary: Option<Range<usize>>,
    effects: Vec<Effect>,
    clipboard_history: Vec<String>,
    line_copy: Option<String>,
    message: Option<String>,
    quit_requested: bool,
    question: Option<Question>,
    viewport: Viewport,
    tab_width: usize,
    on_disk_change: OnDiskChange,
    settings: Settings,
    appearance: Appearance,
    configured_theme: Option<&'static Theme>,
    working_directory: PathBuf,
    home: Option<PathBuf>,
    probe: fn(&str) -> Probe,
    formatter: fn(&str, &Path, &str) -> Result<String, String>,
    font_note: Option<String>,
    config_path: Option<PathBuf>,
    paren_pair: Option<(usize, usize)>,
    next_job: u64,
    last_compile: Option<String>,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    pub fn new() -> Self {
        Self::with_buffer(Buffer::new(buffers::SCRATCH))
    }

    pub fn with_buffer(buffer: Buffer) -> Self {
        Self {
            buffers: BufferList::new(buffer),
            windows: windows::Windows::new(),
            minibuffer: None,
            search: None,
            replace: None,
            last_searches: [None, None],
            last_replacements: [None, None],
            keymap: bindings::global(),
            minibuffer_keymap: bindings::minibuffer(),
            dired_keymap: bindings::dired(),
            splash_keymap: bindings::splash(),
            doctor_keymap: bindings::doctor(),
            rust_keymap: bindings::rust(),
            markdown_keymap: bindings::markdown(),
            compilation_keymap: bindings::compilation(),
            search_keymap: bindings::search(),
            pending: Vec::new(),
            last_key: None,
            last_command: None,
            command_history: Vec::new(),
            goal_column: None,
            shift_selection: false,
            typed_in_step: 0,
            drag_anchor: None,
            primary: None,
            effects: Vec::new(),
            clipboard_history: Vec::new(),
            line_copy: None,
            message: None,
            quit_requested: false,
            question: None,
            viewport: Viewport::new(DEFAULT_ROWS, 0),
            tab_width: Config::default().tab_width,
            on_disk_change: OnDiskChange::default(),
            settings: Settings::from(&Config::default()),
            appearance: Appearance::Light,
            configured_theme: None,
            working_directory: env::current_dir().unwrap_or_default(),
            home: dirs::home_dir(),
            probe: tools::probe,
            formatter: tools::rustfmt,
            font_note: None,
            config_path: config::default_path(),
            paren_pair: None,
            next_job: 0,
            last_compile: None,
        }
    }

    pub fn apply_config(&mut self, config: &Config) {
        self.tab_width = config.tab_width;
        self.on_disk_change = config.on_disk_change;
        self.settings = Settings::from(config);
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
        self.buffers.current()
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

    pub fn resize_window(&mut self, rows: usize, columns: usize) {
        self.viewport.resize(rows, columns);
        self.reveal_point();
    }

    pub(super) fn layout(&self) -> Layout<'_> {
        Layout::new(
            self.buffers.current(),
            self.viewport.wrap_width(),
            self.tab_width,
        )
    }

    pub(super) fn reveal_point(&mut self) {
        let layout = Layout::new(
            self.buffers.current(),
            self.viewport.wrap_width(),
            self.tab_width,
        );
        let spot = layout.screen_line(self.buffers.current().point());
        self.viewport.reveal(spot, &layout);
    }

    fn point_line(&self) -> usize {
        self.buffers
            .current()
            .line_of(self.buffers.current().point())
    }

    fn current_mut(&mut self) -> &mut Buffer {
        match &mut self.minibuffer {
            Some(minibuffer) => &mut minibuffer.input,
            None => self.buffers.current_mut(),
        }
    }

    pub fn tab_width(&self) -> usize {
        self.tab_width
    }

    pub fn point_display_column(&self) -> usize {
        let buffer = self.buffers.current();
        movement::display_column(buffer, buffer.point(), self.tab_width)
    }

    pub fn mode_line(&self) -> String {
        self.pane().mode_line()
    }

    pub fn handle_key(&mut self, mut key: Key) {
        self.message = None;
        if self.answer_question(key) || self.answer_replace(key) || self.search_key(key) {
            return;
        }
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
        self.publish_primary();
        self.prepare_display();
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
        let local = if self.minibuffer.is_some() {
            Some(&self.minibuffer_keymap)
        } else {
            self.mode_keymap()
        };
        if let Some(keymap) = local {
            match keymap.lookup(keys) {
                Lookup::Unbound => {}
                found => return found,
            }
        }
        self.keymap.lookup(keys)
    }

    fn mode_keymap(&self) -> Option<&Keymap> {
        match self.buffer_scope() {
            Scope::Dired => Some(&self.dired_keymap),
            Scope::Splash => Some(&self.splash_keymap),
            Scope::Doctor => Some(&self.doctor_keymap),
            Scope::Rust => Some(&self.rust_keymap),
            Scope::Markdown => Some(&self.markdown_keymap),
            Scope::Compilation => Some(&self.compilation_keymap),
            Scope::Global | Scope::Minibuffer | Scope::Search => None,
        }
    }

    fn buffer_scope(&self) -> Scope {
        match self.buffers.mode(0) {
            Some(Mode::Dired(_)) => Scope::Dired,
            Some(Mode::Splash(_)) => Scope::Splash,
            Some(Mode::Doctor(_)) => Scope::Doctor,
            Some(Mode::Compilation(_)) => Scope::Compilation,
            Some(Mode::Text(Some(syntax))) if syntax.language() == Language::Rust => Scope::Rust,
            Some(Mode::Text(Some(syntax))) if syntax.language() == Language::Markdown => {
                Scope::Markdown
            }
            Some(Mode::Text(_) | Mode::Preview(_)) | None => Scope::Global,
        }
    }

    pub fn line_style(&self, line: usize) -> LineStyle {
        self.line_style_at(0, line)
    }

    fn line_style_at(&self, index: usize, line: usize) -> LineStyle {
        match self.buffers.mode(index) {
            Some(Mode::Dired(listing)) => listing.line_style(line),
            Some(Mode::Splash(splash)) => splash.line_style(line),
            Some(Mode::Doctor(doctor)) => doctor.line_style(line),
            Some(Mode::Preview(_) | Mode::Compilation(_)) => LineStyle::default(),
            Some(Mode::Text(_)) | None => LineStyle {
                background: None,
                spans: self.syntax_highlights_at(index, line),
            },
        }
    }

    pub fn line_marks(&self, line: usize) -> Vec<(Range<usize>, Highlight)> {
        let mut marks = self.paren_marks(line);
        marks.extend(self.search_marks(line));
        marks.extend(self.replace_marks(line));
        marks
    }

    fn where_is(&self, command: Command) -> Option<KeySequence> {
        self.mode_keymap()
            .and_then(|keymap| keymap.where_is(command))
            .or_else(|| self.keymap.where_is(command))
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
        if command.modifies_text() && self.refuse_read_only() {
            return;
        }
        let goal_column = self.goal_column.take();
        let previous = self.last_command.replace(command);
        self.close_undo_step(command, previous);
        if command.is_motion() {
            self.prepare_motion_selection(shifted);
        }
        self.perform(command, goal_column, previous);
        self.reset_selection(command);
        self.refresh_minibuffer();
        self.reveal_point();
    }

    #[expect(clippy::too_many_lines, reason = "one arm per command")]
    fn perform(&mut self, command: Command, goal_column: Option<usize>, previous: Option<Command>) {
        match command {
            Command::SelfInsertCommand => self.self_insert(),
            Command::Newline => self.newline(),
            Command::IndentForTabCommand => self.indent_for_tab(),
            Command::IndentRegion => self.indent_region(),
            Command::CommentLine => self.comment_line(),
            Command::CommentDwim => self.comment_dwim(),
            Command::DeleteChar => self.delete_char(true),
            Command::DeleteBackwardChar => self.delete_char(false),
            Command::DeleteLine => self.delete_line(),
            Command::DeleteWord => self.delete_to(motion::forward_word),
            Command::BackwardDeleteWord => self.delete_to(motion::backward_word),
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
            Command::BeginningOfDefun => self.beginning_of_defun(),
            Command::EndOfDefun => self.end_of_defun(),
            Command::ForwardSexp => self.forward_sexp(),
            Command::BackwardSexp => self.backward_sexp(),
            Command::BackwardUpList => self.backward_up_list(),
            Command::DownList => self.down_list(),
            Command::SetMarkCommand => self.set_mark_command(previous),
            Command::ExchangePointAndMark => self.exchange_point_and_mark(),
            Command::MarkWholeBuffer => self.mark_whole_buffer(),
            Command::Copy => self.copy(false),
            Command::Cut => self.copy(true),
            Command::Paste => self.request_paste(),
            Command::PasteFromHistory => self.paste_from_history(),
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::ExecuteExtendedCommand => self.execute_extended_command(),
            Command::ExitMinibuffer => self.exit_minibuffer(),
            Command::MinibufferNext => self.minibuffer_next(),
            Command::MinibufferPrevious => self.minibuffer_previous(),
            Command::MinibufferComplete => self.minibuffer_complete(),
            Command::MinibufferDeleteBackward => self.minibuffer_delete_backward(),
            Command::ExitMinibufferWithInput => self.exit_minibuffer_with_input(),
            Command::IsearchForward => self.isearch(SearchDirection::Forward, false),
            Command::IsearchBackward => self.isearch(SearchDirection::Backward, false),
            Command::IsearchForwardRegexp => self.isearch(SearchDirection::Forward, true),
            Command::QueryReplace => self.query_replace(false),
            Command::QueryReplaceRegexp => self.query_replace(true),
            Command::IsearchBackwardRegexp => self.isearch(SearchDirection::Backward, true),
            Command::IsearchRepeatForward
            | Command::IsearchRepeatBackward
            | Command::IsearchPrintingChar
            | Command::IsearchDeleteChar
            | Command::IsearchToggleRegexp
            | Command::IsearchYankWordOrChar
            | Command::IsearchYankKill
            | Command::IsearchQueryReplace
            | Command::IsearchQueryReplaceRegexp
            | Command::IsearchExit
            | Command::IsearchAbort => self.run_in_search(command),
            Command::FindFile => self.find_file(),
            Command::SaveBuffer => self.save_buffer(),
            Command::WriteFile => self.write_file(),
            Command::SaveSomeBuffers => self.save_some_buffers(false),
            Command::SwitchToBuffer => self.switch_to_buffer(),
            Command::KillBuffer => self.kill_buffer(),
            Command::RevertBuffer => self.revert_buffer(),
            Command::ReadOnlyMode => self.toggle_read_only(),
            Command::Dired => self.dired_prompt(),
            Command::DiredJump => self.dired_jump(),
            Command::DiredNextLine => self.dired_next_line(1),
            Command::DiredPreviousLine => self.dired_next_line(-1),
            Command::DiredFindFile => self.dired_find_file(),
            Command::DiredUpDirectory => self.dired_up_directory(),
            Command::DiredCycleSort => self.dired_cycle_sort(),
            Command::DiredMark => self.dired_mark(Some(Mark::Marked)),
            Command::DiredFlagFileDeletion => self.dired_mark(Some(Mark::Flagged)),
            Command::DiredUnmark => self.dired_mark(None),
            Command::DiredUnmarkBackward => self.dired_unmark_backward(),
            Command::DiredUnmarkAllMarks => self.dired_unmark_all(),
            Command::DiredToggleMarks => self.dired_toggle_marks(),
            Command::DiredDoFlaggedDelete => self.dired_do_delete(true),
            Command::DiredDoDelete => self.dired_do_delete(false),
            Command::DiredDoRename => self.dired_do_transfer(Kind::Move),
            Command::DiredDoCopy => self.dired_do_transfer(Kind::Copy),
            Command::DiredCreateDirectory => self.dired_create_directory(),
            Command::DiredDoOpen => self.dired_do_open(),
            Command::DiredHideDetailsMode => self.dired_hide_details(),
            Command::QuitWindow => self.quit_window(),
            Command::KeyboardQuit => self.keyboard_quit(),
            Command::SaveBuffersKillOxmacs => self.save_some_buffers(true),
            Command::KillOxmacs => self.quit_requested = true,
            Command::AboutOxmacs => self.about(),
            Command::LoadTheme => self.load_theme(),
            Command::ToggleTheme => self.toggle_theme(),
            Command::RustMode => self.set_major_mode(Some(Language::Rust)),
            Command::TomlMode => self.set_major_mode(Some(Language::Toml)),
            Command::MarkdownMode => self.set_major_mode(Some(Language::Markdown)),
            Command::TextMode => self.set_major_mode(None),
            Command::OxmacsDoctor => self.oxmacs_doctor(),
            Command::SplitWindowBelow => self.split_window(Axis::Rows),
            Command::SplitWindowRight => self.split_window(Axis::Columns),
            Command::OtherWindow => self.other_window(),
            Command::DeleteWindow => self.delete_window(),
            Command::DeleteOtherWindows => self.delete_other_windows(),
            Command::WindmoveLeft => self.windmove(Direction::Left),
            Command::WindmoveRight => self.windmove(Direction::Right),
            Command::WindmoveUp => self.windmove(Direction::Up),
            Command::WindmoveDown => self.windmove(Direction::Down),
            Command::BalanceWindows => self.balance_windows(),
            Command::EnlargeWindow => self.enlarge_window(Axis::Rows, 1),
            Command::EnlargeWindowHorizontally => self.enlarge_window(Axis::Columns, 1),
            Command::ShrinkWindowHorizontally => self.enlarge_window(Axis::Columns, -1),
            Command::RustFormatBuffer => self.rust_format_buffer(),
            Command::Imenu => self.imenu(),
            Command::MarkdownInsertListItem => self.markdown_insert_list_item(),
            Command::MarkdownPromote => self.markdown_shift(false),
            Command::MarkdownDemote => self.markdown_shift(true),
            Command::MarkdownPreview => self.markdown_preview(),
            Command::Compile => self.compile(),
            Command::Recompile => self.recompile(),
            Command::KillCompilation => self.kill_compilation(),
            Command::ForwardButton => self.forward_button(true),
            Command::BackwardButton => self.forward_button(false),
            Command::PushButton => self.push_button(),
        }
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
        editor.resize_window(10, 0);

        press(&mut editor, "M->");
        assert_eq!(editor.viewport().top().line, 24);

        press(&mut editor, "M-<");
        assert_eq!(editor.viewport().top().line, 0);
    }

    #[test]
    fn point_below_the_window_recenters_by_screen_rows() {
        let mut editor = editor_with(&"x".repeat(40), 0);
        editor.resize_window(4, 5);

        press(&mut editor, "M->");

        assert_eq!(
            editor.viewport().top(),
            crate::display::ScreenLine { line: 0, row: 7 }
        );
    }

    #[test]
    fn resizing_the_window_keeps_point_visible() {
        let mut editor = editor_with(&numbered_lines(30), 0);
        editor.resize_window(20, 0);
        press(
            &mut editor,
            "C-n C-n C-n C-n C-n C-n C-n C-n C-n C-n C-n C-n",
        );
        assert_eq!(editor.viewport().top().line, 0);

        editor.resize_window(5, 0);

        let top = editor.viewport().top().line;
        assert!((top..top + 5).contains(&12));
    }

    #[test]
    fn mode_line_shows_buffer_name_line_and_display_column() {
        let editor = editor_with("ab\n\tcd", 5);

        assert_eq!(editor.mode_line(), "-- t   L2 C9");
    }

    #[test]
    fn mode_line_marks_modified_buffers() {
        let mut editor = editor_with("ab", 0);

        press(&mut editor, "x");

        assert_eq!(editor.mode_line(), "** t   L1 C1");
    }

    #[test]
    fn mode_line_shows_crlf_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("w.txt"), "one\r\n").unwrap();
        let mut editor = Editor::new();

        editor.open_file(&dir.path().join("w.txt"));

        assert_eq!(editor.mode_line(), "-- w.txt   L1 C0   CRLF");
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
