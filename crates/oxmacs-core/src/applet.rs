use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::command::ScopeId;
use crate::display::LineStyle;
use crate::editor::Editor;
use crate::theme::Theme;

pub trait AppletBuffer: Any + Send + fmt::Debug {
    fn scope(&self) -> ScopeId;

    fn line_style(&self, _line: usize) -> LineStyle {
        LineStyle::default()
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
    use crate::editor::Editor;
    use crate::editor::Page;
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
            |_| vec!["apple".to_owned(), "banana".to_owned(), "cherry".to_owned()],
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
                "true".to_owned()
            ])
        );
        assert!(editor.question().is_none() && editor.minibuffer().is_none());
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
