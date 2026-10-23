use ox::heap::{Heap, Hold};
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::Editor;
use oxmacs_core::plugin::PluginBuffer;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::{Highlight, NICK_COLORS};

use crate::log;
use crate::shapes::{self, Shape};

const UI: &str = "ox/ui";

#[derive(Debug, Clone, PartialEq)]
pub enum Press {
    Held(Hold),
    Slot(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Column {
        spacing: f32,
        children: Vec<Node>,
    },
    Row {
        spacing: f32,
        children: Vec<Node>,
    },
    Text {
        text: String,
        face: Option<Highlight>,
        size: Option<f32>,
    },
    Button {
        label: String,
        press: Option<Press>,
    },
    Space {
        size: Option<f32>,
    },
    Scrollable(Box<Node>),
    Canvas {
        shapes: Vec<Shape>,
        height: Option<f32>,
        relative: bool,
    },
}

#[derive(Debug)]
pub struct ViewBuffer {
    scope: ScopeId,
    pub node: Node,
}

impl PluginBuffer for ViewBuffer {
    fn scope(&self) -> ScopeId {
        self.scope
    }
}

#[derive(Debug)]
struct Open {
    name: String,
    function: Hold,
    callbacks: Vec<Hold>,
    buffer: BufferId,
    scope: ScopeId,
}

#[derive(Debug, Default)]
struct Views {
    open: Vec<Open>,
    dirty: bool,
    seen: u64,
}

const FACES: &[(&str, Highlight)] = &[
    ("heading", Highlight::Heading),
    ("link", Highlight::Link),
    ("symlink", Highlight::Symlink),
    ("comment", Highlight::Comment),
    ("doc-comment", Highlight::DocComment),
    ("keyword", Highlight::Keyword),
    ("string", Highlight::String),
    ("function", Highlight::Function),
    ("type", Highlight::Type),
    ("constant", Highlight::Constant),
    ("attribute", Highlight::Attribute),
    ("macro", Highlight::Macro),
    ("success", Highlight::Success),
    ("warning", Highlight::Warning),
    ("error", Highlight::Error),
    ("added", Highlight::Added),
    ("removed", Highlight::Removed),
    ("marked", Highlight::Marked),
    ("flagged", Highlight::Flagged),
    ("region", Highlight::Region),
];

pub(crate) fn face_name(highlight: Highlight) -> Option<&'static str> {
    FACES
        .iter()
        .find(|(_, known)| *known == highlight)
        .map(|(name, _)| *name)
}

pub(crate) fn face_named(name: &str) -> Option<Highlight> {
    if let Some(number) = name.strip_prefix("nick-") {
        return number
            .parse::<u8>()
            .ok()
            .filter(|number| *number < NICK_COLORS)
            .map(Highlight::Nick);
    }
    FACES
        .iter()
        .find(|(face, _)| *face == name)
        .map(|(_, highlight)| *highlight)
}

fn wrong(heap: &Heap, name: &str, wanted: &str, value: Value) -> Error {
    Error::new(format!(
        "{name} expects {wanted}, got {}",
        print::write(heap, value)
    ))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "view sizes are small pixel counts"
)]
fn size(heap: &Heap, name: &str, value: Value) -> Result<f32, Error> {
    match value {
        Value::Int(number) => Ok(number as f32),
        Value::Float(number) => Ok(number as f32),
        _ => Err(wrong(heap, name, "a number", value)),
    }
}

type Options<'a> = (Vec<(&'static str, Value)>, &'a [Value]);

fn options<'a>(
    heap: &Heap,
    name: &str,
    arguments: &'a [Value],
    known: &[&str],
) -> Result<Options<'a>, Error> {
    let mut found = Vec::new();
    let mut rest = arguments;
    while let [Value::Keyword(key), value, more @ ..] = rest {
        if !known.contains(&key.name()) {
            break;
        }
        found.push((key.name(), *value));
        rest = more;
    }
    if let [Value::Keyword(key), ..] = rest
        && known.contains(&key.name())
    {
        return Err(Error::new(format!("{name}'s :{} has no value", key.name())));
    }
    if let Some(stray) = rest.iter().find(|value| matches!(value, Value::Keyword(_))) {
        return Err(Error::new(format!(
            "{name} doesn't take {}",
            print::write(heap, *stray)
        )));
    }
    Ok((found, rest))
}

