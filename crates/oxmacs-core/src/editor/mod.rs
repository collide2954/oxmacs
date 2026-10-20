mod atlas;
mod buffers;
mod build;
mod chat;
mod dired;
mod dispatch;
mod editing;
mod effect;
mod job;
mod languages;
mod motion;
#[cfg(test)]
mod opening;
mod page;
mod prompts;
mod search;
mod special;
#[cfg(test)]
mod test_support;
mod windows;

pub use effect::{ConnectionId, Effect, FetchId};
pub use job::{Job, JobId, Outcome};
pub use languages::preview::PreviewScroll;
pub use languages::r_plots::{GalleryView, PlotView};
pub use motion::mouse::{Cell, Clicks};
pub use prompts::minibuffer::{Choice, Completion, Minibuffer};
pub use prompts::popup::PopupView;
pub use prompts::question::Question;
pub use search::isearch::Search;
pub use search::replace::Replace;
#[cfg(feature = "animated-splash")]
pub use special::splash_art::SplashAnimation;
pub use windows::numbers::Gutter;
pub use windows::pane::{Pane, ScreenRow};
pub use windows::status::{BufferState, Status};
pub use windows::tiling::{Axis, Rect, Tiling, WindowId};

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::{env, mem};

use buffers::{BufferList, Mode};

use crate::command::{Command, CommandId, CommandSpec, Commands, Scope};
use crate::config::{self, Config, LineNumbers, OnDiskChange, RustLint};
use crate::display::{Layout, LineStyle, Viewport};
use crate::editor::chat::Irc;
use crate::editor::special::weather::WeatherState;
use crate::external::tools;
use crate::input::bindings::Keymaps;
use crate::input::{Key, KeyCode, KeyParseError, KeySequence, Keymap, Lookup, Modifiers};
use crate::syntax::Language;
use crate::text::Buffer;
use crate::theme::{self, Appearance, Highlight, Theme};

const DEFAULT_ROWS: usize = 24;

#[derive(Debug, Clone, Copy)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag mirrors a config.toml setting"
)]
struct Settings {
    delete_to_trash: bool,
    format_on_save: bool,
    rust_lint: RustLint,
    line_numbers: LineNumbers,
    line_number_separator: bool,
    auto_save: bool,
}

impl From<&Config> for Settings {
    fn from(config: &Config) -> Self {
        Self {
            delete_to_trash: config.delete_to_trash,
            format_on_save: config.format_on_save,
            rust_lint: config.rust_lint,
            line_numbers: config.line_numbers,
            line_number_separator: config.line_number_separator,
            auto_save: config.auto_save,
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
    keymaps: Keymaps,
    popup: Option<prompts::popup::Popup>,
    r_sessions: languages::r_console::Sessions,
    xref: Vec<(crate::text::BufferId, usize)>,
    atlas_notes: Vec<(crate::text::BufferId, u64, crate::atlas::note::Note)>,
    atlas_line: Option<(crate::text::BufferId, usize)>,
    today: fn() -> jiff::civil::Date,
    time_now: fn() -> jiff::civil::Time,
    atlas_index: Option<crate::atlas::vault::Index>,
    processes: special::processes::Processes,
    toolbox: special::toolbox::Toolbox,
    pending: Vec<Key>,
    last_key: Option<Key>,
    last_command: Option<Command>,
    command_history: Vec<CommandId>,
    commands: Commands,
    goal_column: Option<usize>,
    shift_selection: bool,
    typed_in_step: usize,
    drag_anchor: Option<usize>,
    line_anchor: Option<usize>,
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
    tool_report: special::doctor::ToolReport,
    find_r: fn(Option<&str>) -> Option<PathBuf>,
    formatter: fn(&str, &Path, &str) -> Result<String, String>,
    air_formatter: fn(&Path, &str, &Path) -> Result<String, String>,
    tools_root: Option<PathBuf>,
    font_note: Option<String>,
    config_path: Option<PathBuf>,
    weather: WeatherState,
    next_fetch: u64,
    irc: Irc,
    paren_pair: Option<(usize, usize)>,
    next_job: u64,
    last_compile: Option<String>,
    lints: build::lint::Lints,
    errors_from: build::errors::ErrorSource,
    config: Config,
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
            keymaps: Keymaps::default(),
            popup: None,
            r_sessions: languages::r_console::Sessions::default(),
            xref: Vec::new(),
            atlas_notes: Vec::new(),
            atlas_line: None,
            today: atlas::today,
            time_now: atlas::time_now,
            atlas_index: None,
            processes: special::processes::Processes::default(),
            toolbox: special::toolbox::Toolbox::default(),
            pending: Vec::new(),
            last_key: None,
            last_command: None,
            command_history: Vec::new(),
            commands: Commands::default(),
            goal_column: None,
            shift_selection: false,
            typed_in_step: 0,
            drag_anchor: None,
            line_anchor: None,
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
            tool_report: special::doctor::ToolReport::default(),
            find_r: tools::find_r,
            formatter: tools::rustfmt,
            air_formatter: tools::air_format,
            tools_root: dirs::data_dir().map(|data| data.join("oxmacs").join("tools")),
            font_note: None,
            config_path: config::default_path(),
            weather: WeatherState::default(),
            next_fetch: 0,
            irc: Irc::default(),
            paren_pair: None,
            next_job: 0,
            last_compile: None,
            lints: build::lint::Lints::default(),
            errors_from: build::errors::ErrorSource::default(),
            config: Config::default(),
        }
    }

