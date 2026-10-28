use std::ops::Range;
use std::time::{Duration, Instant};

use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::{Editor, Lasts, Look, Severity};
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::api::handlers;
use crate::api::handles;
use oxmacs_core::theme::face_named;

fn wrong(heap: &Heap, what: &str, value: Value) -> Error {
    Error::new(format!(
        "annotate expects {what}, got {}",
        print::brief(heap, value)
    ))
}

fn position(heap: &Heap, native: &str, value: Value) -> Result<usize, Error> {
    match value {
        Value::Int(number) => usize::try_from(number)
            .map_err(|_| Error::new(format!("{native} expects positions from 0, got {number}"))),
        _ => Err(Error::new(format!(
            "{native} expects a position, got {}",
            print::brief(heap, value)
        ))),
    }
}

pub(crate) fn span(
    heap: &Heap,
    editor: &Editor,
    native: &str,
    arguments: &[Value],
) -> Result<(BufferId, Range<usize>), Error> {
    let start = position(heap, native, arguments[0])?;
    let end = position(heap, native, arguments[1])?;
    let buffer = handles::current(heap, editor)?;
    Ok((buffer, start.min(end)..start.max(end)))
}

fn face(heap: &Heap, value: Value) -> Result<Option<Highlight>, Error> {
    match value {
        Value::Nil => Ok(None),
        Value::Keyword(name) => face_named(name.name())
            .map(Some)
            .ok_or_else(|| wrong(heap, "a face such as :region or :removed", value)),
        _ => Err(wrong(heap, "a face such as :region or :removed", value)),
    }
}

fn severity(heap: &Heap, value: Value) -> Result<Option<Severity>, Error> {
    match value {
        Value::Nil => Ok(None),
        Value::Keyword(name) if name.name() == "error" => Ok(Some(Severity::Error)),
        Value::Keyword(name) if name.name() == "warning" => Ok(Some(Severity::Warning)),
        _ => Err(wrong(heap, ":underline :error or :warning", value)),
    }
}

fn note(heap: &Heap, value: Value) -> Result<Option<String>, Error> {
    match value {
        Value::Nil => Ok(None),
        _ => heap
            .text(value)
            .map(|text| Some(text.to_owned()))
            .ok_or_else(|| wrong(heap, ":note as a string", value)),
    }
}

fn lasts(heap: &Heap, value: Value) -> Result<Option<Lasts>, Error> {
    let wanted = ":lasts :edit, :cleared or milliseconds from 0";
    match value {
        Value::Nil => Ok(Some(Lasts::Edit)),
        Value::Keyword(name) if name.name() == "edit" => Ok(Some(Lasts::Edit)),
        Value::Keyword(name) if name.name() == "cleared" => Ok(Some(Lasts::Cleared)),
        Value::Int(0) => Ok(None),
        Value::Int(number) => u64::try_from(number)
            .map(|milliseconds| {
                Some(Lasts::Fade(
                    Instant::now(),
                    Duration::from_millis(milliseconds),
                ))
            })
            .map_err(|_| wrong(heap, wanted, value)),
        _ => Err(wrong(heap, wanted, value)),
    }
}

fn key(heap: &Heap, value: Value) -> Option<String> {
    (value != Value::Nil).then(|| handlers::key(heap, value))
}

fn annotate(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let (buffer, range) = span(heap, editor, "annotate", arguments)?;
    let shown = face(heap, arguments[2])?;
    let text = note(heap, arguments[3])?;
    let wave = severity(heap, arguments[4])?;
    let lasts = lasts(heap, arguments[5])?;
    let key = key(heap, arguments[6]).unwrap_or_default();
    let Some(lasts) = lasts else {
        return Ok(Value::Nil);
    };
    let mut looks = Vec::new();
    match text {
        Some(text) => looks.push((
            range.end..range.end,
            Look::Note(text, shown.unwrap_or(Highlight::Comment)),
        )),
        None if shown.is_some() || wave.is_none() => looks.push((
            range.clone(),
            Look::Shade(shown.unwrap_or(Highlight::Region)),
        )),
        None => {}
    }
    if let Some(wave) = wave {
        looks.push((range, Look::Wave(wave)));
    }
    for (range, look) in looks {
        editor.annotate(buffer, range, look, lasts, &key);
    }
    Ok(Value::Nil)
}