fn children(heap: &Heap, name: &str, values: &[Value], out: &mut Vec<Node>) -> Result<(), Error> {
    for value in values {
        if *value == Value::Nil {
            continue;
        }
        if let Some(node) = heap.foreign_of::<Node>(*value) {
            out.push(node.clone());
        } else if let Some(text) = heap.text(*value) {
            out.push(Node::Text {
                text: text.to_owned(),
                face: None,
                size: None,
            });
        } else if let Some(items) = heap.elements(*value) {
            children(heap, name, &items, out)?;
        } else {
            return Err(Error::new(format!(
                "{name} can't show {}",
                print::write(heap, *value)
            )));
        }
    }
    Ok(())
}

fn widget(heap: &mut Heap, name: &str, node: Node) -> Result<Value, Error> {
    heap.foreign(name, node)
        .map_err(|full| Error::new(full.to_string()))
}

fn stack(heap: &mut Heap, name: &str, arguments: &[Value], row: bool) -> Result<Value, Error> {
    let trailing = match arguments {
        [.., Value::Keyword(key), _] if key.name() == "spacing" => 2,
        _ => 0,
    };
    let (inside, last) = arguments.split_at(arguments.len() - trailing);
    let mut given = last.to_vec();
    given.extend_from_slice(inside);
    let (found, rest) = options(heap, name, &given, &["spacing"])?;
    let mut spacing = 0.0;
    for (_, value) in found {
        spacing = size(heap, name, value)?;
    }
    let mut nodes = Vec::new();
    children(heap, name, rest, &mut nodes)?;
    let node = if row {
        Node::Row {
            spacing,
            children: nodes,
        }
    } else {
        Node::Column {
            spacing,
            children: nodes,
        }
    };
    widget(heap, name, node)
}

fn text(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let shown = print::display(heap, arguments[0]);
    let (found, rest) = options(heap, "text", &arguments[1..], &["face", "size"])?;
    if let Some(extra) = rest.first() {
        return Err(wrong(heap, "text", "options after its value", *extra));
    }
    let mut face = None;
    let mut text_size = None;
    for (key, value) in found {
        if key == "size" {
            text_size = Some(size(heap, "text", value)?);
            continue;
        }
        let known = match value {
            Value::Keyword(name) => face_named(name.name()),
            _ => None,
        };
        face = Some(known.ok_or_else(|| {
            Error::new(format!(
                "text doesn't know the face {}",
                print::write(heap, value)
            ))
        })?);
    }
    widget(
        heap,
        "text",
        Node::Text {
            text: shown,
            face,
            size: text_size,
        },
    )
}

fn button(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let label = print::display(heap, arguments[0]);
    let (found, rest) = options(heap, "button", &arguments[1..], &["on-press"])?;
    if let Some(extra) = rest.first() {
        return Err(wrong(heap, "button", "options after its label", *extra));
    }
    let press = found
        .first()
        .map(|(_, function)| Press::Held(heap.hold(*function)));
    widget(heap, "button", Node::Button { label, press })
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_native_in(UI, "column", 0, None, |heap, arguments| {
        stack(heap, "column", arguments, false)
    });
    isolate.define_native_in(UI, "row", 0, None, |heap, arguments| {
        stack(heap, "row", arguments, true)
    });
    isolate.define_native_in(UI, "text", 1, None, text);
    isolate.define_native_in(UI, "button", 1, None, button);
    isolate.define_native_in(UI, "space", 0, Some(1), |heap, arguments| {
        let size = match arguments.first() {
            Some(value) => Some(size(heap, "space", *value)?),
            None => None,
        };
        widget(heap, "space", Node::Space { size })
    });
    isolate.define_native_in(UI, "scrollable", 1, Some(1), |heap, arguments| {
        let mut nodes = Vec::new();
        children(heap, "scrollable", &arguments[..1], &mut nodes)?;
        let child = nodes
            .into_iter()
            .next()
            .unwrap_or(Node::Space { size: None });
        widget(heap, "scrollable", Node::Scrollable(Box::new(child)))
    });
    isolate.define_native_in(UI, "canvas", 1, None, |heap, arguments| {
        let drawn = shapes::shapes(heap, arguments[0])?;
        let (found, rest) = options(heap, "canvas", &arguments[1..], &["height", "relative"])?;
        if let Some(extra) = rest.first() {
            return Err(wrong(heap, "canvas", "options after its shapes", *extra));
        }
        let mut height = None;
        let mut relative = false;
        for (option, value) in found {
            if option == "relative" {
                relative = value.truthy();
            } else {
                height = Some(size(heap, "canvas", value)?);
            }
        }
        widget(
            heap,
            "canvas",
            Node::Canvas {
                shapes: drawn,
                height,
                relative,
            },
        )
    });
    isolate.define_host_native::<Editor>("open-view", 2, None, open);
}

