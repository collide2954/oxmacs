use std::ops::Range;
use std::path::{Path, PathBuf};

use ox::heap::{Heap, Hold};
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::command::{Command, ScopeId};
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::{Editor, InputLine};
use oxmacs_core::plugin::PluginBuffer;
use oxmacs_core::text::{Buffer, BufferId};
use oxmacs_core::theme::Highlight;

use crate::views::face_named;

type Pills = Vec<(Range<usize>, Highlight)>;

#[derive(Debug)]
struct Chat {
    input: InputLine,
    prompt: String,
}

#[derive(Debug)]
pub struct LinesBuffer {
    scope: ScopeId,
    styles: Vec<LineStyle>,
    pills: Vec<Pills>,
    directory: Option<PathBuf>,
    watched: Option<PathBuf>,
    moved: Option<Hold>,
    resized: Option<Hold>,
    seen_line: Option<usize>,
    seen_width: Option<usize>,
    unsaved: bool,
    chat: Option<Chat>,
}

impl PluginBuffer for LinesBuffer {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn input_line(&mut self) -> Option<&mut InputLine> {
        self.chat.as_mut().map(|chat| &mut chat.input)
    }

    fn directory(&self) -> Option<&Path> {
        self.directory.as_deref()
    }

    fn watched(&self) -> Option<&Path> {
        self.watched.as_deref().or(self.directory.as_deref())
    }

    fn fixed_name(&self) -> bool {
        true
    }

    fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    fn pills(&self, line: usize) -> Pills {
        self.pills.get(line).cloned().unwrap_or_default()
    }

    fn unsaved(&self) -> bool {
        self.unsaved
    }
}

struct Rendered {
    text: String,
    styles: Vec<LineStyle>,
    pills: Vec<Pills>,
}

struct Line {
    column: usize,
    style: LineStyle,
    pills: Pills,
}

fn segment(
    heap: &Heap,
    name: &str,
    value: Value,
    text: &mut String,
    line: &mut Line,
) -> Result<(), Error> {
    let parts = heap
        .text(value)
        .is_none()
        .then(|| heap.elements(value))
        .flatten();
    let (shown, face, pill) = match parts.as_deref() {
        Some([shown, face]) => (*shown, Some(*face), false),
        Some([shown, face, Value::Keyword(pill)]) if pill.name() == "pill" => {
            (*shown, Some(*face), true)
        }
        Some(_) => {
            return Err(Error::new(format!(
                "{name} expects text, [text face] or [text face :pill] in a line, got {}",
                print::brief(heap, value)
            )));
        }
        None => (value, None, false),
    };
    let shown = print::display(heap, shown);
    let start = line.column;
    line.column += shown.chars().count();
    text.push_str(&shown);
    if let Some(given) = face {
        let highlight = self::face(heap, name, given)?;
        line.style.spans.push((start..line.column, highlight));
        if pill {
            line.pills.push((start..line.column, highlight));
        }
    }
    Ok(())
}

fn face(heap: &Heap, name: &str, value: Value) -> Result<Highlight, Error> {
    match value {
        Value::Keyword(face) => face_named(face.name()),
        _ => None,
    }
    .ok_or_else(|| {
        Error::new(format!(
            "{name} doesn't know the face {}",
            print::brief(heap, value)
        ))
    })
}

fn render(heap: &Heap, name: &str, lines: Value) -> Result<Rendered, Error> {
    let lines = heap.elements(lines).ok_or_else(|| {
        Error::new(format!(
            "{name} expects a vector of lines, got {}",
            print::brief(heap, lines)
        ))
    })?;
    let mut text = String::new();
    let mut styles = Vec::with_capacity(lines.len());
    let mut pills = Vec::with_capacity(lines.len());
    for (index, given) in lines.into_iter().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        let mut line = Line {
            column: 0,
            style: LineStyle::default(),
            pills: Vec::new(),
        };
        let mut shown = given;
        if let Some(fields) = heap.entries(given) {
            shown = Value::Nil;
            for (key, value) in fields {
                match key {
                    Value::Keyword(key) if key.name() == "line" => shown = value,
                    Value::Keyword(key) if key.name() == "background" && value != Value::Nil => {
                        line.style.background = Some(face(heap, name, value)?);
                    }
                    _ => {}
                }
            }
        }
        match heap
            .text(shown)
            .is_none()
            .then(|| heap.elements(shown))
            .flatten()
        {
            Some(segments) => {
                for part in segments {
                    segment(heap, name, part, &mut text, &mut line)?;
                }
            }
            None => text.push_str(&print::display(heap, shown)),
        }
        styles.push(line.style);
        pills.push(line.pills);
    }
    Ok(Rendered {
        text,
        styles,
        pills,
    })
}

fn plugin_buffer(editor: &Editor, name: &str) -> Option<BufferId> {
    editor
        .plugin_buffers::<LinesBuffer>()
        .into_iter()
        .find(|id| {
            editor
                .buffer_by_id(*id)
                .is_some_and(|buffer| buffer.name() == name)
        })
}

fn prompt_start(buffer: &Buffer, chat: &Chat) -> usize {
    chat.input
        .start(buffer)
        .unwrap_or_else(|| buffer.len_chars())
        .saturating_sub(chat.prompt.chars().count())
}

