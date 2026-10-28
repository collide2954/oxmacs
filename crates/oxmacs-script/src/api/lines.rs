use std::collections::HashMap;
use std::ops::Range;

use ox_lang::heap::{Heap, Hold};
use ox_lang::value::{Value, equal, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::{Editor, EventKind};
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;
use oxmacs_core::view::overlays::{self, Look};

use crate::api::handles;
use crate::view::views::face_named;

const SHAPES: &str =
    "text, [text face], [text face :pill], [text face :keycap] or [text face :link target]";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Mark {
    Plain,
    Pill,
    Keycap,
    Link(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Piece {
    pub(crate) text: String,
    pub(crate) face: Option<Highlight>,
    pub(crate) mark: Mark,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Line {
    pub(crate) pieces: Vec<Piece>,
    pub(crate) background: Option<Highlight>,
    pub(crate) opacity: f32,
    pub(crate) item: Option<Value>,
}

impl Line {
    pub(crate) fn text(&self) -> String {
        self.pieces
            .iter()
            .map(|piece| piece.text.as_str())
            .collect()
    }

    fn spans(&self) -> impl Iterator<Item = (Range<usize>, &Piece)> {
        let mut column = 0;
        self.pieces.iter().map(move |piece| {
            let start = column;
            column += piece.text.chars().count();
            (start..column, piece)
        })
    }

    pub(crate) fn style(&self) -> LineStyle {
        LineStyle {
            background: self.background,
            spans: self
                .spans()
                .filter_map(|(range, piece)| Some((range, piece.face?)))
                .collect(),
        }
    }

    pub(crate) fn pills(&self) -> Vec<(Range<usize>, Highlight)> {
        self.spans()
            .filter(|(_, piece)| piece.mark == Mark::Pill)
            .filter_map(|(range, piece)| Some((range, piece.face?)))
            .collect()
    }

    pub(crate) fn links(&self) -> Vec<(Range<usize>, Value)> {
        self.spans()
            .filter_map(|(range, piece)| match piece.mark {
                Mark::Link(target) => Some((range, target)),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn row(&self) -> overlays::Row {
        overlays::Row {
            pieces: self
                .pieces
                .iter()
                .map(|piece| overlays::Piece {
                    text: piece.text.clone(),
                    look: match (piece.mark, piece.face) {
                        (Mark::Keycap, _) => Look::Keycap,
                        (_, Some(face)) => Look::Face(face),
                        (_, None) => Look::Plain,
                    },
                })
                .collect(),
            opacity: self.opacity,
        }
    }
}

fn face(heap: &Heap, native: &str, value: Value) -> Result<Option<Highlight>, Error> {
    match value {
        Value::Nil => Ok(None),
        Value::Keyword(name) if face_named(name.name()).is_some() => Ok(face_named(name.name())),
        _ => Err(Error::new(format!(
            "{native} doesn't know the face {}",
            print::brief(heap, value)
        ))),
    }
}

fn parts(heap: &Heap, value: Value) -> Option<Vec<Value>> {
    if heap.text(value).is_some() {
        None
    } else {
        heap.elements(value)
    }
}

fn plain(heap: &Heap, value: Value) -> Piece {
    Piece {
        text: print::display(heap, value),
        face: None,
        mark: Mark::Plain,
    }
}

fn piece(heap: &Heap, native: &str, value: Value) -> Result<Piece, Error> {
    let Some(given) = parts(heap, value) else {
        return Ok(plain(heap, value));
    };
    let (shown, look, mark) = match given[..] {
        [shown, look] => (shown, look, Mark::Plain),
        [shown, look, Value::Keyword(mark)] if mark.name() == "pill" => (shown, look, Mark::Pill),
        [shown, look, Value::Keyword(mark)] if mark.name() == "keycap" => {
            (shown, look, Mark::Keycap)
        }
        [shown, look, Value::Keyword(mark), target] if mark.name() == "link" => {
            (shown, look, Mark::Link(target))
        }
        _ => {
            return Err(Error::new(format!(
                "{native} expects {SHAPES} as a piece, got {}",
                print::brief(heap, value)
            )));
        }
    };
    Ok(Piece {
        text: print::display(heap, shown),
        face: face(heap, native, look)?,
        mark,
    })
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "opacities are small numbers"
)]
fn number(value: Value) -> Option<f32> {
    match value {
        Value::Int(number) => Some(number as f32),
        Value::Float(number) => Some(number as f32),
        _ => None,
    }
}

pub(crate) fn line(heap: &Heap, native: &str, value: Value) -> Result<Line, Error> {
    let mut line = Line {
        pieces: Vec::new(),
        background: None,
        opacity: 1.0,
        item: None,
    };
    let mut shown = value;
    if let Some(fields) = heap.entries(value) {
        shown = Value::Nil;
        for (key, given) in fields {
            let Value::Keyword(key) = key else {
                line.item = Some(value);
                continue;
            };
            match key.name() {
                "line" => shown = given,
                "background" => line.background = face(heap, native, given)?,
                "opacity" if given != Value::Nil => {
                    line.opacity = number(given)
                        .ok_or_else(|| {
                            Error::new(format!(
                                "{native} expects a number from 0 to 1 for :opacity, got {}",
                                print::brief(heap, given)
                            ))
                        })?
                        .clamp(0.0, 1.0);
                }
                "opacity" => {}
                _ => line.item = Some(value),
            }
        }
    }
    match parts(heap, shown) {
        Some(given) => {
            for part in given.into_iter().filter(|part| *part != Value::Nil) {
                line.pieces.push(piece(heap, native, part)?);
            }
        }
        None if shown == Value::Nil => {}
        None => line.pieces.push(plain(heap, shown)),
    }
    Ok(line)
}

pub(crate) fn lines(heap: &Heap, native: &str, value: Value) -> Result<Vec<Line>, Error> {
    parts(heap, value)
        .ok_or_else(|| {
            Error::new(format!(
                "{native} expects a vector of lines, got {}",
                print::brief(heap, value)
            ))
        })?
        .into_iter()
        .map(|given| line(heap, native, given))
        .collect()
}

#[derive(Debug, Default)]
struct Row {
    item: Option<Hold>,
    targets: Vec<Hold>,
}

#[derive(Debug, Default)]
struct Kept(HashMap<BufferId, Vec<Row>>);

fn rows_of(heap: &mut Heap, lines: &[Line]) -> Vec<Row> {
    lines
        .iter()
        .map(|line| Row {
            item: line.item.map(|item| heap.hold(item)),
            targets: line
                .links()
                .into_iter()
                .map(|(_, target)| heap.hold(target))
                .collect(),
        })
        .collect()
}

fn release_rows(heap: &mut Heap, rows: Vec<Row>) {
    for row in rows {
        for hold in row.item.into_iter().chain(row.targets) {
            heap.release(hold);
        }
    }
}

fn worth_keeping(lines: &[Line]) -> bool {
    lines
        .iter()
        .any(|line| line.item.is_some() || !line.links().is_empty())
}

fn kept(editor: &mut Editor) -> &mut Kept {
    if editor.plugin_state::<Kept>().is_none() {
        editor.set_plugin_state(Kept::default());
    }
    editor
        .plugin_state_mut::<Kept>()
        .unwrap_or_else(|| unreachable!("the rows were just added"))
}

pub(crate) fn keep(heap: &mut Heap, editor: &mut Editor, id: BufferId, lines: &[Line]) {
    let old = editor
        .plugin_state_mut::<Kept>()
        .and_then(|state| state.0.remove(&id));
    release_rows(heap, old.unwrap_or_default());
    if worth_keeping(lines) {
        editor.subscribe(EventKind::Killed);
        let rows = rows_of(heap, lines);
        kept(editor).0.insert(id, rows);
    }
}

pub(crate) fn keep_more(
    heap: &mut Heap,
    editor: &mut Editor,
    id: BufferId,
    before: usize,
    lines: &[Line],
) {
    let present = editor
        .plugin_state::<Kept>()
        .is_some_and(|state| state.0.contains_key(&id));
    if !present && !worth_keeping(lines) {
        return;
    }
    editor.subscribe(EventKind::Killed);
    let rows = rows_of(heap, lines);
    let kept = kept(editor).0.entry(id).or_default();
    kept.resize_with(before.max(kept.len()), Row::default);
    kept.splice(before..before, rows);
}

pub(crate) fn release(heap: &mut Heap, editor: &mut Editor, id: BufferId) {
    let old = editor
        .plugin_state_mut::<Kept>()
        .and_then(|state| state.0.remove(&id));
    release_rows(heap, old.unwrap_or_default());
}

pub(crate) fn target(editor: &Editor, id: BufferId, row: usize, index: usize) -> Option<Hold> {
    editor
        .plugin_state::<Kept>()?
        .0
        .get(&id)?
        .get(row)?
        .targets
        .get(index)
        .copied()
}

fn items(editor: &Editor, id: BufferId) -> Vec<Option<Hold>> {
    editor
        .plugin_state::<Kept>()
        .and_then(|state| state.0.get(&id))
        .map(|rows| rows.iter().map(|row| row.item).collect())
        .unwrap_or_default()
}

fn line_number(heap: &Heap, native: &str, value: Value) -> Result<usize, Error> {
    match value {
        Value::Int(number) => usize::try_from(number).map_err(|_| {
            Error::new(format!(
                "{native} expects a line from 0, got {}",
                print::brief(heap, value)
            ))
        }),
        _ => Err(ox_lang::native::expected(
            heap,
            native,
            "a line number",
            value,
        )),
    }
}

fn line_item(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let id = handles::current(heap, editor)?;
    let line = if let Some(given) = arguments.first() {
        line_number(heap, "line-item", *given)?
    } else {
        let buffer = handles::current_buffer(heap, editor)?;
        buffer.line_of(buffer.point())
    };
    Ok(items(editor, id)
        .get(line)
        .copied()
        .flatten()
        .and_then(|hold| heap.held(hold))
        .unwrap_or(Value::Nil))
}

fn has_fields(heap: &Heap, item: Value, fields: &[(Value, Value)]) -> bool {
    let Some(entries) = heap.entries(item) else {
        return false;
    };
    fields.iter().all(|(key, wanted)| {
        entries
            .iter()
            .any(|(known, value)| equal(heap, *known, *key) && equal(heap, *value, *wanted))
    })
}

fn move_to(heap: &Heap, editor: &mut Editor, line: usize) -> Result<(), Error> {
    let buffer = handles::current_buffer_mut(heap, editor)?;
    let start = buffer.line_start(line);
    buffer.set_point(start);
    Ok(())
}

fn goto_item(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let fields = heap.entries(arguments[0]).ok_or_else(|| {
        ox_lang::native::expected(heap, "goto-item", "a map of fields", arguments[0])
    })?;
    let id = handles::current(heap, editor)?;
    let found = items(editor, id)
        .into_iter()
        .enumerate()
        .find_map(|(line, hold)| {
            let item = heap.held(hold?)?;
            has_fields(heap, item, &fields).then_some((line, item))
        });
    let Some((line, item)) = found else {
        return Ok(Value::Nil);
    };
    move_to(heap, editor, line)?;
    Ok(item)
}

fn next_item(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let backward = match arguments.first() {
        None | Some(Value::Nil) => false,
        Some(Value::Int(step)) => *step < 0,
        Some(other) => {
            return Err(ox_lang::native::expected(
                heap,
                "next-item",
                "1 or -1",
                *other,
            ));
        }
    };
    let id = handles::current(heap, editor)?;
    let here = {
        let buffer = handles::current_buffer(heap, editor)?;
        buffer.line_of(buffer.point())
    };
    let items = items(editor, id);
    let kept = |at: usize| items.get(at).copied().flatten().map(|item| (at, item));
    let found = if backward {
        (0..here).rev().find_map(kept)
    } else {
        (here + 1..items.len()).find_map(kept)
    };
    let Some((line, item)) = found else {
        return Ok(Value::Nil);
    };
    move_to(heap, editor, line)?;
    Ok(heap.held(item).unwrap_or(Value::Nil))
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "line-item")
        .arity(0, Some(1))
        .host::<Editor>(line_item);
    isolate
        .native("ox", "goto-item")
        .arity(1, Some(1))
        .host::<Editor>(goto_item);
    isolate
        .native("ox", "next-item")
        .arity(0, Some(1))
        .host::<Editor>(next_item);
}

#[cfg(test)]
mod tests;
