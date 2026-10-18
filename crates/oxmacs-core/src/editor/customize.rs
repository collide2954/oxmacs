use toml::Value;

use crate::config::Config;
use crate::config::schema::{Kind, SETTINGS, Setting};
use crate::display::LineStyle;
use crate::editor::buffers::Mode;
use crate::editor::minibuffer::Purpose;
use crate::editor::page::Page;
use crate::editor::{Editor, Effect};
use crate::files::paths;
use crate::text::Buffer;
use crate::theme::Highlight;

const NAME: &str = "*Settings*";
const HELP: &str = "RET changes   C-x C-s saves   g reverts   q quits";
const MARGIN: usize = 2;
const GAP: usize = 2;

#[derive(Debug)]
pub(super) struct Customize {
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
    pub(super) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    fn row(&self, line: usize) -> Option<&Row> {
        self.rows.iter().find(|row| row.line == line)
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
    let values: Vec<String> = SETTINGS
        .iter()
        .map(|setting| shown(setting, config.get(setting.key).as_ref()))
        .collect();
    let value_width = values
        .iter()
        .map(|value| value.chars().count())
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
    for (index, (setting, value)) in SETTINGS.iter().zip(&values).enumerate() {
        if setting.group != group {
            group = setting.group;
            page.line(&[]);
            page.heading(group);
        }
        let changed = config.get(setting.key) != state.saved.get(setting.key);
        let marker = format!("{:<MARGIN$}", if changed { "*" } else { "" });
        let name = format!("{:<width$}", setting.key, width = key_width + GAP);
        let width = value.chars().count();
        let padding = " ".repeat(value_width - width + GAP);
        let start = MARGIN + key_width + GAP;
        rows.push(Row {
            line: page.styles.len(),
            setting: index,
            value: (start, start + width),
        });
        page.line(&[
            (&marker, changed.then_some(Highlight::Warning)),
            (&name, None),
            (value, Some(Highlight::Link)),
            (&padding, None),
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
        (0..self.buffers.len())
            .find(|index| matches!(self.buffers.mode(*index), Some(Mode::Customize(_))))
    }

    fn customizing(&self) -> Option<&Customize> {
        match self.buffers.mode(0)? {
            Mode::Customize(state) => Some(state),
            _ => None,
        }
    }

    pub(super) fn unsaved_settings(&self, index: usize) -> bool {
        matches!(self.buffers.mode(index), Some(Mode::Customize(state)) if state.saved != self.config)
    }

    pub(super) fn customize(&mut self) {
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
        if let Some(Mode::Customize(state)) = self.buffers.mode_mut(index) {
            state.rows = rows;
            state.styles = page.styles;
        }
        if let Some(buffer) = self.buffers.get_mut(index) {
            let point = buffer.point();
            let line = buffer.line_of(point);
            let column = point - buffer.line_start(line);
            buffer.set_text(&page.text);
            buffer.mark_saved();
            let line = line.min(buffer.line_of(buffer.len_chars()));
            buffer.set_point((buffer.line_start(line) + column).min(buffer.line_end(line)));
        }
    }

    pub(super) fn widget_forward(&mut self, forward: bool) {
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

    pub(super) fn widget_button_press(&mut self) {
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

    pub(super) fn setting_at(&self, position: usize) -> Option<usize> {
        let buffer = self.buffers.current();
        let line = buffer.line_of(position);
        let column = position - buffer.line_start(line);
        self.customizing()?
            .row(line)
            .filter(|row| (row.value.0..row.value.1).contains(&column))
            .map(|row| row.setting)
    }

    pub(super) fn change_setting(&mut self, index: usize) {
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

    pub(super) fn set_setting_from(&mut self, index: usize, input: &str) {
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

    fn set_setting(&mut self, setting: &Setting, value: Option<Value>) {
        match self.config.with(setting.key, value) {
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
    use crate::config::Config;
    use crate::config::schema::SETTINGS;
    use crate::editor::test_support::{at_point, lines, minibuffer_input, press, type_text};
    use crate::editor::{Cell, Clicks, Editor, Effect};

    fn settings() -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        editor.config_path = Some(dir.path().join("config.toml"));
        press(&mut editor, "M-x");
        type_text(&mut editor, "customize");
        press(&mut editor, "RET");
        (dir, editor)
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
        let theme = row(&editor, "theme");
        assert!(theme.contains("[follow OS]"), "{theme}");
        assert!(
            theme.ends_with("follow OS | adwaita | adwaita-dark"),
            "{theme}"
        );
        assert!(row(&editor, "animations").contains("[x]"));
        assert!(row(&editor, "tab-width").contains(" 8 "));
        assert!(row(&editor, "font-size").contains(" 11 "));
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
        assert!(at_point(&editor).starts_with("[x]"));
        assert_eq!(
            editor.buffer().line_of(editor.buffer().point()),
            line_of(&editor, "auto-save")
        );

        press(&mut editor, "TAB S-TAB");
        assert_eq!(
            editor.buffer().line_of(editor.buffer().point()),
            line_of(&editor, "auto-save")
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
