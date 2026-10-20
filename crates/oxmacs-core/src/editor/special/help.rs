use crate::command::CommandId;
use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::editor::prompts::minibuffer::Purpose;
use crate::text::Buffer;
use crate::theme::Highlight;

const NAME: &str = "*Help*";
const KEYS: &str = "Keys      ";
const WORKS_IN: &str = "Works in  ";
const INDENT: &str = "          ";
const FILL_COLUMN: usize = 70;

#[derive(Debug, Default)]
pub(in crate::editor) struct Help {
    command: Option<CommandId>,
    width: usize,
    styles: Vec<LineStyle>,
    r: bool,
}

impl Help {
    pub(in crate::editor) fn r(styles: Vec<LineStyle>) -> Self {
        Self {
            styles,
            r: true,
            ..Self::default()
        }
    }

    pub(in crate::editor) fn is_r(&self) -> bool {
        self.r
    }

    pub(in crate::editor) fn set_styles(&mut self, styles: Vec<LineStyle>) {
        self.styles = styles;
    }

    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }
}

fn listed(items: &[String], last: &str) -> String {
    match items.split_last() {
        Some((final_item, rest)) if !rest.is_empty() => {
            format!("{} {last} {final_item}", rest.join(", "))
        }
        _ => items.concat(),
    }
}

struct Described {
    name: &'static str,
    summary: &'static str,
    help: Option<&'static str>,
    place: Option<&'static str>,
}