fn keys(heap: &Heap, value: Value) -> Result<Vec<(String, String)>, Error> {
    let name = |value: Value| match value {
        Value::Keyword(name) | Value::Symbol(name) => name.name().to_owned(),
        _ => print::display(heap, value),
    };
    heap.entries(value)
        .ok_or_else(|| {
            wrong(
                heap,
                "open-view",
                "a map of keys to commands for :keys",
                value,
            )
        })
        .map(|entries| {
            entries
                .into_iter()
                .map(|(key, command)| (name(key), name(command)))
                .collect()
        })
}

pub(crate) fn buffer_name(heap: &Heap, native: &str, value: Value) -> Result<String, Error> {
    heap.text(value)
        .map(str::to_owned)
        .ok_or_else(|| wrong(heap, native, "a buffer name", value))
}

fn open(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = buffer_name(heap, "open-view", arguments[0])?;
    let (found, rest) = options(heap, "open-view", &arguments[2..], &["keys"])?;
    if let Some(extra) = rest.first() {
        return Err(wrong(
            heap,
            "open-view",
            "options after its function",
            *extra,
        ));
    }
    let mut bindings = Vec::new();
    for (_, value) in found {
        bindings = keys(heap, value)?;
    }
    let mut views = editor.take_plugin_state::<Views>().unwrap_or_default();
    let (alive, closed): (Vec<Open>, Vec<Open>) = std::mem::take(&mut views.open)
        .into_iter()
        .partition(|view| editor.buffer_by_id(view.buffer).is_some());
    for view in closed {
        heap.release(view.function);
        for callback in view.callbacks {
            heap.release(callback);
        }
    }
    views.open = alive;
    let function = heap.hold(arguments[1]);
    let index = if let Some(index) = views.open.iter().position(|view| view.name == name) {
        heap.release(views.open[index].function);
        views.open[index].function = function;
        index
    } else {
        let scope = editor.add_scope(Box::leak(name.clone().into_boxed_str()));
        let buffer = editor.add_plugin_buffer_with(&name, |buffer| {
            buffer.set_read_only(true);
            ViewBuffer {
                scope,
                node: Node::Column {
                    spacing: 0.0,
                    children: Vec::new(),
                },
            }
        });
        views.open.push(Open {
            name,
            function,
            callbacks: Vec::new(),
            buffer,
            scope,
        });
        views.open.len() - 1
    };
    let (buffer, scope) = (views.open[index].buffer, views.open[index].scope);
    views.dirty = true;
    editor.set_plugin_state(views);
    for (keys, command) in bindings {
        let id = editor
            .command_named(&command)
            .ok_or_else(|| Error::new(format!("There is no command named {command}")))?;
        editor
            .bind(scope, &keys, id)
            .map_err(|error| Error::new(format!("{keys}: {error}")))?;
    }
    editor.show_buffer(buffer);
    Ok(Value::Nil)
}

fn number(node: Node, callbacks: &mut Vec<Hold>) -> Node {
    match node {
        Node::Column { spacing, children } => Node::Column {
            spacing,
            children: children
                .into_iter()
                .map(|child| number(child, callbacks))
                .collect(),
        },
        Node::Row { spacing, children } => Node::Row {
            spacing,
            children: children
                .into_iter()
                .map(|child| number(child, callbacks))
                .collect(),
        },
        Node::Scrollable(child) => Node::Scrollable(Box::new(number(*child, callbacks))),
        Node::Button {
            label,
            press: Some(Press::Held(hold)),
        } => {
            callbacks.push(hold);
            Node::Button {
                label,
                press: Some(Press::Slot(callbacks.len() - 1)),
            }
        }
        other => other,
    }
}