fn clear_annotations(
    heap: &mut Heap,
    editor: &mut Editor,
    arguments: &[Value],
) -> Result<Value, Error> {
    let buffer = handles::current(heap, editor)?;
    let key = key(heap, arguments[0]);
    editor.clear_annotations(buffer, key.as_deref());
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "annotate")
        .arity(2, Some(2))
        .keywords(&["face", "note", "underline", "lasts", "key"])
        .host::<Editor>(annotate);
    isolate
        .native("ox", "clear-annotations")
        .arity(0, Some(0))
        .keywords(&["key"])
        .host::<Editor>(clear_annotations);
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use oxmacs_core::editor::{Editor, Severity};
    use oxmacs_core::theme::Highlight;

    use crate::Ox;

    fn written(text: &str) -> (Editor, Ox) {
        let mut editor = Editor::new();
        editor.buffer_mut().insert_at(0, text);
        (editor, Ox::new())
    }

    fn eval(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
        match ox.eval(editor, source) {
            Ok(shown) => shown,
            Err(error) => format!("error: {}", error.message),
        }
    }

    #[test]
    fn annotate_shades_characters_of_the_current_buffer_until_the_next_edit() {
        let (mut editor, mut ox) = written("abc def\nghi");
        let now = Instant::now();

        eval(&mut editor, &mut ox, "(annotate 9 4)");

        let pane = editor.pane();
        assert_eq!(
            pane.line_decorations(0, now).shades,
            [(4..7, Highlight::Region, 1.0)]
        );
        assert_eq!(
            pane.line_decorations(1, now).shades,
            [(0..1, Highlight::Region, 1.0)]
        );
        assert!(!pane.flashing(now));
        editor.buffer_mut().insert_at(0, "x");
        assert_eq!(editor.pane().line_decorations(0, now).shades, []);
    }

    #[test]
    fn annotations_fade_over_milliseconds() {
        let (mut editor, mut ox) = written("abc def");
        let before = Instant::now();

        eval(
            &mut editor,
            &mut ox,
            "(annotate 7 4 :face :removed :lasts 2000)",
        );

        let pane = editor.pane();
        assert_eq!(pane.line_decorations(0, before).shades[0].0, 4..7);

        assert_eq!(
            pane.line_decorations(0, before).shades[0].1,
            Highlight::Removed
        );
        assert!(pane.flashing(before + Duration::from_millis(1900)));
        assert!(!pane.flashing(Instant::now() + Duration::from_millis(2000)));
    }

    #[test]
    fn annotate_underlines_and_notes() {
        let (mut editor, mut ox) = written("abc def\nghi");
        let now = Instant::now();

        eval(
            &mut editor,
            &mut ox,
            r#"(annotate 4 7 :underline :warning :lasts :cleared :key :lint)
               (annotate 8 11 :note "=> 3")"#,
        );
        editor.buffer_mut().insert_at(11, "!");

        let pane = editor.pane();
        assert_eq!(
            pane.line_decorations(0, now).waves,
            [(4..7, Severity::Warning)]
        );
        assert_eq!(pane.line_decorations(0, now).shades, []);
        assert_eq!(pane.line_decorations(1, now).note, None);
    }

    #[test]
    fn notes_sit_after_the_line_where_annotations_end() {
        let (mut editor, mut ox) = written("abc def\nghi");

        eval(
            &mut editor,
            &mut ox,
            r#"(annotate 0 9 :note "=> 3" :face :error)"#,
        );

        let pane = editor.pane();
        assert_eq!(pane.line_decorations(0, Instant::now()).note, None);
        assert_eq!(
            pane.line_decorations(1, Instant::now()).note,
            Some(("=> 3".to_owned(), Highlight::Error))
        );
    }

    #[test]
    fn clear_annotations_clears_one_key_or_every_one() {
        let (mut editor, mut ox) = written("abc def");
        let now = Instant::now();
        eval(
            &mut editor,
            &mut ox,
            "(annotate 0 3 :lasts :cleared) (annotate 4 7 :underline :error :lasts :cleared :key :lint)",
        );

        eval(&mut editor, &mut ox, "(clear-annotations :key :lint)");
        assert_eq!(editor.pane().line_decorations(0, now).waves, []);
        assert_eq!(editor.pane().line_decorations(0, now).shades.len(), 1);

        eval(&mut editor, &mut ox, "(clear-annotations)");
        assert_eq!(editor.pane().line_decorations(0, now).shades, []);
    }

    #[test]
    fn annotate_says_what_it_expects() {
        let (mut editor, mut ox) = written("abc");

        for (source, expected) in [
            (
                "(annotate 0 1 :face :plaid)",
                "error: annotate expects a face such as :region or :removed, got :plaid",
            ),
            (
                "(annotate 0 1 :underline :info)",
                "error: annotate expects :underline :error or :warning, got :info",
            ),
            (
                "(annotate 0 1 :note 3)",
                "error: annotate expects :note as a string, got 3",
            ),
            (
                "(annotate 0 1 :lasts :forever)",
                "error: annotate expects :lasts :edit, :cleared or milliseconds from 0, got :forever",
            ),
            (
                "(annotate 0 1 :speed 3)",
                "error: annotate has no keyword parameter :speed",
            ),
        ] {
            assert_eq!(eval(&mut editor, &mut ox, source), expected);
        }
    }
}
