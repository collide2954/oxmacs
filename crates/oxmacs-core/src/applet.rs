use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

use crate::command::ScopeId;
use crate::display::LineStyle;
use crate::editor::{Editor, InputLine};
use crate::theme::Theme;

pub use crate::editor::Offer;

pub trait AppletBuffer: Any + Send + fmt::Debug {
    fn scope(&self) -> ScopeId;

    fn line_style(&self, _line: usize) -> LineStyle {
        LineStyle::default()
    }

    fn input_line(&mut self) -> Option<&mut InputLine> {
        None
    }

    fn directory(&self) -> Option<&Path> {
        None
    }
}

pub trait Applet {
    fn name(&self) -> &'static str;

    fn version(&self) -> &'static str;

    fn opt_in(&self) -> bool {
        false
    }

    fn themes(&self) -> &'static [Theme] {
        &[]
    }

    fn subcommands(&self) -> &'static [Subcommand] {
        &[]
    }

    fn install(&self, editor: &mut Editor);
}

#[derive(Debug, Clone, Copy)]
pub struct Subcommand {
    pub name: &'static str,
    pub summary: &'static str,
    pub run: fn() -> Result<(), String>,
}

pub fn subcommand<'a>(
    applets: impl IntoIterator<Item = &'a dyn Applet>,
    name: &str,
) -> Option<&'static Subcommand> {
    applets
        .into_iter()
        .flat_map(Applet::subcommands)
        .find(|subcommand| subcommand.name == name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finding {
    Info,
    Good,
    Warning,
    Problem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorLine {
    pub label: String,
    pub finding: Finding,
    pub text: String,
}

#[derive(Clone)]
pub struct Payload(Arc<dyn Any + Send + Sync>);

impl Payload {
    pub fn new<T: Any + Send + Sync>(value: T) -> Self {
        Self(Arc::new(value))
    }

    pub fn get<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }

    pub fn kind(&self) -> TypeId {
        Any::type_id(&*self.0)
    }
}

impl PartialEq for Payload {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Payload {}

impl fmt::Debug for Payload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Payload")
    }
}

type Handler = Arc<dyn Fn(&mut Editor, &Payload) + Send + Sync>;

#[derive(Default)]
pub struct Handlers(HashMap<TypeId, Handler>);

impl fmt::Debug for Handlers {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Handlers")
            .field("count", &self.0.len())
            .finish()
    }
}

impl Handlers {
    pub fn add<T: Any + Send + Sync>(&mut self, handler: fn(&mut Editor, &T)) {
        let handler: Handler = Arc::new(move |editor: &mut Editor, payload: &Payload| {
            if let Some(event) = payload.get::<T>() {
                handler(editor, event);
            }
        });
        self.0.insert(TypeId::of::<T>(), handler);
    }

    pub fn find(&self, payload: &Payload) -> Option<Handler> {
        self.0.get(&payload.kind()).cloned()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Installed {
    pub name: &'static str,
    pub version: &'static str,
    pub opt_in: bool,
}

#[derive(Default)]
pub struct States(HashMap<TypeId, Box<dyn Any + Send>>);

impl fmt::Debug for States {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("States")
            .field("count", &self.0.len())
            .finish()
    }
}

impl States {
    pub fn get<T: Any + Send>(&self) -> Option<&T> {
        self.0.get(&TypeId::of::<T>())?.downcast_ref()
    }

    pub fn get_mut<T: Any + Send>(&mut self) -> Option<&mut T> {
        self.0.get_mut(&TypeId::of::<T>())?.downcast_mut()
    }

    pub fn set<T: Any + Send>(&mut self, state: T) {
        self.0.insert(TypeId::of::<T>(), Box::new(state));
    }

    pub fn take<T: Any + Send>(&mut self) -> Option<T> {
        self.0
            .remove(&TypeId::of::<T>())?
            .downcast()
            .ok()
            .map(|state| *state)
    }
}

#[cfg(test)]
mod tests {
    use super::Payload;
    use crate::command::{CommandSpec, Scope, ScopeId};
    use crate::config::Config;
    use crate::display::LineStyle;
    use crate::editor::Page;
    use crate::editor::{Editor, InputLine, Question};
    use crate::input::KeySequence;
    use crate::theme::{self, ADWAITA, ADWAITA_DARK, Highlight};

    use super::*;

    #[derive(Debug, Default)]
    struct Count(usize);