fn prompt_style(prompt: &str) -> LineStyle {
    LineStyle {
        background: None,
        spans: vec![(0..prompt.chars().count(), Highlight::Keyword)],
    }
}

fn transcript(rendered: &Rendered) -> String {
    if rendered.styles.is_empty() {
        String::new()
    } else {
        format!("{}\n", rendered.text)
    }
}

fn replace_unrecorded(buffer: &mut Buffer, end: usize, text: &str) {
    let old = buffer.text_in(0..end);
    if old == text {
        return;
    }
    let prefix = old
        .chars()
        .zip(text.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = old
        .chars()
        .rev()
        .zip(text.chars().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(end - prefix)
        .min(text.chars().count() - prefix);
    buffer.delete_unrecorded(prefix..end - suffix);
    let middle: String = text
        .chars()
        .skip(prefix)
        .take(text.chars().count() - prefix - suffix)
        .collect();
    buffer.insert_unrecorded(prefix, &middle);
}

fn start_chat(editor: &mut Editor, id: BufferId, prompt: String) {
    let Some((buffer, view)) = editor.plugin_buffer_mut::<LinesBuffer>(id) else {
        return;
    };
    if let Some(chat) = &mut view.chat {
        if chat.prompt != prompt {
            let from = prompt_start(buffer, chat);
            let from_end = (buffer.point() >= from).then(|| buffer.len_chars() - buffer.point());
            buffer.delete_unrecorded(from..from + chat.prompt.chars().count());
            buffer.insert_unrecorded(from, &prompt);
            buffer.set_marker(chat.input.mark(), from + prompt.chars().count());
            if let Some(from_end) = from_end {
                buffer.set_point(buffer.len_chars() - from_end);
            }
            if let Some(style) = view.styles.last_mut() {
                *style = prompt_style(&prompt);
            }
            chat.prompt = prompt;
        }
        return;
    }
    buffer.set_read_only(false);
    let end = buffer.len_chars();
    let added = if end > 0 {
        format!("\n{prompt}")
    } else {
        prompt.clone()
    };
    buffer.insert_unrecorded(end, &added);
    let input = InputLine::new(buffer);
    buffer.set_point(buffer.len_chars());
    view.styles.push(prompt_style(&prompt));
    view.pills.push(Vec::new());
    view.chat = Some(Chat { input, prompt });
    let scope = view.scope;
    editor.bind(scope, "M-p", Command::ComintPreviousInput).ok();
    editor.bind(scope, "M-n", Command::ComintNextInput).ok();
}

fn fill_chat(editor: &mut Editor, id: BufferId, rendered: Rendered, placed: Option<usize>) {
    let Some((buffer, view)) = editor.plugin_buffer_mut::<LinesBuffer>(id) else {
        return;
    };
    let Some(chat) = &view.chat else {
        return;
    };
    let end = prompt_start(buffer, chat);
    let prompt = prompt_style(&chat.prompt);
    let point = buffer.point();
    let here = (point < end).then(|| {
        let line = buffer.line_of(point);
        (line, point - buffer.line_start(line))
    });
    let text = transcript(&rendered);
    replace_unrecorded(buffer, end, &text);
    let lines = rendered.styles.len();
    view.styles = rendered.styles;
    view.styles.push(prompt);
    view.pills = rendered.pills;
    view.pills.push(Vec::new());
    let wanted = placed.map(|line| (line, 0)).or(here);
    if let Some((line, column)) = wanted {
        let line = line.min(lines.saturating_sub(1));
        let start = buffer.line_start(line);
        buffer.set_point((start + column).min(buffer.line_end(line)));
    }
}

fn append(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = crate::views::buffer_name(heap, "append-lines", arguments[0])?;
    let id = plugin_buffer(editor, &name)
        .ok_or_else(|| Error::new(format!("There is no plugin buffer named {name}")))?;
    let rendered = render(heap, "append-lines", arguments[1])?;
    if rendered.styles.is_empty() {
        return Ok(Value::Nil);
    }
    let Some((buffer, view)) = editor.plugin_buffer_mut::<LinesBuffer>(id) else {
        return Ok(Value::Nil);
    };
    let (at, text, index) = match &view.chat {
        Some(chat) => (
            prompt_start(buffer, chat),
            transcript(&rendered),
            view.styles.len().saturating_sub(1),
        ),
        None if buffer.len_chars() == 0 && view.styles.is_empty() => (0, rendered.text.clone(), 0),
        None => (
            buffer.len_chars(),
            format!("\n{}", rendered.text),
            view.styles.len(),
        ),
    };
    buffer.insert_unrecorded(at, &text);
    let following = at + text.chars().count();
    view.styles.splice(index..index, rendered.styles);
    view.pills.splice(index..index, rendered.pills);
    if view.chat.is_some() {
        editor.move_points(id, |point, _| (point >= following).then_some(point));
    }
    Ok(Value::Nil)
}

fn chat_of(editor: &mut Editor, id: BufferId) -> Option<(&mut Buffer, &mut Chat)> {
    let (buffer, view) = editor.plugin_buffer_mut::<LinesBuffer>(id)?;
    Some((buffer, view.chat.as_mut()?))
}

fn take_input(heap: &mut Heap, editor: &mut Editor, _: &[Value]) -> Result<Value, Error> {
    let id = editor.buffer().id();
    let Some((buffer, chat)) = chat_of(editor, id) else {
        return Ok(Value::Nil);
    };
    let text = chat.input.text(buffer).unwrap_or_default();
    chat.input.remember(&text);
    let start = chat
        .input
        .start(buffer)
        .unwrap_or_else(|| buffer.len_chars());
    buffer.delete_unrecorded(start..buffer.len_chars());
    buffer.set_point(buffer.len_chars());
    heap.string(&text)
        .map_err(|full| Error::new(full.to_string()))
}

fn input_text(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let id = match arguments.first() {
        Some(name) => editor.buffer_named(&print::display(heap, *name)),
        None => Some(editor.buffer().id()),
    };
    let Some((buffer, chat)) = id.and_then(|id| chat_of(editor, id)) else {
        return Ok(Value::Nil);
    };
    let text = chat.input.text(buffer).unwrap_or_default();
    heap.string(&text)
        .map_err(|full| Error::new(full.to_string()))
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "natives return results even when they cannot fail"
)]
fn input_point(_: &mut Heap, editor: &mut Editor, _: &[Value]) -> Result<Value, Error> {
    let id = editor.buffer().id();
    let Some((buffer, chat)) = chat_of(editor, id) else {
        return Ok(Value::Nil);
    };
    let start = chat
        .input
        .start(buffer)
        .unwrap_or_else(|| buffer.len_chars());
    Ok(buffer
        .point()
        .checked_sub(start)
        .map_or(Value::Nil, |offset| {
            Value::Int(i64::try_from(offset).unwrap_or(i64::MAX))
        }))
}

fn set_input(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = crate::views::buffer_name(heap, "set-input", arguments[0])?;
    let text = print::display(heap, arguments[1]);
    let point = match arguments.get(2..) {
        Some([Value::Keyword(option), value]) if option.name() == "point" => {
            Some(number(heap, *value)?)
        }
        Some([]) | None => None,
        Some(_) => return Err(Error::new("set-input takes :point")),
    };
    let id = editor
        .buffer_named(&name)
        .ok_or_else(|| Error::new(format!("There is no buffer named {name}")))?;
    let Some((buffer, chat)) = chat_of(editor, id) else {
        return Err(Error::new(format!("{name} has no input line")));
    };
    chat.input.replace(buffer, &text);
    if let (Some(point), Some(start)) = (point, chat.input.start(buffer)) {
        buffer.set_point((start + point).min(buffer.len_chars()));
    }
    Ok(Value::Nil)
}

fn fill(editor: &mut Editor, id: BufferId, rendered: Rendered, placed: Option<usize>) {
    if editor
        .plugin_buffer::<LinesBuffer>(id)
        .is_some_and(|(_, view)| view.chat.is_some())
    {
        fill_chat(editor, id, rendered, placed);
        return;
    }
    let Some(buffer) = editor.buffer_by_id(id) else {
        return;
    };
    let here = buffer.line_of(buffer.point());
    let column = buffer.point() - buffer.line_start(here);
    let (line, column) = placed.map_or((here, column), |line| (line, 0));
    if buffer.contents() != rendered.text {
        editor.fill_buffer(id, &rendered.text);
    }
    if let Some((buffer, view)) = editor.plugin_buffer_mut::<LinesBuffer>(id) {
        view.styles = rendered.styles;
        view.pills = rendered.pills;
        let line = line.min(buffer.len_lines().saturating_sub(1));
        let start = buffer.line_start(line);
        buffer.set_point((start + column).min(buffer.line_end(line)));
    }
}

fn keys(heap: &Heap, value: Value) -> Result<Vec<(String, String)>, Error> {
    let name = |value: Value| match value {
        Value::Keyword(name) | Value::Symbol(name) => name.name().to_owned(),
        _ => print::display(heap, value),
    };
    heap.entries(value)
        .map(|entries| {
            entries
                .into_iter()
                .map(|(key, command)| (name(key), name(command)))
                .collect()
        })
        .ok_or_else(|| {
            Error::new(format!(
                "open-buffer expects a map of keys to commands for :keys, got {}",
                print::brief(heap, value)
            ))
        })
}

#[derive(Default)]
struct Options {
    bindings: Vec<(String, String)>,
    directory: Option<PathBuf>,
    watched: Option<PathBuf>,
    beside: bool,
    select: bool,
    moved: Option<Value>,
    resized: Option<Value>,
    line: Option<usize>,
    unsaved: Option<bool>,
    input: Option<String>,
    hidden: bool,
}

fn options(heap: &Heap, given: &[Value]) -> Result<Options, Error> {
    let mut found = Options::default();
    for pair in given.chunks(2) {
        match pair {
            [Value::Keyword(option), value] => match option.name() {
                "keys" => found.bindings = keys(heap, *value)?,
                "directory" => found.directory = Some(PathBuf::from(print::display(heap, *value))),
                "watch" => found.watched = Some(PathBuf::from(print::display(heap, *value))),
                "beside" => found.beside = value.truthy(),
                "select" => found.select = value.truthy(),
                "on-move" => found.moved = Some(*value),
                "on-resize" => found.resized = Some(*value),
                "line" => found.line = Some(number(heap, *value)?),
                "unsaved" => found.unsaved = Some(value.truthy()),
                "input" => found.input = Some(print::display(heap, *value)),
                "show" => found.hidden = !value.truthy(),
                _ => return Err(unknown(heap, pair[0])),
            },
            _ => return Err(unknown(heap, pair[0])),
        }
    }
    Ok(found)
}

fn unknown(heap: &Heap, option: Value) -> Error {
    Error::new(format!(
        "open-buffer takes :keys, :directory, :watch, :beside, :select, :on-move, :on-resize, :line, :unsaved, :input and :show, not {}",
        print::brief(heap, option)
    ))
}

fn hold(heap: &mut Heap, old: Option<Hold>, value: Option<Value>) -> Option<Hold> {
    let Some(value) = value else {
        return old;
    };
    if let Some(old) = old {
        heap.release(old);
    }
    Some(heap.hold(value))
}

fn bind(editor: &mut Editor, id: BufferId, bindings: Vec<(String, String)>) -> Result<(), Error> {
    let Some(scope) = editor
        .plugin_buffer::<LinesBuffer>(id)
        .map(|(_, view)| view.scope)
    else {
        return Ok(());
    };
    for (keys, command) in bindings {
        let command_id = editor
            .command_named(&command)
            .ok_or_else(|| Error::new(format!("There is no command named {command}")))?;
        editor
            .bind(scope, &keys, command_id)
            .map_err(|error| Error::new(format!("{keys}: {error}")))?;
    }
    Ok(())
}

pub(crate) fn reveal(editor: &mut Editor, id: BufferId, beside: bool, select: bool) {
    if !beside {
        editor.show_buffer(id);
    } else if select {
        editor.show_buffer_beside(id);
    } else if !editor.buffer_shown(id) {
        let origin = editor.selected_window();
        editor.show_buffer_beside(id);
        editor.select_window(origin);
    }
}

fn open(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = crate::views::buffer_name(heap, "open-buffer", arguments[0])?;
    let rendered = render(heap, "open-buffer", arguments[1])?;
    let mut given = options(heap, &arguments[2..])?;
    let id = plugin_buffer(editor, &name).unwrap_or_else(|| {
        let scope = editor.add_scope(Box::leak(name.clone().into_boxed_str()));
        editor.add_plugin_buffer_with(&name, |buffer| {
            buffer.set_read_only(true);
            LinesBuffer {
                scope,
                styles: Vec::new(),
                pills: Vec::new(),
                directory: None,
                watched: None,
                moved: None,
                resized: None,
                seen_line: None,
                seen_width: None,
                unsaved: false,
                chat: None,
            }
        })
    });
    if let Some(prompt) = given.input.take() {
        start_chat(editor, id, prompt);
    }
    fill(editor, id, rendered, given.line.take());
    if let Some((buffer, view)) = editor.plugin_buffer_mut::<LinesBuffer>(id) {
        view.seen_line = Some(buffer.line_of(buffer.point()));
        if given.directory.is_some() {
            view.directory = given.directory;
        }
        if given.watched.is_some() {
            view.watched = given.watched;
        }
        view.moved = hold(heap, view.moved, given.moved);
        view.resized = hold(heap, view.resized, given.resized);
        if let Some(unsaved) = given.unsaved {
            view.unsaved = unsaved;
        }
    }
    bind(editor, id, given.bindings)?;
    if !given.hidden {
        reveal(editor, id, given.beside, given.select);
    }
    Ok(Value::Nil)
}

pub fn watch(isolate: &mut Isolate, editor: &mut Editor) {
    let mut calls: Vec<(Hold, usize)> = Vec::new();
    let current = editor.buffer().id();
    for id in editor.plugin_buffers::<LinesBuffer>() {
        let width = editor.buffer_width(id);
        let Some((buffer, view)) = editor.plugin_buffer_mut::<LinesBuffer>(id) else {
            continue;
        };
        let line = buffer.line_of(buffer.point());
        if id == current && view.seen_line != Some(line) {
            view.seen_line = Some(line);
            calls.extend(view.moved.map(|moved| (moved, line)));
        }
        if let Some(width) = width
            && view.seen_width != Some(width)
        {
            view.seen_width = Some(width);
            calls.extend(view.resized.map(|resized| (resized, width)));
        }
    }
    for (hold, number) in calls {
        let Some(handler) = isolate.held(hold) else {
            continue;
        };
        let argument = Value::Int(i64::try_from(number).unwrap_or(i64::MAX));
        if let Err(error) = isolate.spawn(handler, &[argument]) {
            crate::log::report(editor, &error, None);
        }
    }
}
fn set_lines(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = crate::views::buffer_name(heap, "set-lines", arguments[0])?;
    let id = plugin_buffer(editor, &name)
        .ok_or_else(|| Error::new(format!("There is no plugin buffer named {name}")))?;
    let rendered = render(heap, "set-lines", arguments[1])?;
    let mut line = None;
    let mut unsaved = None;
    for pair in arguments[2..].chunks(2) {
        match pair {
            [Value::Keyword(option), value] if option.name() == "line" => {
                line = Some(number(heap, *value)?);
            }
            [Value::Keyword(option), value] if option.name() == "unsaved" => {
                unsaved = Some(value.truthy());
            }
            _ => return Err(Error::new("set-lines takes :line and :unsaved")),
        }
    }
    fill(editor, id, rendered, line);
    if let Some(unsaved) = unsaved
        && let Some((_, view)) = editor.plugin_buffer_mut::<LinesBuffer>(id)
    {
        view.unsaved = unsaved;
    }
    Ok(Value::Nil)
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "natives return results even when they cannot fail"
)]
fn current_line(_: &mut Heap, editor: &mut Editor, _: &[Value]) -> Result<Value, Error> {
    let buffer = editor.buffer();
    let line = buffer.line_of(buffer.point());
    Ok(Value::Int(i64::try_from(line).unwrap_or(i64::MAX)))
}

