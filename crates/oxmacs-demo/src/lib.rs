use std::time::{SystemTime, UNIX_EPOCH};

use iced::Task;
use oxmacs_core::applet::{Applet, AppletBuffer, Payload};
use oxmacs_core::command::{Command, CommandSpec, Scope, ScopeId};
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::{Editor, Page};
use oxmacs_core::theme::Highlight;
use oxmacs_gui::applet::GuiApplet;

const NAME: &str = "*Demo*";
const HELP: &str = "a add · d delete · r roll a die · q quit";
const FIRST_ITEM: usize = 3;
const SUGGESTIONS: &[&str] = &[
    "Steep the tea",
    "Water the plants",
    "Write the changelog",
    "Read the README",
];

#[derive(Debug)]
pub struct Demo;

#[derive(Debug)]
pub struct Roll;

#[derive(Debug)]
pub struct Rolled(pub u8);

#[derive(Debug, Default)]
struct List {
    scope: Option<ScopeId>,
    items: Vec<String>,
    rolled: Option<u8>,
}

#[derive(Debug)]
struct View {
    scope: ScopeId,
    styles: Vec<LineStyle>,
}

impl AppletBuffer for View {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }
}

pub fn roll() -> u8 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos());
    u8::try_from(nanos % 6).unwrap_or(0) + 1
}

fn open(editor: &mut Editor) {
    let Some(scope) = editor.applet_state::<List>().and_then(|list| list.scope) else {
        return;
    };
    match editor.find_applet_buffer::<View>(|_| true) {
        Some(id) => editor.show_buffer(id),
        None => {
            editor.open_applet_buffer(
                NAME,
                View {
                    scope,
                    styles: Vec::new(),
                },
            );
        }
    }
    render(editor);
}

fn render(editor: &mut Editor) {
    let Some(id) = editor.find_applet_buffer::<View>(|_| true) else {
        return;
    };
    let (items, rolled) = editor
        .applet_state::<List>()
        .map(|list| (list.items.clone(), list.rolled))
        .unwrap_or_default();
    let mut page = Page::default();
    page.heading("Demo applet");
    page.line(&[(HELP, Some(Highlight::Comment))]);
    page.line(&[]);
    for item in &items {
        page.line(&[("• ", Some(Highlight::Link)), (item, None)]);
    }
    if let Some(rolled) = rolled {
        page.line(&[]);
        page.line(&[(&format!("Rolled a {rolled}"), Some(Highlight::Constant))]);
    }
    let (text, styles) = page.into_parts();
    if let Some((_, view)) = editor.applet_buffer_mut::<View>(id) {
        view.styles = styles;
    }
    editor.fill_buffer(id, &text);
}

fn add(editor: &mut Editor) {
    editor.prompt_with(
        "Add item: ",
        |_| SUGGESTIONS.iter().map(|item| (*item).to_owned()).collect(),
        |editor, item| {
            if item.trim().is_empty() {
                return;
            }
            if let Some(list) = editor.applet_state_mut::<List>() {
                list.items.push(item);
            }
            render(editor);
        },
    );
}

fn delete(editor: &mut Editor) {
    let buffer = editor.buffer();
    let line = buffer.line_of(buffer.point());
    let item = line.checked_sub(FIRST_ITEM).and_then(|index| {
        editor
            .applet_state::<List>()
            .and_then(|list| list.items.get(index).cloned())
    });
    let Some(item) = item else {
        editor.show_message("No item here");
        return;
    };
    editor.ask(&format!("Delete {item}?"), move |editor, yes| {
        if yes {
            if let Some(list) = editor.applet_state_mut::<List>() {
                list.items.retain(|known| *known != item);
            }
            render(editor);
        }
    });
}

fn ask_for_roll(editor: &mut Editor) {
    editor.request(Roll);
    editor.show_message("Rolling…");
}

fn rolled(editor: &mut Editor, event: &Rolled) {
    if let Some(list) = editor.applet_state_mut::<List>() {
        list.rolled = Some(event.0);
    }
    render(editor);
}

impl Applet for Demo {
    fn name(&self) -> &'static str {
        "demo"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn opt_in(&self) -> bool {
        true
    }

