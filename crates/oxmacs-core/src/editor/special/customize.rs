use std::fs;
use std::path::PathBuf;

use toml::Value;

use crate::config::schema::{Kind, SETTINGS, Setting};
use crate::config::{self, Config};
use crate::display::LineStyle;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::editor::prompts::minibuffer::Purpose;
use crate::editor::{Editor, Effect};
use crate::files::{self, paths};
use crate::text::Buffer;
use crate::theme::Highlight;

const NAME: &str = "*Settings*";
const HELP: &str = "RET change  C-x C-s save  g revert  q quit";
const MARGIN: usize = 2;
const GAP: usize = 2;
const HINT_INDENT: &str = "    ";

#[derive(Debug)]
pub(in crate::editor) struct Customize {
    saved: Config,
    problem: Option<String>,
    rows: Vec<Row>,
    styles: Vec<LineStyle>,
}

#[derive(Debug, Clone, Copy)]
struct Row {
    line: usize,
    setting: usize,
    value: (usize, usize),
}

impl Customize {
    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    fn row(&self, line: usize) -> Option<&Row> {
        self.rows
            .iter()
            .find(|row| row.line == line || row.line + 1 == line)
    }
}

fn shown(setting: &Setting, value: Option<&Value>) -> String {
    let unset = setting.unset.unwrap_or_default();
    match value {
        None if matches!(setting.kind, Kind::Choice(_)) => format!("[{unset}]"),
        None => format!("({unset})"),
        Some(Value::Boolean(true)) => "[x]".to_owned(),
        Some(Value::Boolean(false)) => "[ ]".to_owned(),
        Some(Value::String(text)) if matches!(setting.kind, Kind::Choice(_)) => {
            format!("[{text}]")
        }
        Some(Value::String(text)) => text.clone(),
        Some(Value::Float(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

fn hint(setting: &Setting) -> String {
    match setting.kind {
        Kind::Choice(values) => setting
            .unset
            .into_iter()
            .chain(values.iter().copied())
            .collect::<Vec<_>>()
            .join(" | "),
        Kind::Toggle | Kind::Integer | Kind::Number | Kind::Text => setting.hint.to_owned(),
    }
}

fn editable_text(value: Option<Value>) -> String {
    match value {
        Some(Value::String(text)) => text,
        Some(Value::Float(number)) => number.to_string(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn page(config: &Config, state: &Customize, location: &str) -> (Page, Vec<Row>) {
    let key_width = SETTINGS
        .iter()
        .map(|setting| setting.key.len())
        .max()
        .unwrap_or(0);
    let mut page = Page::default();
    page.line(&[
        ("Settings", Some(Highlight::Heading)),
        (&format!("  {location}"), None),
    ]);
    page.line(&[(HELP, Some(Highlight::Comment))]);
    if let Some(problem) = &state.problem {
        page.line(&[(problem, Some(Highlight::Error))]);
    }
    let mut rows = Vec::new();
    let mut group = "";
    for (index, setting) in SETTINGS.iter().enumerate() {
        if setting.group != group {
            group = setting.group;
            page.line(&[]);
            page.heading(group);
        }
        let value = shown(setting, config.get(setting.key).as_ref());
        let changed = config.get(setting.key) != state.saved.get(setting.key);
        let marker = format!("{:<MARGIN$}", if changed { "*" } else { "" });
        let name = format!("{:<width$}", setting.key, width = key_width + GAP);
        let start = MARGIN + key_width + GAP;
        rows.push(Row {
            line: page.styles.len(),
            setting: index,
            value: (start, start + value.chars().count()),
        });
        page.line(&[
            (&marker, changed.then_some(Highlight::Warning)),
            (&name, None),
            (&value, Some(Highlight::Link)),
        ]);
        page.line(&[
            (HINT_INDENT, None),
            (&hint(setting), Some(Highlight::Comment)),
        ]);
    }
    (page, rows)
}

impl Editor {
    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn reconfigure(&mut self, config: &Config) {
        self.apply_config(config);
        self.effects.push(Effect::Reconfigure);
    }

    fn settings_index(&self) -> Option<usize> {
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::Customize(_)))
    }

    pub(in crate::editor) fn customizing(&self) -> Option<&Customize> {
        match self.buffers.mode(0)? {
            Mode::Customize(state) => Some(state),
            _ => None,
        }
    }

    pub(in crate::editor) fn unsaved_settings(&self, index: usize) -> bool {
        matches!(self.buffers.mode(index), Some(Mode::Customize(state)) if state.saved != self.config)
    }

    pub(in crate::editor) fn customize(&mut self) {
        if let Some(index) = self.settings_index() {
            self.select_buffer(index);
            return;
        }
        let (saved, problem) = self.saved_config();
        let mut buffer = Buffer::with_text(NAME, "");
        buffer.set_read_only(true);
        self.add_buffer_with(
            buffer,
            Mode::Customize(Customize {
                saved,
                problem,
                rows: Vec::new(),
                styles: Vec::new(),
            }),
        );
        self.render_settings();
        self.widget_forward(true);
    }

    fn saved_config(&self) -> (Config, Option<String>) {
        match &self.config_path {
            Some(path) => match Config::load(path) {
                Ok(config) => (config, None),
                Err(error) => (
                    self.config.clone(),
                    Some(format!("{error}; fix it before saving")),
                ),
            },
            None => (
                self.config.clone(),
                Some("No config directory, so settings cannot be saved".to_owned()),
            ),
        }
    }

    fn render_settings(&mut self) {
        let Some(index) = self.settings_index() else {
            return;
        };
        let location = self.config_path.as_deref().map_or_else(
            || "no config directory".to_owned(),
            |path| paths::abbreviate(path, self.home.as_deref()),
        );
        let Some(Mode::Customize(state)) = self.buffers.mode(index) else {
            return;
        };
        let (page, rows) = page(&self.config, state, &location);
        if let Some(buffer) = self.buffers.get_mut(index) {
            page.fill(buffer);
        }
        if let Some(Mode::Customize(state)) = self.buffers.mode_mut(index) {
            state.rows = rows;
            state.styles = page.styles;
        }
    }

    pub(in crate::editor) fn widget_forward(&mut self, forward: bool) {
        let line = self.point_line();
        let Some(state) = self.customizing() else {
            return;
        };
        let target = if forward {
            state
                .rows
                .iter()
                .find(|row| row.line > line)
                .or(state.rows.first())
        } else {
            state
                .rows
                .iter()
                .rev()
                .find(|row| row.line < line)
                .or(state.rows.last())
        };
        let Some(&Row {
            line,
            setting,
            value,
        }) = target
        else {
            return;
        };
        let buffer = self.buffers.current_mut();
        buffer.set_point(buffer.line_start(line) + value.0);
        if let Some(setting) = SETTINGS.get(setting) {
            self.show_message(setting.hint);
        }
    }

    pub(in crate::editor) fn widget_button_press(&mut self) {
        let line = self.point_line();
        match self
            .customizing()
            .and_then(|state| state.row(line))
            .map(|row| row.setting)
        {
            Some(setting) => self.change_setting(setting),
            None => self.show_message("No setting here"),
        }
    }

    pub(in crate::editor) fn setting_at(&self, position: usize) -> Option<usize> {
        let buffer = self.buffers.current();
        let line = buffer.line_of(position);
        let column = position - buffer.line_start(line);
        self.customizing()?
            .row(line)
            .filter(|row| (row.value.0..row.value.1).contains(&column))
            .map(|row| row.setting)
    }

    pub(in crate::editor) fn change_setting(&mut self, index: usize) {
        let Some(setting) = SETTINGS.get(index) else {
            return;
        };
        let current = self.config.get(setting.key);
        match setting.kind {
            Kind::Toggle => {
                let on = matches!(current, Some(Value::Boolean(true)));
                self.set_setting(setting, Some(Value::Boolean(!on)));
            }
            Kind::Choice(values) => {
                let options: Vec<Option<&str>> = setting
                    .unset
                    .map(|_| None)
                    .into_iter()
                    .chain(values.iter().map(|value| Some(*value)))
                    .collect();
                let now = current.as_ref().and_then(Value::as_str);
                let at = options
                    .iter()
                    .position(|option| *option == now)
                    .unwrap_or(0);
                let next = options[(at + 1) % options.len()];
                self.set_setting(setting, next.map(|value| Value::String(value.to_owned())));
            }
            Kind::Integer | Kind::Number | Kind::Text => {
                if self.open_minibuffer(&format!("{}: ", setting.key), Purpose::Setting(index)) {
                    self.set_minibuffer_input(&editable_text(current));
                }
            }
        }
    }

    pub(in crate::editor) fn set_setting_from(&mut self, index: usize, input: &str) {
        let Some(setting) = SETTINGS.get(index) else {
            return;
        };
        let value = match setting.kind {
            Kind::Integer => input
                .trim()
                .parse()
                .map(|number| Some(Value::Integer(number)))
                .map_err(|_| "a whole number"),
            Kind::Number => input
                .trim()
                .parse()
                .map(|number| Some(Value::Float(number)))
                .map_err(|_| "a number"),
            _ if input.is_empty() && setting.unset.is_some() => Ok(None),
            Kind::Toggle | Kind::Choice(_) | Kind::Text => {
                Ok(Some(Value::String(input.to_owned())))
            }
        };
        match value {
            Ok(value) => self.set_setting(setting, value),
            Err(expected) => self.show_message(format!("{} needs {expected}", setting.key)),
        }
    }

    pub(in crate::editor) fn unsaved_config_path(&self) -> Option<PathBuf> {
        self.settings_index()
            .filter(|index| self.unsaved_settings(*index))
            .and(self.config_path.clone())
    }

    pub(in crate::editor) fn save_settings(&mut self) -> bool {
        let Some(path) = self.unsaved_config_path() else {
            self.show_message("(No changes need to be saved)");
            return true;
        };
        let shown = paths::abbreviate(&path, self.home.as_deref());
        let written = files::load(&path)
            .map_err(|error| error.to_string())
            .and_then(|loaded| {
                let text = config::rewrite(&loaded.text, &self.config)
                    .map_err(|error| format!("{shown}: {error}"))?;
                if let Some(directory) = path.parent() {
                    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
                }
                files::save(&path, &text, loaded.format).map_err(|error| error.to_string())
            });
        if let Err(error) = written {
            self.show_message(error);
            return false;
        }
        if let Some(index) = self.settings_index()
            && let Some(Mode::Customize(state)) = self.buffers.mode_mut(index)
        {
            state.saved = self.config.clone();
            state.problem = None;
        }
        self.render_settings();
        self.show_message(format!("Wrote {shown}"));
        true
    }

    pub(in crate::editor) fn revert_settings(&mut self) {
        let (saved, problem) = self.saved_config();
        if problem.is_none() {
            self.reconfigure(&saved);
        }
        if let Some(index) = self.settings_index()
            && let Some(Mode::Customize(state)) = self.buffers.mode_mut(index)
        {
            state.saved = saved;
            state.problem = problem;
        }
        self.render_settings();
    }

    fn set_setting(&mut self, setting: &Setting, value: Option<Value>) {
        self.change_config(setting.key, value);
    }

    pub(in crate::editor) fn change_config(&mut self, key: &str, value: Option<Value>) {
        match self.config.with(key, value) {
            Ok(config) => {
                self.reconfigure(&config);
                self.render_settings();
            }
            Err(error) => self.show_message(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::command::Command;
    use crate::config::Config;
    use crate::config::schema::SETTINGS;
    use crate::editor::test_support::{at_point, lines, minibuffer_input, press, type_text};
    use crate::editor::{Cell, Clicks, Editor, Effect};

    fn settings() -> (tempfile::TempDir, Editor) {
        settings_from(None)
    }

    fn settings_from(text: Option<&str>) -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut editor = Editor::new();
        if let Some(text) = text {
            fs::write(&path, text).unwrap();
            if let Ok(config) = Config::parse(text) {
                editor.apply_config(&config);
            }
        }
        editor.config_path = Some(path);
        press(&mut editor, "M-x");
        type_text(&mut editor, "customize");
        press(&mut editor, "RET");
        (dir, editor)
    }

    fn config_text(dir: &tempfile::TempDir) -> String {
        fs::read_to_string(dir.path().join("config.toml")).unwrap()
    }

    #[test]
    fn saving_writes_only_the_changes_to_config_toml() {
        let (dir, mut editor) = settings_from(Some("# mine\ntab-width = 4\n"));
        on(&mut editor, "clock");
        press(&mut editor, "RET");

        press(&mut editor, "C-x C-s");

        assert_eq!(config_text(&dir), "# mine\ntab-width = 4\nclock = false\n");
        let message = editor.message().unwrap();
        assert!(
            message.starts_with("Wrote ") && message.ends_with("config.toml"),
            "{message}"
        );
        assert!(editor.mode_line().starts_with("%% *Settings*"));
        assert!(row(&editor, "clock").starts_with("  clock"));
    }

    #[test]
    fn saving_creates_the_config_directory() {
        let (dir, mut editor) = settings();
        let path = dir.path().join("oxmacs").join("config.toml");
        editor.config_path = Some(path.clone());
        on(&mut editor, "splash");
        press(&mut editor, "RET C-x C-s");

        assert_eq!(fs::read_to_string(path).unwrap(), "splash = false\n");
    }

    #[test]
    fn saving_without_changes_says_so() {
        let (_dir, mut editor) = settings();

        press(&mut editor, "C-x C-s");

        assert_eq!(editor.message(), Some("(No changes need to be saved)"));
    }

    #[test]
    fn reverting_returns_to_the_saved_settings() {
        let (_dir, mut editor) = settings_from(Some("tab-width = 4\n"));
        on(&mut editor, "clock");
        press(&mut editor, "RET");

        press(&mut editor, "g");

        assert!(editor.config().clock);
        assert_eq!(editor.tab_width(), 4);
        assert!(editor.mode_line().starts_with("%% *Settings*"));
    }

    #[test]
    fn broken_config_files_are_reported_and_left_alone() {
        let (dir, mut editor) = settings_from(Some("tab-width = \n"));
        assert!(lines(&editor)[2].ends_with("; fix it before saving"));
        on(&mut editor, "clock");
        press(&mut editor, "RET");

        press(&mut editor, "C-x C-s");

        assert_eq!(config_text(&dir), "tab-width = \n");
        assert!(editor.mode_line().starts_with("%* *Settings*"));
    }

    #[test]
    fn settings_changed_elsewhere_show_as_unsaved() {
        let (dir, mut editor) = settings();
        press(&mut editor, "q");

        editor.execute(Command::ToggleTheme);

        assert_eq!(editor.config().theme.as_deref(), Some("adwaita-dark"));
        press(&mut editor, "M-x");
        type_text(&mut editor, "customize");
        press(&mut editor, "RET");
        assert!(row(&editor, "theme").starts_with("* theme"));
        assert!(row(&editor, "theme").contains("[adwaita-dark]"));
        assert!(editor.mode_line().starts_with("%* *Settings*"));
        assert!(!dir.path().join("config.toml").exists());
    }

    #[test]
    fn changes_made_before_customizing_are_unsaved_too() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        editor.config_path = Some(dir.path().join("config.toml"));
        press(&mut editor, "M-x");
        type_text(&mut editor, "load-theme");
        press(&mut editor, "RET");
        type_text(&mut editor, "adwaita");
        press(&mut editor, "M-RET");

        press(&mut editor, "M-x");
        type_text(&mut editor, "customize");
        press(&mut editor, "RET");

        assert!(row(&editor, "theme").contains("[adwaita]"));
        assert!(editor.mode_line().starts_with("%* *Settings*"));
    }

    #[test]
    fn quitting_offers_to_save_unsaved_settings() {
        let (dir, mut editor) = settings();
        on(&mut editor, "clock");
        press(&mut editor, "RET");

        press(&mut editor, "C-x C-c");
        let prompt = editor.question().unwrap().prompt().to_owned();
        assert!(prompt.starts_with("Save file "), "{prompt}");
        press(&mut editor, "y");

        assert_eq!(config_text(&dir), "clock = false\n");
        assert!(editor.should_quit());
    }

    fn line_of(editor: &Editor, key: &str) -> usize {
        lines(editor)
            .iter()
            .position(|line| {
                line.get(2..)
                    .is_some_and(|rest| rest.starts_with(&format!("{key} ")))
            })
            .unwrap_or_else(|| panic!("{key} missing"))
    }

    fn on(editor: &mut Editor, key: &str) {
        let line = line_of(editor, key);
        let buffer = editor.buffers.current_mut();
        buffer.set_point(buffer.line_start(line));
    }

    fn row(editor: &Editor, key: &str) -> String {
        lines(editor)[line_of(editor, key)].clone()
    }

    #[test]
    fn customize_lists_every_setting_by_group() {
        let (_dir, editor) = settings();

        assert_eq!(editor.buffer().name(), "*Settings*");
        assert!(editor.buffer().is_read_only());
        let lines = lines(&editor);
        assert!(lines[0].starts_with("Settings  "), "{}", lines[0]);
        for group in ["Appearance", "Editing", "Files", "Rust"] {
            assert!(lines.iter().any(|line| line == group), "{group}");
        }
        for setting in SETTINGS {
            line_of(&editor, setting.key);
        }
        assert!(row(&editor, "theme").ends_with(" [follow OS]"));
        assert_eq!(
            lines[line_of(&editor, "theme") + 1],
            "    follow OS | adwaita | adwaita-dark"
        );
        assert_eq!(
            lines[line_of(&editor, "tab-width") + 1],
            "    Columns per tab character"
        );
        assert!(row(&editor, "animations").ends_with(" [x]"));
        assert!(row(&editor, "tab-width").ends_with(" 8"));
        assert!(row(&editor, "font-size").ends_with(" 11"));
    }

    #[test]
    fn n_and_p_move_between_settings_and_explain_them() {
        let (_dir, mut editor) = settings();
        assert!(at_point(&editor).starts_with("[follow OS]"));

        press(&mut editor, "n");
        assert!(at_point(&editor).starts_with("(bundled)"));
        assert_eq!(
            editor.message(),
            Some("Monospaced font name; leave empty for the bundled JetBrains Mono")
        );

        press(&mut editor, "p p");
        assert!(at_point(&editor).starts_with("(auto-detect)"));
        assert_eq!(
            editor.buffer().line_of(editor.buffer().point()),
            line_of(&editor, "r-program")
        );

        press(&mut editor, "TAB S-TAB");
        assert_eq!(
            editor.buffer().line_of(editor.buffer().point()),
            line_of(&editor, "r-program")
        );
    }

    #[test]
    fn return_toggles_switches_and_cycles_choices() {
        let (_dir, mut editor) = settings();
        editor.take_effects();

        on(&mut editor, "animations");
        press(&mut editor, "RET");
        assert!(!editor.config().animations);
        assert!(row(&editor, "animations").starts_with("* animations"));
        assert!(row(&editor, "animations").contains("[ ]"));
        assert_eq!(editor.take_effects(), [Effect::Reconfigure]);

        on(&mut editor, "theme");
        for expected in [Some("adwaita"), Some("adwaita-dark"), None] {
            press(&mut editor, "RET");
            assert_eq!(editor.config().theme.as_deref(), expected);
        }
        assert!(row(&editor, "theme").starts_with("  theme"));
    }

    #[test]
    fn numbers_are_read_and_checked_in_the_minibuffer() {
        let (_dir, mut editor) = settings();
        on(&mut editor, "tab-width");

        press(&mut editor, "RET");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "tab-width: ");
        assert_eq!(minibuffer_input(&editor), "8");
        press(&mut editor, "C-a C-k 4 RET");
        assert_eq!(editor.tab_width(), 4);

        press(&mut editor, "RET C-a C-k 0 RET");
        assert_eq!(editor.message(), Some("tab-width must be at least 1"));
        assert_eq!(editor.tab_width(), 4);

        press(&mut editor, "RET C-a C-k");
        type_text(&mut editor, "wide");
        press(&mut editor, "RET");
        assert_eq!(editor.message(), Some("tab-width needs a whole number"));
    }

    #[test]
    fn text_settings_can_be_set_and_unset() {
        let (_dir, mut editor) = settings();
        on(&mut editor, "font-family");

        press(&mut editor, "RET");
        assert_eq!(minibuffer_input(&editor), "");
        type_text(&mut editor, "Iosevka");
        press(&mut editor, "RET");
        assert_eq!(editor.config().font_family.as_deref(), Some("Iosevka"));
        assert!(row(&editor, "font-family").contains("Iosevka"));

        press(&mut editor, "RET C-a C-k RET");
        assert_eq!(editor.config().font_family, None);
        assert!(row(&editor, "font-family").contains("(bundled)"));
    }

    #[test]
    fn return_on_a_hint_changes_the_setting_above_it() {
        let (_dir, mut editor) = settings();
        on(&mut editor, "clock");

        press(&mut editor, "C-n RET");

        assert!(!editor.config().clock);
    }

    #[test]
    fn unsaved_changes_show_in_the_mode_line() {
        let (_dir, mut editor) = settings();
        assert!(editor.mode_line().starts_with("%% *Settings*"));

        on(&mut editor, "clock");
        press(&mut editor, "RET");

        assert!(editor.mode_line().starts_with("%* *Settings*"));
    }

    #[test]
    fn clicking_a_value_changes_it() {
        let (_dir, mut editor) = settings();
        let line = line_of(&editor, "splash");
        let column = row(&editor, "splash").find('[').unwrap();

        editor.click(
            Cell {
                row: line,
                column: column + 1,
            },
            Clicks::Single,
            false,
        );

        assert!(!editor.config().splash);
    }

    #[test]
    fn reconfiguring_applies_at_once_and_tells_the_ui() {
        let mut editor = Editor::new();

        editor.reconfigure(&Config::parse("tab-width = 4").unwrap());

        assert_eq!(editor.tab_width(), 4);
        assert_eq!(editor.config().tab_width, 4);
        assert_eq!(editor.take_effects(), [Effect::Reconfigure]);
    }
}