fn render(
    isolate: &mut Isolate,
    editor: &mut Editor,
    function: Hold,
) -> Result<(Node, Vec<Hold>), Error> {
    let function = isolate
        .held(function)
        .ok_or_else(|| Error::new("A view lost its function"))?;
    let value = isolate.call_with(editor, function, &[])?;
    let node = isolate
        .heap()
        .foreign_of::<Node>(value)
        .cloned()
        .ok_or_else(|| {
            Error::new(format!(
                "A view must return a widget, got {}",
                isolate.write(value)
            ))
        })?;
    let mut callbacks = Vec::new();
    let node = number(node, &mut callbacks);
    Ok((node, callbacks))
}

pub fn redraw(editor: &mut Editor) {
    if let Some(views) = editor.plugin_state_mut::<Views>() {
        views.dirty = true;
    }
}

pub fn refresh(isolate: &mut Isolate, editor: &mut Editor) {
    let writes = isolate.heap().atom_writes();
    let Some(views) = editor.plugin_state_mut::<Views>() else {
        return;
    };
    if !views.dirty && views.seen == writes {
        return;
    }
    views.dirty = false;
    let open: Vec<(BufferId, Hold)> = views
        .open
        .iter()
        .map(|view| (view.buffer, view.function))
        .collect();
    for (buffer, function) in open {
        if editor.buffer_by_id(buffer).is_none() {
            continue;
        }
        let (node, callbacks) = match render(isolate, editor, function) {
            Ok(rendered) => rendered,
            Err(error) => {
                log::report(editor, &error, None);
                continue;
            }
        };
        if let Some(view) = editor
            .plugin_state_mut::<Views>()
            .and_then(|views| views.open.iter_mut().find(|view| view.buffer == buffer))
        {
            for old in std::mem::replace(&mut view.callbacks, callbacks) {
                isolate.release(old);
            }
        }
        if let Some((_, shown)) = editor.plugin_buffer_mut::<ViewBuffer>(buffer)
            && shown.node != node
        {
            shown.node = node;
        }
    }
    let writes = isolate.heap().atom_writes();
    if let Some(views) = editor.plugin_state_mut::<Views>() {
        views.seen = writes;
    }
}

