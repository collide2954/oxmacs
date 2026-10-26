use ox_lang::heap::{Heap, Hold};
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::Editor;

use crate::ide::log;

const NAME: &str = "set-tab-bar";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabBar {
    pub tabs: Vec<String>,
    pub selected: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabEvent {
    Select(usize),
    Close(usize),
    New,
    Move { from: usize, to: usize },
}

#[derive(Debug, Default)]
struct Bar {
    shown: Option<TabBar>,
    select: Option<Hold>,
    close: Option<Hold>,
    new: Option<Hold>,
    moved: Option<Hold>,
}

impl Bar {
    fn holds(&mut self) -> Vec<Hold> {
        [
            self.select.take(),
            self.close.take(),
            self.new.take(),
            self.moved.take(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

pub fn shown(editor: &Editor) -> Option<&TabBar> {
    editor
        .plugin_state::<Bar>()
        .and_then(|bar| bar.shown.as_ref())
}

fn wrong(heap: &Heap, wanted: &str, value: Value) -> Error {
    Error::new(format!(
        "{NAME} {wanted}, got {}",
        print::brief(heap, value)
    ))
}

fn index(heap: &Heap, value: Value) -> Result<Option<usize>, Error> {
    match value {
        Value::Nil => Ok(None),
        Value::Int(number) => usize::try_from(number)
            .map(Some)
            .map_err(|_| wrong(heap, ":selected must be a tab's index", value)),
        _ => Err(wrong(heap, ":selected must be a tab's index", value)),
    }
}

fn set_tab_bar(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let mut bar = editor.take_plugin_state::<Bar>().unwrap_or_default();
    for old in bar.holds() {
        heap.release(old);
    }
    bar.shown = None;
    let result = fill(heap, &mut bar, arguments);
    if result.is_err() {
        for hold in bar.holds() {
            heap.release(hold);
        }
        bar.shown = None;
    }
    editor.set_plugin_state(bar);
    result.map(|()| Value::Nil)
}

fn fill(heap: &mut Heap, bar: &mut Bar, arguments: &[Value]) -> Result<(), Error> {
    if arguments[0] == Value::Nil {
        return Ok(());
    }
    let tabs: Vec<String> = heap
        .elements(arguments[0])
        .ok_or_else(|| wrong(heap, "takes a vector of tab labels", arguments[0]))?
        .into_iter()
        .map(|label| print::display(heap, label))
        .collect();
    let selected = index(heap, arguments[1])?;
    let mut held = |value: Value| (value != Value::Nil).then(|| heap.hold(value));
    bar.select = held(arguments[2]);
    bar.close = held(arguments[3]);
    bar.new = held(arguments[4]);
    bar.moved = held(arguments[5]);
    if selected.is_some_and(|at| at >= tabs.len()) {
        return Err(Error::new(format!(
            "{NAME}'s :selected is past the last of {} tabs",
            tabs.len()
        )));
    }
    bar.shown = Some(TabBar { tabs, selected });
    Ok(())
}

fn count(number: usize) -> Value {
    Value::Int(i64::try_from(number).unwrap_or(i64::MAX))
}

pub fn event(isolate: &mut Isolate, editor: &mut Editor, event: TabEvent) {
    let Some(bar) = editor.plugin_state::<Bar>() else {
        return;
    };
    let (hold, arguments) = match event {
        TabEvent::Select(at) => (bar.select, vec![count(at)]),
        TabEvent::Close(at) => (bar.close, vec![count(at)]),
        TabEvent::New => (bar.new, Vec::new()),
        TabEvent::Move { from, to } => (bar.moved, vec![count(from), count(to)]),
    };
    let Some(handler) = hold.and_then(|hold| isolate.held(hold)) else {
        return;
    };
    if let Err(error) = isolate.spawn(handler, &arguments) {
        log::report(editor, &error, None);
    }
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", NAME)
        .arity(1, Some(1))
        .keywords(&["selected", "on-select", "on-close", "on-new", "on-move"])
        .host::<Editor>(set_tab_bar);
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

    use super::{TabBar, TabEvent, shown};
    use crate::Ox;
    use crate::testing::settle;

    #[test]
    fn tab_bars_show_their_labels_and_call_back() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(
            &mut editor,
            r#"(def heard (atom []))
(set-tab-bar ["main" :notes] :selected 1
  :on-select (fn [i] (swap! heard conj [:select i]))
  :on-close (fn [i] (swap! heard conj [:close i]))
  :on-new (fn [] (swap! heard conj :new))
  :on-move (fn [from to] (swap! heard conj [:move from to])))"#,
        )
        .unwrap();

        assert_eq!(
            shown(&editor),
            Some(&TabBar {
                tabs: vec!["main".to_owned(), ":notes".to_owned()],
                selected: Some(1),
            })
        );
        for happened in [
            TabEvent::Select(0),
            TabEvent::Close(1),
            TabEvent::New,
            TabEvent::Move { from: 0, to: 1 },
        ] {
            ox.tab_event(&mut editor, happened);
        }
        settle(&mut editor, &mut ox);
        assert_eq!(
            ox.eval(&mut editor, "@heard").unwrap(),
            "[[:select 0] [:close 1] :new [:move 0 1]]"
        );

        ox.eval(&mut editor, "(set-tab-bar nil)").unwrap();
        assert_eq!(shown(&editor), None);
    }

    #[test]
    fn mistakes_in_tab_bars_are_explained_and_hide_it() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, r#"(set-tab-bar ["a"])"#).unwrap();

        for (source, message) in [
            (
                "(set-tab-bar 5)",
                "set-tab-bar takes a vector of tab labels, got 5",
            ),
            (
                r#"(set-tab-bar ["a"] :selected 3)"#,
                "set-tab-bar's :selected is past the last of 1 tabs",
            ),
            (
                r#"(set-tab-bar ["a"] :color 3)"#,
                "set-tab-bar has no keyword parameter :color",
            ),
        ] {
            assert_eq!(ox.eval(&mut editor, source).unwrap_err().message, message);
            assert_eq!(shown(&editor), None);
        }
    }
}
