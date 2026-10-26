use std::sync::Arc;

use ox_lang::heap::{Heap, Hold};
use ox_lang::native::expected;
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::Editor;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::{Highlight, NICK_COLORS};
use oxmacs_core::view::{Arg, Node, Picture, Pixels, ViewBuffer};

use crate::ide::log;
use crate::view::shapes;

const UI: &str = "ox/ui";

#[derive(Debug)]
struct ImagePixels(Arc<ox_library::image::Image>);

impl Pixels for ImagePixels {
    fn size(&self) -> (u32, u32) {
        self.0.pixels.dimensions()
    }

    fn rgba(&self) -> &[u8] {
        self.0.pixels.as_raw()
    }

    fn key(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }

    fn unused(&self) -> bool {
        Arc::strong_count(&self.0) == 1
    }
}

pub(crate) fn picture(image: Arc<ox_library::image::Image>) -> Picture {
    Picture(Arc::new(ImagePixels(image)))
}

#[derive(Debug)]
struct Open {
    name: String,
    function: Hold,
    callbacks: Vec<Hold>,
    buffer: BufferId,
    scope: ScopeId,
    resized: Option<Hold>,
    size: Option<(u32, u32)>,
    failed: Option<String>,
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

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "view sizes are small pixel counts"
)]
fn size(heap: &Heap, name: &str, value: Value) -> Result<f32, Error> {
    match value {
        Value::Int(number) => Ok(number as f32),
        Value::Float(number) => Ok(number as f32),
        _ => Err(expected(heap, name, "a number", value)),
    }
}

fn size_given(heap: &Heap, name: &str, value: Value) -> Result<Option<f32>, Error> {
    match value {
        Value::Nil => Ok(None),
        given => size(heap, name, given).map(Some),
    }
}

pub(crate) fn pressed(heap: &mut Heap, value: Value) -> Option<Hold> {
    (value != Value::Nil).then(|| heap.hold(value))
}

fn children(
    heap: &Heap,
    name: &str,
    values: &[Value],
    out: &mut Vec<Node<Hold>>,
) -> Result<(), Error> {
    for value in values {
        if *value == Value::Nil {
            continue;
        }
        if let Some(node) = heap.foreign_of::<Node<Hold>>(*value) {
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
                print::brief(heap, *value)
            )));
        }
    }
    Ok(())
}

fn widget(heap: &mut Heap, name: &str, node: Node<Hold>) -> Result<Value, Error> {
    heap.foreign(name, node).map_err(Error::from)
}

fn stack(heap: &mut Heap, name: &str, arguments: &[Value], row: bool) -> Result<Value, Error> {
    let spacing = size_given(heap, name, arguments[0])?.unwrap_or(0.0);
    let mut nodes = Vec::new();
    children(heap, name, &arguments[1..], &mut nodes)?;
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
    let face = match arguments[1] {
        Value::Nil => None,
        Value::Keyword(name) if face_named(name.name()).is_some() => face_named(name.name()),
        other => {
            return Err(Error::new(format!(
                "text doesn't know the face {}",
                print::brief(heap, other)
            )));
        }
    };
    let text_size = size_given(heap, "text", arguments[2])?;
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
    let press = pressed(heap, arguments[1]);
    widget(heap, "button", Node::Button { label, press })
}