    fn install(&self, editor: &mut Editor) {
        let scope = editor.add_scope(NAME);
        editor.set_applet_state(List {
            scope: Some(scope),
            ..List::default()
        });
        editor.add_command(CommandSpec {
            name: "demo",
            summary: "Open the demo applet",
            help: Some(
                "An example applet showing how applets add commands, keys, buffers, prompts, questions and effects. Its source and crates/oxmacs-demo/README.md walk through each part.",
            ),
            scope: Scope::Global.into(),
            run: open,
        });
        let local = [
            (
                "a",
                "demo-add",
                "Add an item, picking from suggestions",
                add as fn(&mut Editor),
            ),
            (
                "d",
                "demo-delete",
                "Delete the item at point after asking",
                delete,
            ),
            ("r", "demo-roll", "Ask the GUI to roll a die", ask_for_roll),
        ];
        for (key, name, summary, run) in local {
            let command = editor.add_command(CommandSpec {
                name,
                summary,
                help: None,
                scope,
                run,
            });
            editor.bind(scope, key, command).ok();
        }
        editor.bind(scope, "q", Command::QuitWindow).ok();
        editor.on_event(rolled);
    }
}

impl GuiApplet for Demo {
    fn perform(&self, effect: &Payload) -> Option<Task<Payload>> {
        effect.get::<Roll>()?;
        Some(Task::perform(async { roll() }, |value| {
            Payload::new(Rolled(value))
        }))
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::applet::Payload;
    use oxmacs_core::editor::{Editor, Effect};
    use oxmacs_core::input::KeySequence;
    use oxmacs_gui::applet::GuiApplet;

    use super::*;

    fn keys(editor: &mut Editor, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
    }

    fn typed(editor: &mut Editor, text: &str) {
        let spaced: Vec<String> = text.chars().map(|c| c.to_string()).collect();
        keys(editor, &spaced.join(" "));
    }

    fn opened() -> Editor {
        let mut editor = Editor::new();
        editor.install(&Demo);
        keys(&mut editor, "M-x");
        typed(&mut editor, "demo");
        keys(&mut editor, "RET");
        editor
    }

    fn lines(editor: &Editor) -> Vec<String> {
        editor
            .buffer()
            .contents()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn m_x_demo_opens_a_styled_buffer_with_its_own_keys() {
        let editor = opened();

        assert_eq!(editor.buffer().name(), "*Demo*");
        assert_eq!(lines(&editor)[0], "Demo applet");
        assert!(lines(&editor)[1].contains("a add"));
        assert_eq!(editor.line_style(0).spans, [(0..11, Highlight::Heading)]);
        assert_eq!(editor.installed()[0].version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn a_adds_an_item_from_a_prompt_with_suggestions() {
        let mut editor = opened();

        keys(&mut editor, "a");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Add item: ");
        typed(&mut editor, "tea");
        assert_eq!(editor.completions()[0].label, "Steep the tea");
        keys(&mut editor, "RET");

        assert!(lines(&editor).contains(&"• Steep the tea".to_owned()));
    }

    #[test]
    fn d_deletes_the_item_at_point_after_asking() {
        let mut editor = opened();
        keys(&mut editor, "a");
        typed(&mut editor, "tea");
        keys(&mut editor, "RET M-> C-p");

        keys(&mut editor, "d");
        assert_eq!(
            editor.question().unwrap().prompt(),
            "Delete Steep the tea? (y or n) "
        );
        keys(&mut editor, "y");

        assert!(!lines(&editor).iter().any(|line| line.contains("tea")));
    }

    #[test]
    fn r_asks_the_gui_to_roll_a_die_and_shows_the_result() {
        let mut editor = opened();

        keys(&mut editor, "r");
        let rolls: Vec<Payload> = editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Applet(payload) => Some(payload),
                _ => None,
            })
            .collect();
        assert_eq!(rolls.len(), 1);
        assert!(Demo.perform(&rolls[0]).is_some());

        editor.deliver(&Payload::new(Rolled(4)));

        assert!(lines(&editor).contains(&"Rolled a 4".to_owned()));
    }

    #[test]
    fn dice_land_between_one_and_six() {
        for _ in 0..20 {
            assert!((1..=6).contains(&roll()));
        }
    }
}