pub fn press(isolate: &mut Isolate, editor: &mut Editor, buffer: BufferId, slot: usize) {
    let callback = editor
        .plugin_state::<Views>()
        .and_then(|views| views.open.iter().find(|view| view.buffer == buffer))
        .and_then(|view| view.callbacks.get(slot).copied())
        .and_then(|hold| isolate.held(hold));
    let Some(callback) = callback else {
        return;
    };
    if let Err(error) = isolate.spawn(callback, &[]) {
        log::report(editor, &error, None);
    }
}
#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::text::Buffer;
    use oxmacs_core::theme::Highlight;

    use super::{Node, Press, ViewBuffer};
    use crate::testing::{keys, settle};
    use crate::{Ox, log};

    const UI: &str = "(module (import (ox ui :only [column row text button space scrollable])))";

    fn shown(editor: &Editor, name: &str) -> Node {
        let id = editor.buffer_named(name).unwrap();
        editor
            .plugin_buffer::<ViewBuffer>(id)
            .unwrap()
            .1
            .node
            .clone()
    }

    fn text(value: &str) -> Node {
        Node::Text {
            text: value.to_owned(),
            face: None,
            size: None,
        }
    }

    fn opened(source: &str) -> (Editor, Ox) {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, &format!("{UI} {source}")).unwrap();
        settle(&mut editor, &mut ox);
        (editor, ox)
    }

    fn logged(editor: &Editor) -> String {
        editor
            .buffer_named(log::LOG)
            .and_then(|id| editor.buffer_by_id(id))
            .map(Buffer::contents)
            .unwrap_or_default()
    }

    #[test]
    fn views_build_widget_trees_in_their_own_buffer() {
        let (editor, _) = opened(
            r#"(open-view "*hello*" (fn [] (column :spacing 4 (text "Hello" :face :heading :size 18) (row (text 1) nil [(text "a") (text "b")]) (space) (space 8) (scrollable (text "long")))))"#,
        );

        assert_eq!(editor.buffer().name(), "*hello*");
        assert_eq!(
            shown(&editor, "*hello*"),
            Node::Column {
                spacing: 4.0,
                children: vec![
                    Node::Text {
                        text: "Hello".to_owned(),
                        face: Some(Highlight::Heading),
                        size: Some(18.0),
                    },
                    Node::Row {
                        spacing: 0.0,
                        children: vec![text("1"), text("a"), text("b")],
                    },
                    Node::Space { size: None },
                    Node::Space { size: Some(8.0) },
                    Node::Scrollable(Box::new(text("long"))),
                ],
            }
        );
    }

    #[test]
    fn stacks_take_their_spacing_after_their_children_too() {
        let (editor, _) =
            opened(r#"(open-view "*after*" (fn [] (row (text "a") (text "b") :spacing 3)))"#);

        assert_eq!(
            shown(&editor, "*after*"),
            Node::Row {
                spacing: 3.0,
                children: vec![text("a"), text("b")],
            }
        );
    }

    #[test]
    fn views_redraw_when_atoms_change() {
        let (mut editor, mut ox) =
            opened(r#"(def count (atom 0)) (open-view "*count*" (fn [] (text @count)))"#);
        assert_eq!(shown(&editor, "*count*"), text("0"));

        ox.eval(&mut editor, "(reset! count 5)").unwrap();
        settle(&mut editor, &mut ox);

        assert_eq!(shown(&editor, "*count*"), text("5"));
    }

    #[test]
    fn buttons_call_their_functions() {
        let (mut editor, mut ox) = opened(
            r#"(def count (atom 0)) (open-view "*count*" (fn [] (row (text @count) (button "More" :on-press (fn [] (swap! count inc))))))"#,
        );
        let Node::Row { children, .. } = shown(&editor, "*count*") else {
            panic!("not a row");
        };
        assert_eq!(
            children[1],
            Node::Button {
                label: "More".to_owned(),
                press: Some(Press::Slot(0)),
            }
        );
        let id = editor.buffer_named("*count*").unwrap();

        ox.press(&mut editor, id, 0);
        ox.press(&mut editor, id, 0);
        settle(&mut editor, &mut ox);

        assert_eq!(
            shown(&editor, "*count*"),
            Node::Row {
                spacing: 0.0,
                children: vec![
                    text("2"),
                    Node::Button {
                        label: "More".to_owned(),
                        press: Some(Press::Slot(0)),
                    },
                ],
            }
        );
    }

    #[test]
    fn view_buffers_have_their_own_keys() {
        let (mut editor, mut ox) = opened(
            r#"(defcommand bump "Bump" (message "bumped")) (open-view "*keys*" (fn [] (text "k")) :keys {b: 'bump "C-c x" 'bump})"#,
        );

        for notation in ["b", "C-c x"] {
            editor.show_message("");
            keys(&mut editor, notation);
            settle(&mut editor, &mut ox);
            assert_eq!(editor.message(), Some("bumped"), "{notation}");
        }
    }

    #[test]
    fn opening_a_view_again_replaces_its_function() {
        let (mut editor, mut ox) = opened(r#"(open-view "*v*" (fn [] (text "first")))"#);

        ox.eval(&mut editor, r#"(open-view "*v*" (fn [] (text "second")))"#)
            .unwrap();
        settle(&mut editor, &mut ox);

        assert_eq!(shown(&editor, "*v*"), text("second"));
        assert_eq!(
            editor
                .buffer_ids()
                .into_iter()
                .filter(|id| editor.buffer_by_id(*id).is_some_and(|b| b.name() == "*v*"))
                .count(),
            1
        );
    }

    #[test]
    fn mistakes_in_views_are_logged() {
        let (editor, _) = opened(
            r#"(open-view "*odd*" (fn [] 5)) (open-view "*face*" (fn [] (text "x" :face :nope)))"#,
        );

        let log = logged(&editor);
        assert!(log.contains("A view must return a widget, got 5"), "{log}");
        assert!(log.contains("text doesn't know the face :nope"), "{log}");
    }
}
