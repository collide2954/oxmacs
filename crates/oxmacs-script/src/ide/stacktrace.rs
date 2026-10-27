use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use ox_lang::Error;
use ox_lang::error::{ErrorKind, Frame};
use ox_lang::symbol::Sym;
use oxmacs_core::command::{Command, DynamicSpec, ScopeId};
use oxmacs_core::editor::Editor;
use oxmacs_core::editor::Visit;
use oxmacs_core::special_buffer::SpecialBuffer;
use oxmacs_core::text::BufferId;

pub const STACKTRACE: &str = "*ox stacktrace*";
const FIRST_FRAME: usize = 3;

#[derive(Debug, Clone)]
pub(crate) struct Place {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub text: Option<Arc<str>>,
}

#[derive(Debug)]
struct PlacesView {
    scope: ScopeId,
    first: usize,
    places: Vec<Place>,
}

fn places(frames: &[Frame]) -> Vec<Place> {
    let mut texts: Vec<(&str, Arc<str>)> = Vec::new();
    frames
        .iter()
        .map(|frame| {
            let file = frame.file();
            let text = frame
                .source
                .as_ref()
                .filter(|_| !Path::new(file).is_file())
                .map(|source| {
                    if let Some((_, text)) = texts.iter().find(|(name, _)| *name == file) {
                        return Arc::clone(text);
                    }
                    let text: Arc<str> = source.text.as_str().into();
                    texts.push((file, Arc::clone(&text)));
                    text
                });
            let (line, column) = frame.line_column();
            Place {
                file: file.to_owned(),
                line,
                column,
                text,
            }
        })
        .collect()
}

impl SpecialBuffer for PlacesView {
    fn scope(&self) -> ScopeId {
        self.scope
    }
}

#[derive(Debug)]
struct SourceView {
    scope: ScopeId,
}

impl SpecialBuffer for SourceView {
    fn scope(&self) -> ScopeId {
        self.scope
    }
}

#[derive(Debug, Clone, Copy)]
struct Scopes {
    trace: ScopeId,
    source: ScopeId,
}

fn scopes(editor: &mut Editor) -> Scopes {
    if let Some(scopes) = editor.plugin_state::<Scopes>() {
        return *scopes;
    }
    let trace = editor.add_scope("ox places");
    let source = editor.add_scope("ox source");
    let visit = editor.add_dynamic_command(DynamicSpec {
        name: "ox-visit-place".to_owned(),
        summary: "Visit the place on this line".to_owned(),
        scope: trace,
        run: Arc::new(visit),
    });
    for (keys, command) in [
        ("n", Command::NextLine.into()),
        ("p", Command::PreviousLine.into()),
        ("q", Command::QuitWindow.into()),
        ("RET", visit),
    ] {
        editor.bind(trace, keys, command).ok();
    }
    editor.bind(source, "q", Command::QuitWindow).ok();
    let scopes = Scopes { trace, source };
    editor.set_plugin_state(scopes);
    scopes
}

pub(crate) fn shown_file(file: &str) -> &str {
    let path = Path::new(file);
    if path.is_absolute() {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(file)
    } else {
        file
    }
}

fn render(error: &Error) -> String {
    let kind = ErrorKind::NAMES
        .iter()
        .find(|(kind, _)| *kind == error.kind)
        .map_or("error", |(_, name)| *name);
    let mut text = format!("{}\n{kind}\n\n", error.message);
    let names: Vec<&str> = error
        .traceback
        .iter()
        .map(|frame| frame.function.map_or("top level", Sym::name))
        .collect();
    let width = names
        .iter()
        .map(|name| name.chars().count())
        .max()
        .unwrap_or(0);
    for (name, frame) in names.iter().zip(&error.traceback) {
        let (line, column) = frame.line_column();
        let file = shown_file(frame.file());
        let _ = writeln!(text, "  {name:<width$}  {file}:{line}:{column}");
    }
    text
}

pub fn show(editor: &mut Editor, error: &Error) {
    if error.traceback.len() < 2 {
        return;
    }
    let places = places(&error.traceback);
    show_places(editor, STACKTRACE, &render(error), FIRST_FRAME, places);
}

pub(crate) fn show_places(
    editor: &mut Editor,
    name: &str,
    text: &str,
    first: usize,
    places: Vec<Place>,
) {
    let scope = scopes(editor).trace;
    let existing = editor
        .buffer_named(name)
        .filter(|id| editor.special_buffer::<PlacesView>(*id).is_some());
    let id = if let Some(id) = existing {
        if let Some((_, view)) = editor.special_buffer_mut::<PlacesView>(id) {
            view.places = places;
            view.first = first;
        }
        id
    } else {
        editor.add_special_buffer_with(name, |buffer| {
            buffer.set_read_only(true);
            PlacesView {
                scope,
                first,
                places,
            }
        })
    };
    editor.fill_buffer(id, text);
    if let Some((buffer, _)) = editor.special_buffer_mut::<PlacesView>(id) {
        let start = buffer.line_start(first);
        buffer.set_point(start);
    }
    editor.show_buffer_beside(id);
}

fn source_buffer(editor: &mut Editor, name: &str, text: &str) -> BufferId {
    let scope = scopes(editor).source;
    editor.add_special_buffer_with(name, |buffer| {
        buffer.insert_at(0, text);
        buffer.set_read_only(true);
        SourceView { scope }
    })
}

fn visit(editor: &mut Editor) {
    let buffer = editor.buffer();
    let line = buffer.line_of(buffer.point());
    let Some(place) = editor
        .special_buffer::<PlacesView>(buffer.id())
        .and_then(|(_, view)| {
            let index = line.checked_sub(view.first)?;
            view.places.get(index).cloned()
        })
    else {
        return;
    };
    editor.execute(Command::OtherWindow);
    if !open_place(
        editor,
        &place.file,
        (place.line, place.column),
        place.text.as_deref(),
    ) {
        editor.show_message(format!("Can't find {}", place.file));
    }
}

pub(crate) fn open_place(
    editor: &mut Editor,
    file: &str,
    (line, column): (usize, usize),
    text: Option<&str>,
) -> bool {
    if Path::new(file).is_file() {
        editor.visit(&Visit {
            path: file.into(),
            line: Some(line),
            column: Some(column),
        });
        return true;
    }
    let target = match (editor.buffer_named(file), text) {
        (Some(id), _) => id,
        (None, Some(text)) => source_buffer(editor, file, text),
        (None, None) => return false,
    };
    editor.show_buffer(target);
    editor.go_to_line(line, column);
    true
}
