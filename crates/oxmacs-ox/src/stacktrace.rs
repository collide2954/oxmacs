use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use ox::Error;
use ox::error::{ErrorKind, Frame};
use ox::symbol::Sym;
use oxmacs_core::cli::Visit;
use oxmacs_core::command::{Command, DynamicSpec, ScopeId};
use oxmacs_core::editor::Editor;
use oxmacs_core::plugin::PluginBuffer;
use oxmacs_core::text::BufferId;

pub const STACKTRACE: &str = "*ox stacktrace*";
const FIRST_FRAME: usize = 3;

#[derive(Debug, Clone)]
struct Place {
    file: String,
    line: usize,
    column: usize,
    text: Option<Arc<str>>,
}

#[derive(Debug)]
struct TraceView {
    scope: ScopeId,
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

impl PluginBuffer for TraceView {
    fn scope(&self) -> ScopeId {
        self.scope
    }
}

#[derive(Debug)]
struct SourceView {
    scope: ScopeId,
}

impl PluginBuffer for SourceView {
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
    let trace = editor.add_scope("ox stacktrace");
    let source = editor.add_scope("ox source");
    let visit = editor.add_dynamic_command(DynamicSpec {
        name: "ox-stacktrace-visit".to_owned(),
        summary: "Visit the source of the frame at point".to_owned(),
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
    let scope = scopes(editor).trace;
    let places = places(&error.traceback);
    let id = if let Some(id) = editor.plugin_buffers::<TraceView>().first().copied() {
        if let Some((_, view)) = editor.plugin_buffer_mut::<TraceView>(id) {
            view.places = places;
        }
        id
    } else {
        editor.add_plugin_buffer_with(STACKTRACE, |buffer| {
            buffer.set_read_only(true);
            TraceView { scope, places }
        })
    };
    editor.fill_buffer(id, &render(error));
    if let Some((buffer, _)) = editor.plugin_buffer_mut::<TraceView>(id) {
        let first = buffer.line_start(FIRST_FRAME);
        buffer.set_point(first);
    }
    editor.show_buffer_beside(id);
}

fn source_buffer(editor: &mut Editor, name: &str, text: &str) -> BufferId {
    let scope = scopes(editor).source;
    editor.add_plugin_buffer_with(name, |buffer| {
        buffer.insert_at(0, text);
        buffer.set_read_only(true);
        SourceView { scope }
    })
}

fn visit(editor: &mut Editor) {
    let buffer = editor.buffer();
    let index = buffer.line_of(buffer.point()).checked_sub(FIRST_FRAME);
    let Some(place) = index.and_then(|index| {
        editor
            .plugin_buffer::<TraceView>(buffer.id())
            .and_then(|(_, view)| view.places.get(index).cloned())
    }) else {
        return;
    };
    editor.execute(Command::OtherWindow);
    if Path::new(&place.file).is_file() {
        editor.visit(&Visit {
            path: place.file.into(),
            line: Some(place.line),
            column: Some(place.column),
        });
        return;
    }
    let target = match (editor.buffer_named(&place.file), &place.text) {
        (Some(id), _) => id,
        (None, Some(text)) => source_buffer(editor, &place.file, text),
        (None, None) => {
            editor.show_message(format!("Can't find {}", place.file));
            return;
        }
    };
    editor.show_buffer(target);
    editor.go_to_line(place.line, place.column);
}