fn number(heap: &Heap, value: Value) -> Result<usize, Error> {
    match value {
        Value::Int(number) => Ok(usize::try_from(number).unwrap_or(0)),
        _ => Err(Error::new(format!(
            "goto-line expects whole numbers, got {}",
            print::brief(heap, value)
        ))),
    }
}

fn goto_line(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let line = number(heap, arguments[0])?;
    let column = match arguments.get(1) {
        Some(column) => number(heap, *column)?,
        None => 0,
    };
    let buffer = editor.buffer_mut();
    let start = buffer.line_start(line);
    let end = buffer.line_end(line);
    buffer.set_point((start + column).min(end));
    Ok(Value::Nil)
}

fn region_lines(heap: &mut Heap, editor: &mut Editor, _: &[Value]) -> Result<Value, Error> {
    let buffer = editor.buffer();
    let Some(selection) = buffer.selection() else {
        return Ok(Value::Nil);
    };
    let first = buffer.line_of(selection.start);
    let last = buffer
        .line_of(selection.end.max(selection.start + 1) - 1)
        .max(first);
    let number = |line: usize| Value::Int(i64::try_from(line).unwrap_or(i64::MAX));
    heap.vector_from(&[number(first), number(last)])
        .map_err(|full| Error::new(full.to_string()))
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("open-buffer", 2, None, open);
    isolate.define_host_native::<Editor>("set-lines", 2, Some(6), set_lines);
    isolate.define_host_native::<Editor>("current-line", 0, Some(0), current_line);
    isolate.define_host_native::<Editor>("append-lines", 2, Some(2), append);
    isolate.define_host_native::<Editor>("take-input", 0, Some(0), take_input);
    isolate.define_host_native::<Editor>("input-text", 0, Some(1), input_text);
    isolate.define_host_native::<Editor>("input-point", 0, Some(0), input_point);
    isolate.define_host_native::<Editor>("set-input", 2, Some(4), set_input);
    isolate.define_host_native::<Editor>("current-column", 0, Some(0), |_, editor, _| {
        let buffer = editor.buffer();
        let point = buffer.point();
        let column = point - buffer.line_start(buffer.line_of(point));
        Ok(Value::Int(i64::try_from(column).unwrap_or(i64::MAX)))
    });
    isolate.define_host_native::<Editor>("line-text", 1, Some(1), |heap, editor, arguments| {
        let line = number(heap, arguments[0])?;
        let buffer = editor.buffer();
        if line >= buffer.len_lines() {
            return Ok(Value::Nil);
        }
        heap.string(&buffer.line_text(line))
            .map_err(|full| Error::new(full.to_string()))
    });
    isolate.define_host_native::<Editor>("goto-line", 1, Some(2), goto_line);
    isolate.define_host_native::<Editor>(
        "buffer-exists?",
        1,
        Some(1),
        |heap, editor, arguments| {
            let name = print::display(heap, arguments[0]);
            Ok(Value::Bool(editor.buffer_named(&name).is_some()))
        },
    );
    isolate.define_host_native::<Editor>("buffer-width", 1, Some(1), |heap, editor, arguments| {
        let name = print::display(heap, arguments[0]);
        Ok(editor
            .buffer_named(&name)
            .and_then(|id| editor.buffer_width(id))
            .map_or(Value::Nil, |width| {
                Value::Int(i64::try_from(width).unwrap_or(i64::MAX))
            }))
    });
    isolate.define_host_native::<Editor>("region-lines", 0, Some(0), region_lines);
    isolate.define_host_native::<Editor>("buffer-shown?", 1, Some(1), |heap, editor, arguments| {
        let name = print::display(heap, arguments[0]);
        Ok(Value::Bool(
            editor
                .buffer_named(&name)
                .is_some_and(|id| editor.buffer_shown(id)),
        ))
    });
    isolate.define_host_native::<Editor>("deactivate-mark", 0, Some(0), |_, editor, _| {
        editor.buffer_mut().deactivate_mark();
        Ok(Value::Nil)
    });
    isolate.define_host_native::<Editor>(
        "switch-to-buffer",
        1,
        Some(1),
        |heap, editor, arguments| {
            let name = print::display(heap, arguments[0]);
            let id = editor
                .buffer_named(&name)
                .ok_or_else(|| Error::new(format!("There is no buffer named {name}")))?;
            editor.show_buffer(id);
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native::<Editor>("buffer-text", 0, Some(0), |heap, editor, _| {
        heap.string(&editor.buffer().contents())
            .map_err(|full| Error::new(full.to_string()))
    });
    isolate.define_host_native::<Editor>(
        "set-buffer-text",
        1,
        Some(1),
        |heap, editor, arguments| {
            let text = print::display(heap, arguments[0]);
            editor.buffer_mut().replace_text(&text);
            Ok(Value::Nil)
        },
    );
    isolate.define_host_native::<Editor>("buffer-name", 0, Some(0), |heap, editor, _| {
        heap.string(editor.buffer().name())
            .map_err(|full| Error::new(full.to_string()))
    });
}

#[cfg(test)]
mod tests {
    use oxmacs_core::display::LineStyle;
    use oxmacs_core::editor::Editor;
    use oxmacs_core::plugin::PluginBuffer;
    use oxmacs_core::theme::Highlight;

    use crate::Ox;
    use crate::testing::{press, settle};

    fn opened(source: &str) -> (Editor, Ox) {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, source).unwrap();
        settle(&mut editor, &mut ox);
        (editor, ox)
    }

    #[test]
    fn plugin_buffers_show_lines_with_faces() {
        let (editor, _) = opened(
            r#"(open-buffer "*list*" ["plain" ["  " ["dir/" :link] " " [4 :comment] " KB"] ""])"#,
        );

        assert_eq!(editor.buffer().name(), "*list*");
        assert_eq!(editor.buffer().contents(), "plain\n  dir/ 4 KB\n");
        assert!(editor.buffer().is_read_only());
        assert_eq!(editor.line_style(0), LineStyle::default());
        assert_eq!(
            editor.line_style(1).spans,
            [(2..6, Highlight::Link), (7..8, Highlight::Comment)]
        );
    }

    #[test]
    fn replacing_the_lines_keeps_point_on_its_line() {
        let (mut editor, mut ox) =
            opened(r#"(open-buffer "*list*" ["a" "b" "c" "d"]) (goto-line 2)"#);
        assert_eq!(ox.eval(&mut editor, "(current-line)").unwrap(), "2");

        ox.eval(&mut editor, r#"(set-lines "*list*" ["A" "B" "C" "D"])"#)
            .unwrap();
        assert_eq!(ox.eval(&mut editor, "(current-line)").unwrap(), "2");
        assert_eq!(editor.buffer().contents(), "A\nB\nC\nD");

        ox.eval(&mut editor, r#"(set-lines "*list*" ["only"])"#)
            .unwrap();
        assert_eq!(ox.eval(&mut editor, "(current-line)").unwrap(), "0");

        ox.eval(&mut editor, r#"(set-lines "*list*" ["a" "b" "c"] :line 2)"#)
            .unwrap();
        assert_eq!(ox.eval(&mut editor, "(current-line)").unwrap(), "2");
        ox.eval(
            &mut editor,
            r#"(open-buffer "*list*" ["a" "b" "c"] :line 1)"#,
        )
        .unwrap();
        assert_eq!(ox.eval(&mut editor, "(current-line)").unwrap(), "1");
        assert_eq!(
            ox.eval(&mut editor, "(buffer-name)").unwrap(),
            r#""*list*""#
        );
    }

    #[test]
    fn plugin_buffers_have_their_own_keys() {
        let (mut editor, mut ox) = opened(
            r#"(defcommand next-one "Next" (goto-line (+ (current-line) 1))) (open-buffer "*list*" ["a" "b" "c"] :keys {n: 'next-one})"#,
        );

        press(&mut editor, &mut ox, "n");
        press(&mut editor, &mut ox, "n");

        assert_eq!(ox.eval(&mut editor, "(current-line)").unwrap(), "2");
    }

    #[test]
    fn lines_can_carry_a_background_and_buffers_a_directory() {
        let folder = tempfile::tempdir().unwrap();
        let path = format!("{:?}", folder.path().display().to_string());
        let (mut editor, mut ox) = opened(&format!(
            r#"(open-buffer "*dir*" [{{line: "marked" background: :marked}} {{line: ["  " ["x" :link]] background: :flagged}} "plain"] :directory {path}) (goto-line 1 2)"#
        ));

        assert_eq!(editor.line_style(0).background, Some(Highlight::Marked));
        assert_eq!(editor.line_style(1).background, Some(Highlight::Flagged));
        assert_eq!(editor.line_style(1).spans, [(2..3, Highlight::Link)]);
        assert_eq!(editor.buffer().point(), editor.buffer().line_start(1) + 2);
        assert!(
            editor
                .watched_directories()
                .contains(&folder.path().to_path_buf())
        );
        assert_eq!(editor.default_directory(), folder.path());
        assert_eq!(
            ox.eval(
                &mut editor,
                r#"(open-buffer "*bad*" [{line: "x" background: :nope}])"#
            )
            .unwrap_err()
            .message,
            "open-buffer doesn't know the face :nope"
        );
    }

    #[test]
    fn segments_can_be_pills_and_buffers_can_watch_another_folder_too() {
        let folder = tempfile::tempdir().unwrap();
        let metadata = folder.path().join(".git");
        std::fs::create_dir(&metadata).unwrap();
        let (editor, _) = opened(&format!(
            r#"(open-buffer "*pills*" [["Head: " ["main" :link :pill] " " ["Ada" :nick-3]]] :directory {:?} :watch {:?})"#,
            folder.path().display().to_string(),
            metadata.display().to_string()
        ));

        assert_eq!(
            editor.line_style(0).spans,
            [(6..10, Highlight::Link), (11..14, Highlight::Nick(3))]
        );
        let id = editor.buffer().id();
        let (_, view) = editor.plugin_buffer::<super::LinesBuffer>(id).unwrap();
        assert_eq!(view.pills(0), [(6..10, Highlight::Link)]);
        assert_eq!(editor.default_directory(), folder.path());
        assert!(editor.watched_directories().contains(&metadata));
    }

    #[test]
    fn plugin_buffers_keep_their_names_when_files_are_saved() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("notes.txt");
        std::fs::write(&file, "hello").unwrap();
        let (mut editor, mut ox) = opened(&format!(
            r#"(open-buffer "*mine*" ["a"] :directory {:?})"#,
            folder.path().display().to_string()
        ));
        editor.open_file(&file);

        press(&mut editor, &mut ox, "x C-x C-s");

        assert!(
            editor.buffer_names().contains(&"*mine*"),
            "{:?}",
            editor.buffer_names()
        );
    }

    #[test]
    fn buffers_opened_beside_can_take_the_focus() {
        let (mut editor, mut ox) = opened(
            r#"(open-buffer "*main*" ["a"]) (open-buffer "*side*" ["x"] :beside true :select true)"#,
        );
        assert_eq!(editor.buffer().name(), "*side*");
        assert_eq!(editor.tiling().windows().len(), 2);

        press(&mut editor, &mut ox, "C-x o");
        ox.eval(
            &mut editor,
            r#"(open-buffer "*side*" ["y"] :beside true :select true)"#,
        )
        .unwrap();

        assert_eq!(editor.buffer().name(), "*side*");
        assert_eq!(editor.tiling().windows().len(), 2);
    }

    #[test]
    fn buffers_can_open_beside_and_follow_point_and_width() {
        let (mut editor, mut ox) = opened(
            r#"(def moves (atom [])) (def widths (atom []))
               (open-buffer "*main*" ["a" "b" "c"] :on-move (fn [line] (swap! moves conj line)))
               (open-buffer "*side*" ["x"] :beside true :on-resize (fn [width] (swap! widths conj width)))"#,
        );
        assert_eq!(editor.buffer().name(), "*main*");
        assert_eq!(editor.tiling().windows().len(), 2);
        let side = editor.buffer_named("*side*").unwrap();
        assert!(editor.buffer_shown(side));
        assert_eq!(
            ox.eval(
                &mut editor,
                r#"[(buffer-shown? "*side*") (buffer-shown? "*scratch*")]"#
            )
            .unwrap(),
            "[true false]"
        );

        press(&mut editor, &mut ox, "C-n");
        press(&mut editor, &mut ox, "C-n");
        press(&mut editor, &mut ox, "C-e");
        assert_eq!(ox.eval(&mut editor, "@moves").unwrap(), "[1 2]");

        for window in editor.tiling().windows() {
            editor.resize_pane(window, 30, 60);
        }
        editor.prepare_display();
        settle(&mut editor, &mut ox);
        let width = ox.eval(&mut editor, r#"(buffer-width "*side*")"#).unwrap();
        assert_ne!(width, "nil");
        assert_eq!(ox.eval(&mut editor, "(last @widths)").unwrap(), width);
        assert_eq!(
            ox.eval(&mut editor, r#"(buffer-width "*nowhere*")"#)
                .unwrap(),
            "nil"
        );
    }

    #[test]
    fn the_region_reads_as_lines_until_it_is_deactivated() {
        let (mut editor, mut ox) = opened(r#"(open-buffer "*lines*" ["a" "b" "c" "d"])"#);
        assert_eq!(ox.eval(&mut editor, "(region-lines)").unwrap(), "nil");

        press(&mut editor, &mut ox, "C-n C-SPC C-n C-e");
        assert_eq!(ox.eval(&mut editor, "(region-lines)").unwrap(), "[1 2]");
        press(&mut editor, &mut ox, "C-a");
        assert_eq!(ox.eval(&mut editor, "(region-lines)").unwrap(), "[1 1]");

        ox.eval(&mut editor, "(deactivate-mark)").unwrap();
        assert_eq!(ox.eval(&mut editor, "(region-lines)").unwrap(), "nil");
    }

    #[test]
    fn the_current_buffer_text_can_be_read_and_replaced() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        press(&mut editor, &mut ox, "h i");
        assert_eq!(ox.eval(&mut editor, "(buffer-text)").unwrap(), r#""hi""#);

        ox.eval(&mut editor, r#"(set-buffer-text "bye")"#).unwrap();
        assert_eq!(editor.buffer().contents(), "bye");
        assert!(editor.buffer().is_modified());

        press(&mut editor, &mut ox, "C-/");
        assert_eq!(editor.buffer().contents(), "hi");
    }

    #[test]
    fn replacing_the_lines_keeps_point_where_it_was_on_its_line() {
        let (mut editor, mut ox) =
            opened(r#"(open-buffer "*list*" ["alpha" "beta" "gamma"]) (goto-line 1 3)"#);
        let before = editor.buffer().point();

        ox.eval(
            &mut editor,
            r#"(set-lines "*list*" ["alpha" "beta" "gamma"])"#,
        )
        .unwrap();
        assert_eq!(editor.buffer().point(), before);

        ox.eval(
            &mut editor,
            r#"(set-lines "*list*" ["alpha" "be" "gamma"])"#,
        )
        .unwrap();
        assert_eq!(editor.buffer().point(), editor.buffer().line_start(1) + 2);
    }

    #[test]
    fn plugins_switch_to_buffers_by_name() {
        let (mut editor, mut ox) = opened(
            r#"(open-buffer "*a*" ["a"]) (open-buffer "*b*" ["b"]) (switch-to-buffer "*a*")"#,
        );
        assert_eq!(editor.buffer().name(), "*a*");

        let missing = ox
            .eval(&mut editor, r#"(switch-to-buffer "*nowhere*")"#)
            .unwrap_err();
        assert_eq!(missing.message, "There is no buffer named *nowhere*");
    }

    #[test]
    fn chat_buffers_keep_typed_input_below_their_lines() {
        let (mut editor, mut ox) = opened(r#"(open-buffer "*chat*" ["one" "two"] :input "> ")"#);
        assert_eq!(editor.buffer().contents(), "one\ntwo\n> ");

        press(&mut editor, &mut ox, "h i");
        ox.eval(
            &mut editor,
            r#"(append-lines "*chat*" ["three" ["four " ["x" :link]]])"#,
        )
        .unwrap();
        assert_eq!(editor.buffer().contents(), "one\ntwo\nthree\nfour x\n> hi");
        assert_eq!(editor.line_style(3).spans, [(5..6, Highlight::Link)]);
        assert_eq!(editor.buffer().point(), editor.buffer().len_chars());

        ox.eval(&mut editor, r#"(set-lines "*chat*" ["fresh"])"#)
            .unwrap();
        assert_eq!(editor.buffer().contents(), "fresh\n> hi");
        assert_eq!(
            ox.eval(&mut editor, "[(input-text) (input-point) (take-input)]")
                .unwrap(),
            r#"["hi" 2 "hi"]"#
        );
        assert_eq!(editor.buffer().contents(), "fresh\n> ");

        press(&mut editor, &mut ox, "M-< x");
        assert_eq!(editor.buffer().contents(), "fresh\n> x");
        press(&mut editor, &mut ox, "M-<");
        assert_eq!(ox.eval(&mut editor, "(input-point)").unwrap(), "nil");
        press(&mut editor, &mut ox, "M-> DEL M-p");
        assert_eq!(editor.buffer().contents(), "fresh\n> hi");

        ox.eval(
            &mut editor,
            r#"(open-buffer "*chat*" ["fresh"] :input ">> ")"#,
        )
        .unwrap();
        assert_eq!(editor.buffer().contents(), "fresh\n>> hi");
        assert_eq!(
            ox.eval(&mut editor, "[(input-text) (input-point)]")
                .unwrap(),
            r#"["hi" 2]"#
        );
    }

    #[test]
    fn buffers_can_be_made_without_showing_them() {
        let (editor, _) = opened(r#"(open-buffer "*quiet*" ["a"] :input "> " :show false)"#);

        assert_eq!(editor.buffer().name(), "*scratch*");
        let id = editor.buffer_named("*quiet*").unwrap();
        assert_eq!(editor.buffer_by_id(id).unwrap().contents(), "a\n> ");
        assert!(!editor.buffer_shown(id));
    }

    #[test]
    fn plugins_set_the_input_and_read_the_text_around_point() {
        let (mut editor, mut ox) = opened(
            r##"(open-buffer "*chat*" ["alpha beta" "gamma"] :input "#oxmacs> ")
               (set-input "*chat*" "hello wor" :point 5)"##,
        );

        assert_eq!(
            editor.buffer().contents(),
            "alpha beta\ngamma\n#oxmacs> hello wor"
        );
        assert_eq!(ox.eval(&mut editor, "(input-point)").unwrap(), "5");
        ox.eval(&mut editor, "(goto-line 0 6)").unwrap();
        assert_eq!(
            ox.eval(
                &mut editor,
                "[(current-column) (line-text 0) (line-text 1) (input-text \"*chat*\")]"
            )
            .unwrap(),
            r#"[6 "alpha beta" "gamma" "hello wor"]"#
        );
        assert_eq!(
            ox.eval(&mut editor, r#"(input-text "*scratch*")"#).unwrap(),
            "nil"
        );
    }

    #[test]
    fn mistakes_in_lines_are_errors() {
        let mut editor = Editor::new();
        let mut ox = Ox::new();

        let face = ox
            .eval(&mut editor, r#"(open-buffer "*x*" [[["a" :nope]]])"#)
            .unwrap_err();
        let line = ox.eval(&mut editor, r#"(open-buffer "*x*" [5])"#);
        let missing = ox
            .eval(&mut editor, r#"(set-lines "*nowhere*" ["a"])"#)
            .unwrap_err();

        assert_eq!(face.message, "open-buffer doesn't know the face :nope");
        assert_eq!(
            ox.eval(
                &mut editor,
                r#"[(buffer-exists? "*scratch*") (buffer-exists? "*nowhere*")]"#
            )
            .unwrap(),
            "[true false]"
        );
        assert!(line.is_ok());
        assert_eq!(missing.message, "There is no plugin buffer named *nowhere*");
    }
}