fn image(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let image = ox_library::image::image_of(heap, arguments[0])
        .ok_or_else(|| expected(heap, "image", "an image", arguments[0]))?;
    let width = size_given(heap, "image", arguments[1])?;
    let height = size_given(heap, "image", arguments[2])?;
    let selected = arguments[4].truthy();
    let press = pressed(heap, arguments[3]);
    widget(
        heap,
        "image",
        Node::Image {
            picture: picture(image),
            width,
            height,
            press,
            selected,
        },
    )
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(UI, "column")
        .arity(0, None)
        .keywords(&["spacing"])
        .function(|heap, arguments| stack(heap, "column", arguments, false));
    isolate
        .native(UI, "row")
        .arity(0, None)
        .keywords(&["spacing"])
        .function(|heap, arguments| stack(heap, "row", arguments, true));
    isolate
        .native(UI, "text")
        .arity(1, Some(1))
        .keywords(&["face", "size"])
        .function(text);
    isolate
        .native(UI, "button")
        .arity(1, Some(1))
        .keywords(&["on-press"])
        .function(button);
    isolate
        .native(UI, "space")
        .arity(0, Some(1))
        .function(|heap, arguments| {
            let size = match arguments.first() {
                Some(value) => Some(size(heap, "space", *value)?),
                None => None,
            };
            widget(heap, "space", Node::Space { size })
        });
    isolate
        .native(UI, "scrollable")
        .arity(1, Some(1))
        .function(|heap, arguments| {
            let mut nodes = Vec::new();
            children(heap, "scrollable", &arguments[..1], &mut nodes)?;
            let child = nodes
                .into_iter()
                .next()
                .unwrap_or(Node::Space { size: None });
            widget(heap, "scrollable", Node::Scrollable(Box::new(child)))
        });
    isolate
        .native(UI, "canvas")
        .arity(1, Some(1))
        .keywords(&["height", "relative"])
        .function(|heap, arguments| {
            let drawn = shapes::shapes(heap, arguments[0])?;
            let height = size_given(heap, "canvas", arguments[1])?;
            let relative = arguments[2].truthy();
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
    isolate
        .native(UI, "image")
        .arity(1, Some(1))
        .keywords(&["width", "height", "on-press", "selected"])
        .function(image);
    isolate
        .native("ox", "open-view")
        .arity(2, Some(2))
        .keywords(&OPEN_VIEW)
        .host::<Editor>(open);
}

fn keys(heap: &Heap, value: Value) -> Result<Vec<(String, String)>, Error> {
    let name = |value: Value| match value {
        Value::Keyword(name) | Value::Symbol(name) => name.name().to_owned(),
        _ => print::display(heap, value),
    };
    heap.entries(value)
        .ok_or_else(|| {
            expected(
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

pub(crate) fn buffer_name(
    heap: &Heap,
    editor: &Editor,
    native: &str,
    value: Value,
) -> Result<String, Error> {
    crate::api::handles::name(heap, editor, native, value)
}

const OPEN_VIEW: [&str; 6] = ["keys", "beside", "select", "show", "on-resize", "watch"];

fn open(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = buffer_name(heap, editor, "open-view", arguments[0])?;
    let [keyed, beside, select, show, resized, watched] = [
        arguments[2],
        arguments[3],
        arguments[4],
        arguments[5],
        arguments[6],
        arguments[7],
    ];
    let bindings = match keyed {
        Value::Nil => Vec::new(),
        keyed => keys(heap, keyed)?,
    };
    let (beside, select) = (beside.truthy(), select.truthy());
    let show = show == Value::Nil || show.truthy();
    let resized = (resized != Value::Nil).then_some(resized);
    let watched =
        (watched != Value::Nil).then(|| std::path::PathBuf::from(print::display(heap, watched)));
    let mut views = editor.take_plugin_state::<Views>().unwrap_or_default();
    let (alive, closed): (Vec<Open>, Vec<Open>) = std::mem::take(&mut views.open)
        .into_iter()
        .partition(|view| editor.buffer_by_id(view.buffer).is_some());
    for view in closed {
        heap.release(view.function);
        for callback in view.callbacks.into_iter().chain(view.resized) {
            heap.release(callback);
        }
    }
    views.open = alive;
    let function = heap.hold(arguments[1]);
    let resized = resized.map(|handler| heap.hold(handler));
    let index = if let Some(index) = views.open.iter().position(|view| view.name == name) {
        heap.release(views.open[index].function);
        views.open[index].function = function;
        views.open[index].failed = None;
        if let Some(resized) = resized
            && let Some(old) = views.open[index].resized.replace(resized)
        {
            heap.release(old);
        }
        index
    } else {
        let scope = editor.add_scope(Box::leak(name.clone().into_boxed_str()));
        let buffer = editor.add_plugin_buffer_with(&name, |buffer| {
            buffer.set_read_only(true);
            ViewBuffer {
                scope,
                watched: None,
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
            resized,
            size: None,
            failed: None,
        });
        views.open.len() - 1
    };
    let (buffer, scope) = (views.open[index].buffer, views.open[index].scope);
    views.dirty = true;
    editor.set_plugin_state(views);
    if let Some((_, view)) = editor.plugin_buffer_mut::<ViewBuffer>(buffer) {
        view.watched = watched;
    }
    for (keys, command) in bindings {
        let id = editor
            .command_named(&command)
            .ok_or_else(|| Error::new(format!("There is no command named {command}")))?;
        editor
            .bind(scope, &keys, id)
            .map_err(|error| Error::new(format!("{keys}: {error}")))?;
    }
    if show {
        crate::api::buffers::reveal(editor, buffer, beside, select);
    }
    Ok(Value::Nil)
}

pub fn sized(isolate: &mut Isolate, editor: &mut Editor, buffer: BufferId, size: (u32, u32)) {
    let Some(view) = editor
        .plugin_state_mut::<Views>()
        .and_then(|views| views.open.iter_mut().find(|view| view.buffer == buffer))
    else {
        return;
    };
    if view.size == Some(size) {
        return;
    }
    view.size = Some(size);
    let Some(handler) = view.resized.and_then(|hold| isolate.held(hold)) else {
        return;
    };
    let arguments = [Value::Int(i64::from(size.0)), Value::Int(i64::from(size.1))];
    if let Err(error) = isolate.spawn(handler, &arguments) {
        log::report(editor, &error, None);
    }
}

fn number(node: Node<Hold>, callbacks: &mut Vec<Hold>) -> Node<usize> {
    node.map(&mut |hold| {
        callbacks.push(hold);
        callbacks.len() - 1
    })
}

fn render(
    isolate: &mut Isolate,
    editor: &mut Editor,
    function: Hold,
) -> Result<(Node<usize>, Vec<Hold>), Error> {
    let function = isolate
        .held(function)
        .ok_or_else(|| Error::new("A view lost its function"))?;
    let value = isolate.within(crate::QUICK, |isolate| {
        isolate.call_with(editor, function, &[])
    })?;
    let node = isolate
        .heap()
        .foreign_of::<Node<Hold>>(value)
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

fn newly_failed(editor: &mut Editor, buffer: BufferId, message: &str) -> bool {
    let Some(view) = editor
        .plugin_state_mut::<Views>()
        .and_then(|views| views.open.iter_mut().find(|view| view.buffer == buffer))
    else {
        return true;
    };
    if view.failed.as_deref() == Some(message) {
        return false;
    }
    view.failed = Some(message.to_owned());
    true
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
                if newly_failed(editor, buffer, &error.message) {
                    log::report(editor, &error, None);
                }
                continue;
            }
        };
        if let Some(view) = editor
            .plugin_state_mut::<Views>()
            .and_then(|views| views.open.iter_mut().find(|view| view.buffer == buffer))
        {
            view.failed = None;
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

fn value(heap: &mut Heap, arg: &Arg) -> Result<Value, Error> {
    match arg {
        Arg::Nil => Ok(Value::Nil),
        Arg::Int(number) => Ok(Value::Int(*number)),
        Arg::Text(text) => heap.string(text).map_err(Error::from),
        Arg::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(value(heap, item)?);
            }
            heap.vector_from(&values).map_err(Error::from)
        }
    }
}

pub fn call(
    isolate: &mut Isolate,
    editor: &mut Editor,
    buffer: BufferId,
    slot: usize,
    args: &[Arg],
) {
    let callback = editor
        .plugin_state::<Views>()
        .and_then(|views| views.open.iter().find(|view| view.buffer == buffer))
        .and_then(|view| view.callbacks.get(slot).copied())
        .and_then(|hold| isolate.held(hold));
    let Some(callback) = callback else {
        return;
    };
    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        match value(isolate.heap_mut(), arg) {
            Ok(made) => values.push(made),
            Err(error) => {
                log::report(editor, &error, None);
                return;
            }
        }
    }
    if let Err(error) = isolate.spawn(callback, &values) {
        log::report(editor, &error, None);
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
    use oxmacs_core::view::UiEvent;

    use crate::testing::{keys, settle};
    use crate::{Ox, ide::log};
    use oxmacs_core::view::{Node, ViewBuffer};

    const UI: &str = "(module (import (ox ui :only [column row text button space scrollable])))";

    fn shown(editor: &Editor, name: &str) -> Node<usize> {
        let id = editor.buffer_named(name).unwrap();
        editor
            .plugin_buffer::<ViewBuffer>(id)
            .unwrap()
            .1
            .node
            .clone()
    }

    fn text(value: &str) -> Node<usize> {
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
            r#"(open-view "*hello*" (fn [] (column (text "Hello" :face :heading :size 18) (row (text 1) nil [(text "a") (text "b")]) (space) (space 8) (scrollable (text "long")) :spacing 4)))"#,
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
    fn text_with_no_face_is_plain() {
        let (editor, _) = opened(
            r#"(open-view "*plain*" (fn [] (row (text "a" :face nil) (text "b" :face (when false :heading)))))"#,
        );

        assert_eq!(
            shown(&editor, "*plain*"),
            Node::Row {
                spacing: 0.0,
                children: vec![text("a"), text("b")],
            }
        );
        assert_eq!(logged(&editor), "");
    }

    #[test]
    fn views_can_open_beside_with_or_without_the_focus() {
        let (mut editor, mut ox) =
            opened(r#"(open-view "*side*" (fn [] (text "x")) :beside true)"#);
        let side = editor.buffer_named("*side*").unwrap();
        assert_eq!(editor.buffer().name(), "*scratch*");
        assert!(editor.buffer_shown(side));

        ox.eval(
            &mut editor,
            r#"(open-view "*side*" (fn [] (text "y")) :beside true :select true)"#,
        )
        .unwrap();

        assert_eq!(editor.buffer().name(), "*side*");
        assert_eq!(editor.tiling().windows().len(), 2);
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
    fn images_show_at_their_size_or_the_size_given() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(
            &mut editor,
            r#"(module (import (ox ui) (ox image))) (def dot (image/new 4 2 :color :red)) (open-view "*picture*" (fn [] (ui/column (ui/image dot) (ui/image dot :width 40 :height 20))))"#,
        )
        .unwrap();
        settle(&mut editor, &mut ox);

        let Node::Column { children, .. } = shown(&editor, "*picture*") else {
            panic!("not a column");
        };
        let sizes: Vec<(u32, u32, Option<f32>, Option<f32>)> = children
            .iter()
            .map(|child| match child {
                Node::Image {
                    picture,
                    width,
                    height,
                    ..
                } => (picture.size().0, picture.size().1, *width, *height),
                other => panic!("not an image: {other:?}"),
            })
            .collect();
        assert_eq!(sizes, [(4, 2, None, None), (4, 2, Some(40.0), Some(20.0))]);
        let Node::Image { picture, .. } = &children[0] else {
            panic!("not an image");
        };
        assert_eq!(
            children[0],
            Node::Image {
                picture: picture.clone(),
                width: None,
                height: None,
                press: None,
                selected: false,
            }
        );
        assert!(
            ox.eval(&mut editor, "(module (import (ox ui))) (ui/image 5)")
                .unwrap_err()
                .message
                .contains("image expects an image, got 5")
        );
    }

    #[test]
    fn images_can_be_pressed_and_marked_selected() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(
            &mut editor,
            r#"(module (import (ox ui) (ox image))) (def pressed (atom 0)) (def dot (image/new 2 2)) (open-view "*thumbs*" (fn [] (ui/row (ui/image dot :selected true :on-press (fn [] (swap! pressed inc))) (ui/image dot))))"#,
        )
        .unwrap();
        settle(&mut editor, &mut ox);

        let Node::Row { children, .. } = shown(&editor, "*thumbs*") else {
            panic!("not a row");
        };
        let marks: Vec<(Option<usize>, bool)> = children
            .iter()
            .map(|child| match child {
                Node::Image {
                    press, selected, ..
                } => (*press, *selected),
                other => panic!("not an image: {other:?}"),
            })
            .collect();
        assert_eq!(marks, [(Some(0), true), (None, false)]);
        let id = editor.buffer_named("*thumbs*").unwrap();
        ox.event(
            &mut editor,
            UiEvent::Press {
                buffer: id,
                slot: 0,
            },
        );
        settle(&mut editor, &mut ox);
        assert_eq!(ox.eval(&mut editor, "@pressed").unwrap(), "1");
    }

    #[test]
    fn views_open_unseen_and_hear_their_size() {
        let (mut editor, mut ox) = opened(
            r#"(def sizes (atom [])) (open-view "*sized*" (fn [] (text "x")) :show false :on-resize (fn [width height] (swap! sizes conj [width height])))"#,
        );
        let id = editor.buffer_named("*sized*").unwrap();
        assert!(!editor.buffer_shown(id));

        for (width, height) in [(800, 600), (800, 600), (640, 600)] {
            ox.event(
                &mut editor,
                UiEvent::Sized {
                    buffer: id,
                    width,
                    height,
                },
            );
            settle(&mut editor, &mut ox);
        }

        assert_eq!(
            ox.eval(&mut editor, "@sizes").unwrap(),
            "[[800 600] [640 600]]"
        );
    }

    #[test]
    fn a_view_that_never_finishes_is_stopped_and_reported() {
        let (editor, _) =
            opened(r#"(def (spin) (spin)) (open-view "*spin*" (fn [] (spin) (text "never")))"#);

        assert!(
            logged(&editor).contains("took longer than 250 ms"),
            "{}",
            logged(&editor)
        );
    }

    #[test]
    fn a_view_stuck_the_same_way_is_reported_once_until_it_draws_again() {
        let (mut editor, mut ox) = opened(
            r#"(def stuck (atom true)) (def (spin) (spin)) (open-view "*maybe*" (fn [] (when @stuck (spin)) (text "ok")))"#,
        );
        let reports = |editor: &Editor| logged(editor).matches("took longer").count();
        assert_eq!(reports(&editor), 1);

        ox.eval(&mut editor, "(reset! stuck :still)").unwrap();
        settle(&mut editor, &mut ox);
        assert_eq!(reports(&editor), 1);

        ox.eval(&mut editor, "(reset! stuck false)").unwrap();
        settle(&mut editor, &mut ox);
        assert_eq!(shown(&editor, "*maybe*"), text("ok"));
        ox.eval(&mut editor, "(reset! stuck true)").unwrap();
        settle(&mut editor, &mut ox);
        assert_eq!(reports(&editor), 2);
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
                press: Some(0),
            }
        );
        let id = editor.buffer_named("*count*").unwrap();

        ox.event(
            &mut editor,
            UiEvent::Press {
                buffer: id,
                slot: 0,
            },
        );
        ox.event(
            &mut editor,
            UiEvent::Press {
                buffer: id,
                slot: 0,
            },
        );
        settle(&mut editor, &mut ox);

        assert_eq!(
            shown(&editor, "*count*"),
            Node::Row {
                spacing: 0.0,
                children: vec![
                    text("2"),
                    Node::Button {
                        label: "More".to_owned(),
                        press: Some(0),
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
