use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
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

    fn install(&self, editor: &mut Editor);
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
}

#[cfg(test)]
mod tests {
    use super::Payload;
    use crate::command::{CommandSpec, Scope, ScopeId};
    use crate::config::Config;
    use crate::display::LineStyle;
    use crate::editor::Page;
    use crate::editor::{Editor, InputLine};
    use crate::input::KeySequence;
    use crate::theme::{self, ADWAITA, ADWAITA_DARK};

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
        assert_eq!(
            editor.line_style(0).spans,
            [(0..13, theme::Highlight::Heading)]
        );
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