type Places = [(Option<&'static str>, Vec<String>)];

fn key_lines(name: &str, places: &Places) -> Vec<String> {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for (label, keys) in places {
        let keys = listed(keys, "or");
        let place = label.unwrap_or_default().to_owned();
        match groups.iter_mut().find(|(shown, _)| *shown == keys) {
            Some((_, scopes)) => scopes.push(place),
            None => groups.push((keys, vec![place])),
        }
    }
    if groups.is_empty() {
        return vec![format!("none; run it with M-x {name}")];
    }
    groups
        .into_iter()
        .map(|(keys, scopes)| {
            let scopes: Vec<String> = scopes
                .into_iter()
                .filter(|scope| !scope.is_empty())
                .collect();
            if scopes.is_empty() {
                keys
            } else {
                format!("{keys} in {}", listed(&scopes, "and"))
            }
        })
        .collect()
}

fn page(command: &Described, places: &Places, width: usize) -> Page {
    let mut page = Page::default();
    page.heading(command.name);
    page.line(&[(&format!("{}.", command.summary), None)]);
    if let Some(help) = command.help {
        page.line(&[]);
        page.paragraph(help, width);
    }
    page.line(&[]);
    for (index, keys) in key_lines(command.name, places).iter().enumerate() {
        let label = if index == 0 { KEYS } else { INDENT };
        page.line(&[(label, Some(Highlight::Comment)), (keys, None)]);
    }
    if let Some(place) = command.place {
        page.line(&[(WORKS_IN, Some(Highlight::Comment)), (place, None)]);
    }
    page
}

impl Editor {
    pub(in crate::editor) fn describe_command(&mut self) {
        self.open_minibuffer("Describe command: ", Purpose::DescribeCommand);
    }

    fn help_index(&self) -> Option<usize> {
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::Help(help) if !help.r))
    }

    pub(in crate::editor) fn describe(&mut self, command: CommandId) {
        if self.help_index().is_none() {
            self.beside(|editor| {
                let mut buffer = Buffer::with_text(NAME, "");
                buffer.set_read_only(true);
                editor.add_buffer_with(buffer, Mode::Help(Help::default()));
            });
        }
        if let Some(index) = self.help_index()
            && let Some(Mode::Help(help)) = self.buffers.mode_mut(index)
        {
            help.command = Some(command);
            help.width = 0;
        }
        self.show_beside(Self::help_index);
        self.render_help();
        self.buffers.current_mut().set_point(0);
        self.reveal_point();
    }

    pub(in crate::editor) fn render_help(&mut self) {
        let Some(index) = self.help_index() else {
            return;
        };
        let Some(Mode::Help(help)) = self.buffers.mode(index) else {
            return;
        };
        let Some(command) = help.command else {
            return;
        };
        let width = self
            .buffers
            .get(index)
            .and_then(|buffer| self.width_showing(buffer.id()))
            .filter(|width| *width > 0)
            .map_or(FILL_COLUMN, |width| width.min(FILL_COLUMN));
        if width == help.width {
            return;
        }
        let places: Vec<(Option<&'static str>, Vec<String>)> = self
            .keymaps
            .all()
            .map(|(scope, keymap)| {
                let keys = keymap
                    .bindings(command)
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                (self.keymaps.label(scope), keys)
            })
            .filter(|(_, keys)| !keys.is_empty())
            .collect();
        let described = Described {
            name: self.commands.name(command),
            summary: self.commands.summary(command),
            help: self.commands.help(command),
            place: self.keymaps.label(self.commands.scope(command)),
        };
        let page = page(&described, &places, width);
        if let Some(buffer) = self.buffers.get_mut(index) {
            page.fill(buffer);
        }
        if let Some(Mode::Help(help)) = self.buffers.mode_mut(index) {
            help.width = width;
            help.styles = page.styles;
        }
    }
}
#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::{labels, lines, press, type_text};

    fn describe(editor: &mut Editor, name: &str) {
        press(editor, "C-h f");
        type_text(editor, name);
        press(editor, "RET");
    }

    #[test]
    fn describing_a_command_shows_its_summary_help_and_keys() {
        let mut editor = Editor::new();

        describe(&mut editor, "find-file");

        assert_eq!(editor.buffer().name(), "*Help*");
        assert!(editor.buffer().is_read_only());
        let lines = lines(&editor);
        assert_eq!(lines[0], "find-file");
        assert_eq!(lines[1], "Open a file, creating it if it is new.");
        assert!(lines[3].starts_with("Type a path and the list narrows"));
        assert!(lines.contains(&"Keys      C-x C-f".to_owned()), "{lines:?}");
    }

    #[test]
    fn help_text_fills_the_window_it_is_shown_in() {
        let mut editor = Editor::new();
        describe(&mut editor, "find-file");
        assert!(lines(&editor).iter().all(|line| line.chars().count() <= 70));

        editor.resize_pane(editor.selected_window(), 20, 41);

        let shown = lines(&editor);
        assert!(
            shown.iter().all(|line| line.chars().count() <= 40),
            "{shown:?}"
        );
        assert!(shown[3].starts_with("Type a path and the list narrows"));
    }

    #[test]
    fn help_opens_beside_the_current_window() {
        let mut editor = Editor::new();

        describe(&mut editor, "find-file");

        assert_eq!(editor.tiling().windows().len(), 2);
        press(&mut editor, "C-x o");
        assert_eq!(editor.buffer().name(), "*scratch*");
    }

    #[test]
    fn keys_are_grouped_by_where_they_work() {
        let mut editor = Editor::new();

        describe(&mut editor, "scroll-up-command");
        let shown = lines(&editor);
        assert!(
            shown.contains(&"Keys      C-v or <next>".to_owned()),
            "{shown:?}"
        );
        assert!(
            shown.contains(&"          SPC in *Help* and R tables".to_owned()),
            "{shown:?}"
        );

        describe(&mut editor, "forward-button");
        let shown = lines(&editor);
        assert!(
            shown.contains(&"Keys      n, TAB, C-n or <down> in the start screen".to_owned()),
            "{shown:?}"
        );
    }

    #[test]
    fn unbound_commands_say_how_to_run_them() {
        let mut editor = Editor::new();

        describe(&mut editor, "kill-oxmacs");

        assert!(lines(&editor).contains(&"Keys      none; run it with M-x kill-oxmacs".to_owned()));
    }

    #[test]
    fn mode_commands_say_where_they_work() {
        let mut editor = Editor::new();

        describe(&mut editor, "rust-format-buffer");

        assert!(lines(&editor).contains(&"Works in  Rust buffers".to_owned()));
    }

    #[test]
    fn every_command_can_be_described_from_anywhere() {
        let mut editor = Editor::new();

        press(&mut editor, "C-h f");
        type_text(&mut editor, "rust-format");

        assert_eq!(labels(&editor), ["rust-format-buffer"]);
        assert_eq!(
            editor.completions()[0].summary,
            Some("Format the buffer with rustfmt")
        );
    }

    #[test]
    fn describing_again_reuses_the_help_buffer() {
        let mut editor = Editor::new();
        describe(&mut editor, "find-file");

        describe(&mut editor, "save-buffer");

        assert_eq!(lines(&editor)[0], "save-buffer");
        let helps = editor
            .buffer_names()
            .into_iter()
            .filter(|name| *name == "*Help*")
            .count();
        assert_eq!(helps, 1);
        assert_eq!(editor.tiling().windows().len(), 2);
    }
}