    fn count(editor: &mut Editor) {
        if let Some(count) = editor.applet_state_mut::<Count>() {
            count.0 += 1;
        }
    }

    struct Counter;

    impl Applet for Counter {
        fn name(&self) -> &'static str {
            "counter"
        }

        fn version(&self) -> &'static str {
            "0.2.0"
        }

        fn opt_in(&self) -> bool {
            true
        }

        fn install(&self, editor: &mut Editor) {
            let command = editor.add_command(CommandSpec {
                name: "count-up",
                summary: "Count one more",
                help: None,
                scope: Scope::Global.into(),
                run: count,
            });
            editor.set_applet_state(Count::default());
            editor.bind(Scope::Global, "C-c +", command).ok();
        }
    }

    #[test]
    fn installed_applets_add_commands_keys_and_state() {
        let mut editor = Editor::new();

        editor.install(&Counter);
        let keys: KeySequence = "C-c + C-c +".parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }

        assert_eq!(editor.applet_state::<Count>().map(|count| count.0), Some(2));
        assert_eq!(
            editor.installed(),
            [Installed {
                name: "counter",
                version: "0.2.0",
                opt_in: true
            }]
        );
    }

    #[derive(Debug)]
    struct Greeter {
        scope: ScopeId,
        count: usize,
        styles: Vec<LineStyle>,
    }

    impl AppletBuffer for Greeter {
        fn scope(&self) -> ScopeId {
            self.scope
        }

        fn line_style(&self, line: usize) -> LineStyle {
            self.styles.get(line).cloned().unwrap_or_default()
        }
    }

    fn greet_open(editor: &mut Editor) {
        let Some(scope) = editor.applet_state::<GreeterScope>().map(|known| known.0) else {
            return;
        };
        let id = editor.open_applet_buffer(
            "*Greeter*",
            Greeter {
                scope,
                count: 0,
                styles: Vec::new(),
            },
        );
        render_greeter(editor, id);
    }

    fn greet_more(editor: &mut Editor) {
        let Some(id) = editor.find_applet_buffer::<Greeter>(|_| true) else {
            return;
        };
        if let Some((_, greeter)) = editor.applet_buffer_mut::<Greeter>(id) {
            greeter.count += 1;
        }
        render_greeter(editor, id);
    }

    fn render_greeter(editor: &mut Editor, id: crate::text::BufferId) {
        let Some((_, greeter)) = editor.applet_buffer_mut::<Greeter>(id) else {
            return;
        };
        let mut page = Page::default();
        page.heading(&format!("Hello {} times", greeter.count));
        let (text, styles) = page.into_parts();
        greeter.styles = styles;
        editor.fill_buffer(id, &text);
    }

    #[derive(Debug)]
    struct GreeterScope(ScopeId);

    struct Greetings;

    impl Applet for Greetings {
        fn name(&self) -> &'static str {
            "greetings"
        }

        fn version(&self) -> &'static str {
            "0.1.0"
        }

        fn install(&self, editor: &mut Editor) {
            let scope = editor.add_scope("*Greeter*");
            editor.set_applet_state(GreeterScope(scope));
            let open = editor.add_command(CommandSpec {
                name: "greet",
                summary: "Open the greeter",
                help: None,
                scope: Scope::Global.into(),
                run: greet_open,
            });
            let more = editor.add_command(CommandSpec {
                name: "greet-more",
                summary: "Greet once more",
                help: None,
                scope,
                run: greet_more,
            });
            editor.bind(Scope::Global, "C-c g", open).ok();
            editor.bind(scope, "+", more).ok();
        }
    }

    fn type_keys(editor: &mut Editor, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
    }

    #[test]
    fn applet_buffers_have_their_own_keys_text_and_styles() {
        let mut editor = Editor::new();
        editor.install(&Greetings);

        type_keys(&mut editor, "C-c g + +");

        assert_eq!(editor.buffer().name(), "*Greeter*");
        assert!(editor.buffer().is_read_only());
        assert_eq!(editor.buffer().contents(), "Hello 2 times\n");
        assert_eq!(editor.line_style(0).spans, [(0..13, Highlight::Heading)]);
        type_keys(&mut editor, "C-x b RET +");
        assert_eq!(editor.buffer().name(), "*scratch*");
        assert_eq!(editor.buffer().contents(), "+");
    }

    #[derive(Debug, Default)]
    struct Answers(Vec<String>);

    fn note(editor: &mut Editor, answer: String) {
        if let Some(answers) = editor.applet_state_mut::<Answers>() {
            answers.0.push(answer);
        }
    }

    #[test]
    fn applets_prompt_with_their_own_completions_and_ask_questions() {
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());

        editor.prompt_with(
            "Fruit: ",
            |_| vec!["apple".into(), "banana".into(), "cherry".into()],
            note,
        );
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Fruit: ");
        type_keys(&mut editor, "a n");
        let offered: Vec<String> = editor
            .completions()
            .into_iter()
            .map(|completion| completion.label)
            .collect();
        assert_eq!(offered, ["banana"]);
        type_keys(&mut editor, "RET");

        editor.prompt("Name: ", note);
        assert!(!editor.minibuffer().unwrap().completes());
        type_keys(&mut editor, "z o e RET");

        editor.prompt_with_text("Port: ", "66", note);
        type_keys(&mut editor, "9 7 RET");

        editor.ask("Keep it?", |editor, keep| note(editor, keep.to_string()));
        assert_eq!(editor.question().unwrap().prompt(), "Keep it? (y or n) ");
        type_keys(&mut editor, "y");

        editor.ask("Again?", |editor, keep| note(editor, keep.to_string()));
        type_keys(&mut editor, "C-g");

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec![
                "banana".to_owned(),
                "zoe".to_owned(),
                "6697".to_owned(),
                "true".to_owned()
            ])
        );
        assert!(editor.question().is_none() && editor.minibuffer().is_none());
    }

    #[test]
    fn offers_carry_notes_and_secret_prompts_hide_what_is_typed() {
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());

        editor.prompt_with(
            "Server: ",
            |_| {
                vec![
                    Offer::noted("libera", "irc.libera.chat:6697"),
                    Offer::from("oftc"),
                ]
            },
            note,
        );
        let notes: Vec<Option<String>> = editor
            .completions()
            .into_iter()
            .map(|completion| completion.annotation)
            .collect();
        assert_eq!(notes, [Some("irc.libera.chat:6697".to_owned()), None]);
        type_keys(&mut editor, "C-g");

        editor.prompt_secret("Password: ", note);
        assert!(editor.minibuffer().unwrap().is_secret());
        assert!(!editor.minibuffer().unwrap().completes());
        type_keys(&mut editor, "s 3 c r 3 t RET");

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec!["s3cr3t".to_owned()])
        );
    }

    #[derive(Debug)]
    struct Fetch(String);

    #[derive(Debug)]
    struct Fetched(String);

    fn fetched(editor: &mut Editor, event: &Fetched) {
        note(editor, event.0.clone());
    }

    #[test]
    fn applets_request_effects_and_get_typed_events_back() {
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());
        editor.on_event(fetched);

        editor.request(Fetch("weather".to_owned()));
        let requested: Vec<String> = editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                crate::editor::Effect::Applet(payload) => {
                    payload.get::<Fetch>().map(|fetch| fetch.0.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(requested, ["weather"]);

        editor.deliver(&Payload::new(Fetched("sunny".to_owned())));
        editor.deliver(&Payload::new(7_u8));

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec!["sunny".to_owned()])
        );
    }

    static PALETTE: [Theme; 2] = [
        Theme {
            name: "palette-light",
            ..ADWAITA
        },
        Theme {
            name: "palette-dark",
            ..ADWAITA_DARK
        },
    ];

    struct Palette;

    impl Applet for Palette {
        fn name(&self) -> &'static str {
            "palette"
        }

        fn version(&self) -> &'static str {
            "1.0.0"
        }

        fn themes(&self) -> &'static [Theme] {
            &PALETTE
        }

        fn install(&self, _editor: &mut Editor) {}
    }

    #[test]
    fn applet_themes_can_be_loaded_configured_and_toggled() {
        let mut editor = Editor::new();

        editor.install(&Palette);
        type_keys(&mut editor, "M-x l o a d - t h e m e RET");
        let labels: Vec<String> = editor
            .completions()
            .into_iter()
            .map(|completion| completion.label)
            .collect();

        assert_eq!(
            labels,
            [
                "adwaita",
                "adwaita-dark",
                "palette-light",
                "palette-dark",
                "system"
            ]
        );
        assert!(Config::parse("theme = \"palette-dark\"").is_ok());
        assert_eq!(theme::counterpart(&PALETTE[1]).name, "palette-light");
    }

    #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct Greeting {
        text: String,
    }

    struct Hello;

    impl Applet for Hello {
        fn name(&self) -> &'static str {
            "hello"
        }

        fn version(&self) -> &'static str {
            "1.0.0"
        }

        fn install(&self, editor: &mut Editor) {
            editor.add_config_table::<Greeting>("hello");
        }
    }

    #[test]
    fn applets_read_their_own_config_table_and_mistakes_are_reported() {
        let mut editor = Editor::new();
        editor.install(&Hello);

        editor.apply_config(&Config::parse("[hello]\ntext = \"hi\"\n\n[other]\nx = 1\n").unwrap());
        assert_eq!(
            editor.applet_config::<Greeting>("hello"),
            Greeting {
                text: "hi".to_owned()
            }
        );
        assert_eq!(editor.unclaimed_config_tables(), ["other"]);

        editor.apply_config(&Config::parse("tab-width = 2\n[hello]\ntxt = \"hi\"\n").unwrap());
        let message = editor.message().unwrap_or_default();
        assert!(
            message.starts_with("[hello] unknown field `txt`"),
            "{message}"
        );
        assert_eq!(
            editor.applet_config::<Greeting>("hello"),
            Greeting::default()
        );
        assert_eq!(editor.tab_width(), 2);
    }

    fn fetch_effects(editor: &mut Editor) -> Vec<(crate::editor::FetchId, String)> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                crate::editor::Effect::Fetch(id, url) => Some((id, url)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn applets_fetch_urls_and_get_the_answer_once() {
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());

        editor.fetch("https://example.org/a", |editor, result| {
            if let Some(answers) = editor.applet_state_mut::<Answers>() {
                answers.0.push(result.unwrap_or_else(|error| error));
            }
        });
        let fetches = fetch_effects(&mut editor);
        assert_eq!(fetches.len(), 1);
        assert_eq!(fetches[0].1, "https://example.org/a");

        editor.fetched(fetches[0].0, Ok("body".to_owned()));
        editor.fetched(fetches[0].0, Err("again".to_owned()));
        editor.fetched(crate::editor::FetchId(999), Ok("stray".to_owned()));

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec!["body".to_owned()])
        );
    }

    #[test]
    fn applets_add_mode_line_segments_in_order() {
        let mut editor = Editor::new();

        let counted = |editor: &Editor| Some(format!("{} buffers", editor.buffer_names().len()));
        editor.add_mode_line_segment(counted);
        editor.add_mode_line_segment(|_| None);
        editor.add_mode_line_segment(counted);

        assert_eq!(editor.mode_line_segments(), ["1 buffers", "1 buffers"]);
    }

    fn remember_tab_width(editor: &mut Editor) {
        let width = editor.tab_width().to_string();
        if let Some(answers) = editor.applet_state_mut::<Answers>() {
            answers.0.push(width);
        }
    }

    #[test]
    fn applets_hear_about_every_config_change() {
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());
        editor.on_configured(remember_tab_width);

        editor.apply_config(&Config::parse("tab-width = 2\n").unwrap());
        editor.apply_config(&Config::parse("tab-width = 3\n").unwrap());

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec!["2".to_owned(), "3".to_owned()])
        );
    }

    #[test]
    fn applets_say_goodbye_when_the_editor_quits() {
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());
        editor.on_quit(|editor| {
            if let Some(answers) = editor.applet_state_mut::<Answers>() {
                answers.0.push("bye".to_owned());
            }
        });

        editor.quitting();

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec!["bye".to_owned()])
        );
    }

    #[derive(Debug)]
    struct Shell {
        scope: ScopeId,
        input: InputLine,
    }

    impl AppletBuffer for Shell {
        fn scope(&self) -> ScopeId {
            self.scope
        }

        fn input_line(&mut self) -> Option<&mut InputLine> {
            Some(&mut self.input)
        }
    }

    fn send(editor: &mut Editor) {
        let id = editor.buffer().id();
        let Some((buffer, shell)) = editor.applet_buffer_mut::<Shell>(id) else {
            return;
        };
        let text = shell.input.text(buffer).unwrap_or_default();
        shell.input.remember(&text);
        shell.input.replace(buffer, "");
        note(editor, text);
    }

    #[test]
    fn applet_buffers_can_take_input_after_a_prompt_with_history() {
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());
        let scope = editor.add_scope("*Shell*");
        let command = editor.add_command(CommandSpec {
            name: "shell-send",
            summary: "Send the input",
            help: None,
            scope,
            run: send,
        });
        editor.bind(scope, "RET", command).unwrap();
        editor
            .bind(scope, "M-p", crate::command::Command::ComintPreviousInput)
            .unwrap();
        let id = editor.add_applet_buffer_with("*Shell*", |buffer| {
            buffer.insert_at(0, "> ");
            Shell {
                scope,
                input: InputLine::new(buffer),
            }
        });
        assert_eq!(editor.buffer().name(), "*scratch*");
        editor.show_buffer(id);

        type_keys(&mut editor, "h i RET");
        assert_eq!(editor.buffer().contents(), "> ");
        type_keys(&mut editor, "C-a x");
        assert_eq!(editor.buffer().contents(), "> x");
        type_keys(&mut editor, "DEL DEL");
        assert_eq!(editor.buffer().contents(), "> ");
        type_keys(&mut editor, "M-p");
        assert_eq!(editor.buffer().contents(), "> hi");

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec!["hi".to_owned()])
        );
    }

    fn count_repeats(editor: &mut Editor) {
        let repeated = editor.repeated();
        note(editor, repeated.to_string());
    }

    #[test]
    fn applet_commands_know_when_they_run_again_right_away() {
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());
        let command = editor.add_command(CommandSpec {
            name: "count-repeats",
            summary: "Note whether this repeats",
            help: None,
            scope: Scope::Global.into(),
            run: count_repeats,
        });
        editor.bind(Scope::Global, "C-c r", command).unwrap();

        type_keys(&mut editor, "C-c r C-c r C-f C-c r");

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec![
                "false".to_owned(),
                "true".to_owned(),
                "false".to_owned()
            ])
        );
    }

    #[derive(Debug)]
    struct Board(ScopeId);

    impl AppletBuffer for Board {
        fn scope(&self) -> ScopeId {
            self.0
        }
    }

    #[test]
    fn applets_see_where_their_buffers_show_and_hear_before_each_display() {
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());
        editor.on_display(|editor| note(editor, "display".to_owned()));
        let scope = editor.add_scope("*Board*");
        let id = editor.add_applet_buffer_with("*Board*", |buffer| {
            buffer.insert_at(0, "one\ntwo\nthree");
            Board(scope)
        });
        assert!(!editor.buffer_shown(id));
        assert_eq!(editor.buffer_width(id), None);

        editor.show_buffer_beside(id);

        assert!(editor.buffer_shown(id));
        assert_eq!(editor.buffer().id(), id);
        assert_eq!(editor.tiling().windows().len(), 2);
        let window = editor.selected_window();
        editor.resize_pane(window, 10, 33);
        assert_eq!(editor.buffer_width(id), Some(32));
        editor.move_points(id, |_, end| Some(end));
        assert_eq!(editor.buffer().point(), 13);
        editor.prepare_display();
        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec!["display".to_owned(), "display".to_owned()])
        );
    }

    #[test]
    fn applets_hear_after_each_command_and_around_each_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plan.txt");
        std::fs::write(&path, "one\n").unwrap();
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());
        editor.on_command(|editor| note(editor, "command".to_owned()));
        editor.on_save(|editor, id| {
            if let Some(buffer) = editor.buffer_by_id_mut(id) {
                buffer.insert_at(0, "# ");
            }
            note(editor, "saving".to_owned());
        });
        editor.on_saved(|editor, id| {
            let name = editor
                .buffer_by_id(id)
                .map(|buffer| buffer.name().to_owned())
                .unwrap_or_default();
            note(editor, format!("saved {name}"));
        });
        let shout = editor.add_command(CommandSpec {
            name: "shout",
            summary: "Shout",
            help: None,
            scope: Scope::Global.into(),
            run: |editor| editor.show_message("hey"),
        });
        editor.bind(Scope::Global, "C-c s", shout).unwrap();
        editor.open_file(&path);
        editor.set_applet_state(Answers::default());

        type_keys(&mut editor, "C-e x C-c s C-x C-s");

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# onex\n");
        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec![
                "command".to_owned(),
                "command".to_owned(),
                "command".to_owned(),
                "saving".to_owned(),
                "saved plan.txt".to_owned(),
                "command".to_owned(),
            ])
        );
    }

    #[test]
    fn applets_reach_files_by_path_and_save_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plan.txt");
        std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
        let mut editor = Editor::new();
        editor.set_home(Some(dir.path().to_owned()));
        assert_eq!(editor.home(), Some(dir.path()));
        assert_eq!(editor.buffer_visiting(&path), None);

        editor.open_file(&path);
        let id = editor.buffer_visiting(&path).unwrap();
        assert!(editor.buffer_ids().contains(&id));
        let start = editor.current_location();
        editor.go_to_line(3, 2);
        editor.remember_location(start);
        if let Some(buffer) = editor.buffer_by_id_mut(id) {
            buffer.insert_at(0, "zero\n");
        }

        assert!(editor.save_buffer(id));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "zero\none\ntwo\nthree\n"
        );
        assert_eq!(editor.buffer().point(), 14);
        type_keys(&mut editor, "M-,");
        assert_eq!(editor.buffer().point(), 0);
    }

    #[derive(Debug)]
    struct Folder(ScopeId, std::path::PathBuf);

    impl AppletBuffer for Folder {
        fn scope(&self) -> ScopeId {
            self.0
        }

        fn directory(&self) -> Option<&Path> {
            Some(&self.1)
        }
    }

    #[test]
    fn applet_buffers_can_show_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        let mut editor = Editor::new();
        let scope = editor.add_scope("*Folder*");

        let id = editor.add_applet_buffer_with("*Folder*", |_| Folder(scope, docs.clone()));
        editor.show_buffer(id);

        assert_eq!(editor.default_directory(), docs);
        assert_eq!(
            editor.buffer().name(),
            format!("docs{}", crate::files::paths::SEPARATOR)
        );
        assert!(editor.watched_directories().contains(&docs));
    }

    #[test]
    fn applets_hear_about_disk_changes_and_open_directories() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());
        editor.on_disk_change(|editor| note(editor, "disk".to_owned()));
        editor.on_open_directory(|editor, path| {
            note(editor, format!("open {}", path.display()));
            true
        });

        editor.check_disk_changes();
        assert!(editor.open_file(dir.path()));

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec![
                "disk".to_owned(),
                format!("open {}", dir.path().display())
            ])
        );
    }

    #[test]
    fn path_prompts_complete_names_and_answer_with_a_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.txt"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        let mut editor = Editor::new();
        editor.set_working_directory(dir.path().to_owned());
        editor.set_applet_state(Answers::default());

        editor.prompt_path("Copy to: ", false, |editor, path| {
            note(editor, path.display().to_string());
        });
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Copy to: ");
        assert_eq!(editor.completions().len(), 2);
        type_keys(&mut editor, "n o t RET");

        editor.prompt_path("Directory: ", true, |editor, path| {
            note(editor, path.display().to_string());
        });
        assert_eq!(editor.completions().len(), 1);
        type_keys(&mut editor, "s u b / RET");

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec![
                dir.path().join("notes.txt").display().to_string(),
                dir.path().join("sub").display().to_string(),
            ])
        );
    }

    #[test]
    fn applets_ask_typed_and_multiple_choice_questions() {
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());

        editor.ask_typed("Delete it?", |editor, yes| note(editor, yes.to_string()));
        assert_eq!(
            editor.question().map(Question::prompt),
            Some("Delete it? (yes or no) ")
        );
        type_keys(&mut editor, "y RET");
        assert_eq!(editor.message(), Some("Please answer yes or no"));
        type_keys(&mut editor, "y e s RET");

        editor.ask_choice(
            "Overwrite b.txt?",
            &['y', 'n', '!', 'q'],
            |editor, choice| {
                note(editor, choice.to_string());
            },
        );
        assert_eq!(
            editor.question().map(Question::prompt),
            Some("Overwrite b.txt? (y, n, !, q) ")
        );
        type_keys(&mut editor, "!");

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec!["true".to_owned(), "!".to_owned()])
        );
    }

    fn open_as_document(editor: &mut Editor, path: &Path) -> bool {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        editor.show_message(format!("document {}", name.unwrap_or_default()));
        true
    }

    #[test]
    fn applets_open_the_file_types_they_claim() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("paper.PDF"), b"%PDF\xFF").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "plain").unwrap();
        let mut editor = Editor::new();
        editor.add_file_opener(&["pdf"], open_as_document);

        assert!(editor.open_file(&dir.path().join("paper.PDF")));
        assert_eq!(editor.message(), Some("document paper.PDF"));
        assert_eq!(editor.buffer_names(), ["*scratch*"]);

        assert!(editor.open_file(&dir.path().join("notes.txt")));
        assert_eq!(editor.buffer().contents(), "plain");
    }
    #[test]
    fn moved_files_keep_their_buffers() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("a.txt");
        let to = dir.path().join("c.txt");
        std::fs::write(&from, "hello").unwrap();
        let mut editor = Editor::new();
        editor.open_file(&from);
        std::fs::rename(&from, &to).unwrap();

        editor.file_moved(&from, &to);

        assert_eq!(editor.buffer().name(), "c.txt");
        assert_eq!(editor.buffer_visiting(&to), Some(editor.buffer().id()));
        assert_eq!(
            editor.absolute(Path::new("x")),
            editor.default_directory().join("x")
        );
    }

    #[test]
    fn g_bound_to_revert_buffer_in_an_applet_buffer_does_not_loop() {
        let mut editor = Editor::new();
        let scope = editor.add_scope("*Board*");
        editor
            .bind(scope, "g", crate::command::Command::RevertBuffer)
            .unwrap();
        let id = editor.add_applet_buffer_with("*Board*", |_| Board(scope));
        editor.show_buffer(id);

        editor.execute(crate::command::Command::RevertBuffer);

        assert_eq!(editor.buffer().name(), "*Board*");
    }
    #[test]
    fn reverting_an_applet_buffer_runs_what_g_runs() {
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());
        let scope = editor.add_scope("*Board*");
        let refresh = editor.add_command(CommandSpec {
            name: "board-refresh",
            summary: "Refresh",
            help: None,
            scope,
            run: |editor| note(editor, "refreshed".to_owned()),
        });
        editor.bind(scope, "g", refresh).unwrap();
        let id = editor.add_applet_buffer_with("*Board*", |_| Board(scope));
        editor.show_buffer(id);

        editor.execute(crate::command::Command::RevertBuffer);

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec!["refreshed".to_owned()])
        );
    }

    #[test]
    fn double_clicks_in_applet_buffers_run_what_return_runs() {
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());
        let scope = editor.add_scope("*Board*");
        let pick = editor.add_command(CommandSpec {
            name: "board-pick",
            summary: "Pick",
            help: None,
            scope,
            run: |editor| {
                let line = editor.buffer().line_of(editor.buffer().point());
                note(editor, format!("line {line}"));
            },
        });
        editor.bind(scope, "RET", pick).unwrap();
        let id = editor.add_applet_buffer_with("*Board*", |buffer| {
            buffer.insert_at(0, "one\ntwo\nthree");
            Board(scope)
        });
        editor.show_buffer(id);
        let window = editor.selected_window();
        editor.resize_pane(window, 10, 40);

        editor.click(
            crate::editor::Cell { row: 1, column: 0 },
            crate::editor::Clicks::Double,
            false,
        );

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec!["line 1".to_owned()])
        );
    }

    #[test]
    fn applets_open_links_outside_and_hide_buffers_they_showed() {
        let mut editor = Editor::new();
        editor.open_externally("https://example.org");
        assert_eq!(
            editor.take_effects(),
            [crate::editor::Effect::OpenExternally(
                "https://example.org".into()
            )]
        );
        let scope = editor.add_scope("*Board*");
        let id = editor.add_applet_buffer_with("*Board*", |_| Board(scope));
        let origin = editor.selected_window();
        editor.show_buffer_beside(id);
        editor.select_window(origin);

        assert!(editor.hide_buffer(id));
        assert_eq!(editor.tiling().windows(), [origin]);
        assert_eq!(editor.selected_window(), origin);
        assert!(!editor.hide_buffer(id));
    }

    #[test]
    fn pick_prompts_say_which_offer_was_chosen_or_what_was_typed() {
        fn ask(editor: &mut Editor) {
            editor.prompt_pick(
                "Note: ",
                |_| {
                    vec![
                        Offer::noted("Inbox", "personal"),
                        Offer::noted("Inbox", "work"),
                    ]
                },
                |editor, picked| note(editor, format!("{picked:?}")),
            );
        }
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());

        ask(&mut editor);
        type_keys(&mut editor, "C-n RET");
        ask(&mut editor);
        type_keys(&mut editor, "G a r d e n RET");
        ask(&mut editor);
        type_keys(&mut editor, "I n b M-RET");

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec![
                "Offered(1)".to_owned(),
                "Typed(\"Garden\")".to_owned(),
                "Typed(\"Inb\")".to_owned(),
            ])
        );
    }

    #[test]
    fn m_ret_answers_a_prompt_with_what_was_typed() {
        let mut editor = Editor::new();
        editor.set_applet_state(Answers::default());
        editor.prompt_with("Server: ", |_| vec![Offer::from("libera")], note);

        type_keys(&mut editor, "l i M-RET");

        assert_eq!(
            editor
                .applet_state::<Answers>()
                .map(|answers| answers.0.clone()),
            Some(vec!["li".to_owned()])
        );
    }

    #[test]
    fn styled_prompts_color_what_is_typed() {
        let mut editor = Editor::new();
        assert_eq!(editor.minibuffer_highlights(), []);

        editor.prompt_styled(
            "Capture: ",
            |_, text| vec![(0..text.chars().count(), Highlight::Keyword)],
            |_, _| {},
        );
        type_keys(&mut editor, "a b");

        assert_eq!(editor.minibuffer_highlights(), [(0..2, Highlight::Keyword)]);
        type_keys(&mut editor, "C-g");
        assert_eq!(editor.minibuffer_highlights(), []);
    }

    struct Syncer;

    impl Applet for Syncer {
        fn name(&self) -> &'static str {
            "syncer"
        }

        fn version(&self) -> &'static str {
            "1.0.0"
        }

        fn subcommands(&self) -> &'static [Subcommand] {
            &[Subcommand {
                name: "sync-daemon",
                summary: "Run the sync daemon without the editor",
                run: || Err("not today".to_owned()),
            }]
        }

        fn install(&self, _editor: &mut Editor) {}
    }

    #[test]
    fn applets_can_add_command_line_subcommands() {
        let applets: [&dyn Applet; 2] = [&Counter, &Syncer];

        let found = subcommand(applets, "sync-daemon").unwrap();

        assert_eq!(found.summary, "Run the sync daemon without the editor");
        assert_eq!((found.run)(), Err("not today".to_owned()));
        assert!(subcommand(applets, "notes.txt").is_none());
    }

    #[test]
    fn applets_borrow_their_state_while_they_change_the_editor() {
        let mut editor = Editor::new();
        editor.set_applet_state(Count(1));

        let hidden = editor.with_applet_state(|editor, count: &mut Count| {
            count.0 += 1;
            editor.show_message("busy");
            editor.applet_state::<Count>().is_none()
        });

        assert_eq!(hidden, Some(true));
        assert_eq!(editor.applet_state::<Count>().map(|count| count.0), Some(2));
        assert_eq!(editor.message(), Some("busy"));
        assert_eq!(editor.with_applet_state(|_, _: &mut String| ()), None);
    }

    #[test]
    fn applets_take_their_state_out_until_they_set_it_again() {
        let mut editor = Editor::new();
        editor.set_applet_state(Count(1));

        let taken = editor.take_applet_state::<Count>();

        assert_eq!(taken.map(|count| count.0), Some(1));
        assert!(editor.applet_state::<Count>().is_none());
        assert!(editor.take_applet_state::<Count>().is_none());
    }

    #[test]
    fn applets_find_list_and_close_their_buffers() {
        let mut editor = Editor::new();
        let scope = editor.add_scope("*Board*");
        let first = editor.add_applet_buffer_with("*Board one*", |_| Board(scope));
        let second = editor.add_applet_buffer_with("*Board two*", |_| Board(scope));

        assert_eq!(editor.applet_buffers::<Board>(), [first, second]);
        assert_eq!(editor.buffer_named("*Board two*"), Some(second));
        assert_eq!(editor.buffer_named("*Board three*"), None);

        editor.show_buffer(first);
        editor.buffer_mut().insert_at(0, "hello");
        assert_eq!(editor.buffer().contents(), "hello");
        editor.close_buffer(first);

        assert_eq!(editor.applet_buffers::<Board>(), [second]);
        assert_ne!(editor.buffer().id(), first);
    }

    #[test]
    fn state_is_kept_per_type() {
        let mut states = States::default();
        states.set(Count(3));
        states.set(String::from("text"));

        assert_eq!(states.get::<Count>().map(|count| count.0), Some(3));
        assert_eq!(states.get::<String>().map(String::as_str), Some("text"));
        assert!(states.get::<u8>().is_none());
    }
}