    pub fn apply_config(&mut self, config: &Config) {
        self.tab_width = config.tab_width;
        self.on_disk_change = config.on_disk_change;
        self.settings = Settings::from(config);
        self.configured_theme = config.theme.as_deref().and_then(theme::find);
        if config.atlas_vault != self.config.atlas_vault {
            self.atlas_index = None;
        }
        self.config = config.clone();
    }

    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.appearance = appearance;
    }

    pub fn theme(&self) -> &'static Theme {
        self.previewed_theme()
            .or(self.configured_theme)
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
        motion::movement::display_column(buffer, buffer.point(), self.tab_width)
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
            self.dispatch(key, Command::KeyboardQuit.into(), false);
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
            self.dispatch(key, command.into(), true);
        } else if inserts_itself(key) {
            self.dispatch(key, Command::SelfInsertCommand.into(), false);
        } else {
            self.report_undefined();
        }
    }

    fn shifted_motion(&self, key: Key) -> Option<Command> {
        match self.lookup(&[key.unshifted()?]) {
            Lookup::Command(id) => id.core().filter(|command| command.is_motion()),
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
        if self.popup.is_some()
            && keys.len() == 1
            && let found @ Lookup::Command(_) = self.keymaps.get(Scope::Popup).lookup(keys)
        {
            return found;
        }
        let local = if self.minibuffer.is_some() {
            Some(self.keymaps.get(Scope::Minibuffer))
        } else {
            self.mode_keymap()
        };
        if let Some(keymap) = local {
            match keymap.lookup(keys) {
                Lookup::Unbound => {}
                found => return found,
            }
        }
        self.keymaps.get(Scope::Global).lookup(keys)
    }

    fn mode_keymap(&self) -> Option<&Keymap> {
        match self.buffer_scope() {
            Scope::Global => None,
            scope => Some(self.keymaps.get(scope)),
        }
    }

    fn buffer_scope(&self) -> Scope {
        match self.buffers.mode(0) {
            Some(Mode::Dired(_)) => Scope::Dired,
            Some(Mode::Splash(_)) => Scope::Splash,
            Some(Mode::Doctor(_)) => Scope::Doctor,
            Some(Mode::Compilation(_)) => Scope::Compilation,
            Some(Mode::Customize(_)) => Scope::Customize,
            Some(Mode::Diagnostics(_)) => Scope::Diagnostics,
            Some(Mode::Help(_)) => Scope::Help,
            Some(Mode::Weather(_)) => Scope::Weather,
            Some(Mode::Chat(_)) => Scope::Chat,
            Some(Mode::Plots(_)) => Scope::Plots,
            Some(Mode::Processes(_)) => Scope::Processes,
            Some(Mode::Tools(_)) => Scope::Tools,
            Some(Mode::IrcSetup(_)) => Scope::IrcSetup,
            Some(Mode::Text(None)) if self.is_atlas(0) => Scope::Atlas,
            Some(Mode::AtlasList(list)) if list.is_calendar() => Scope::AtlasCalendar,
            Some(Mode::AtlasList(_)) => Scope::AtlasList,
            Some(Mode::Environment(_)) => Scope::Environment,
            Some(Mode::Table(_)) => Scope::Table,
            Some(Mode::Text(Some(syntax))) if syntax.language() == Language::Rust => Scope::Rust,
            Some(Mode::Text(Some(syntax))) if syntax.language() == Language::R => Scope::R,
            Some(Mode::Text(Some(syntax))) if syntax.language() == Language::Markdown => {
                Scope::Markdown
            }
            Some(Mode::Preview(_)) => Scope::Preview,
            Some(Mode::Console(_)) => Scope::Console,
            Some(Mode::Text(_)) | None => Scope::Global,
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
            Some(Mode::Compilation(compilation)) => self
                .buffers
                .get(index)
                .map_or_else(LineStyle::default, |buffer| {
                    compilation.errors.line_style(line, &buffer.line_text(line))
                }),
            Some(Mode::Customize(state)) => state.line_style(line),
            Some(Mode::Diagnostics(list)) => list.line_style(line),
            Some(Mode::Help(help)) => help.line_style(line),
            Some(Mode::Weather(view)) => view.line_style(line),
            Some(Mode::Chat(view)) => view.line_style(line),
            Some(Mode::Processes(list)) => list.line_style(line),
            Some(Mode::Tools(list)) => list.line_style(line),
            Some(Mode::IrcSetup(form)) => form.line_style(line),
            Some(Mode::AtlasList(list)) => list.line_style(line),
            Some(Mode::Environment(view)) => view.line_style(line),
            Some(Mode::Table(view)) => view.line_style(line),
            Some(Mode::Preview(_) | Mode::Console(_) | Mode::Plots(_)) => LineStyle::default(),
            Some(Mode::Text(_)) | None if self.is_atlas(index) => LineStyle {
                background: None,
                spans: self.atlas_highlights(index, line),
            },
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

    fn where_is(&self, command: CommandId) -> Option<KeySequence> {
        self.mode_keymap()
            .and_then(|keymap| keymap.where_is(command))
            .or_else(|| self.keymaps.get(Scope::Global).where_is(command))
    }

    fn dispatch(&mut self, key: Key, command: CommandId, shifted: bool) {
        self.pending.clear();
        self.last_key = Some(key);
        self.run_id(command, shifted);
    }

    pub fn execute(&mut self, command: impl Into<CommandId>) {
        self.run_id(command.into(), false);
    }

    pub fn add_command(&mut self, spec: CommandSpec) -> CommandId {
        self.commands.add(spec)
    }

    pub fn bind(
        &mut self,
        scope: Scope,
        keys: &str,
        command: impl Into<CommandId>,
    ) -> Result<(), KeyParseError> {
        let keys: KeySequence = keys.parse()?;
        self.keymaps.get_mut(scope).bind(keys.keys(), command);
        Ok(())
    }

    fn run_id(&mut self, command: CommandId, shifted: bool) {
        match command.core() {
            Some(core) => self.run(core, shifted),
            None => self.run_added(command),
        }
    }

    fn run_added(&mut self, command: CommandId) {
        let Some(run) = self.commands.handler(command) else {
            return;
        };
        self.goal_column = None;
        self.last_command = None;
        run(self);
        self.refresh_minibuffer();
        self.reveal_point();
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
        self.after_atlas_command();
        self.reset_selection(command);
        self.refresh_minibuffer();
        self.refresh_popup(command);
        self.reveal_point();
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
    use super::test_support::{editor_with, lines, press, type_text};
    use super::*;

    fn say_hello(editor: &mut Editor) {
        editor.show_message("hello");
    }

    #[test]
    fn added_commands_run_from_keys_and_m_x_and_have_help() {
        let mut editor = Editor::new();
        let id = editor.add_command(CommandSpec {
            name: "say-hello",
            summary: "Say hello in the echo area",
            help: None,
            scope: Scope::Global,
            run: say_hello,
        });
        editor.bind(Scope::Global, "C-c z", id).unwrap();

        press(&mut editor, "C-c z");
        assert_eq!(editor.message(), Some("hello"));

        press(&mut editor, "M-x");
        type_text(&mut editor, "say-hello");
        let first = &editor.completions()[0];
        assert_eq!(first.label, "say-hello");
        assert_eq!(first.annotation.as_deref(), Some("C-c z"));
        assert_eq!(first.summary, Some("Say hello in the echo area"));
        press(&mut editor, "RET");
        assert_eq!(editor.message(), Some("hello"));

        press(&mut editor, "C-h f");
        type_text(&mut editor, "say-hello");
        press(&mut editor, "RET");
        let help = lines(&editor);
        assert_eq!(help[0], "say-hello");
        assert!(help.iter().any(|line| line.contains("C-c z")), "{help:?}");
    }

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
