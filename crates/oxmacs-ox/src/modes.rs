use std::collections::{HashMap, HashSet};
use std::ops::Range;

use ox::heap::{Heap, Hold};
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::Editor;
use oxmacs_core::syntax::{Feature, Indentation, Language, LanguageSpec, Outline};
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::log;
use crate::views::face_named;

type Spans = Vec<(Range<usize>, Highlight)>;

const FEATURES: &[Feature] = &[Feature::Highlighting, Feature::Comments];

#[derive(Debug)]
struct Mode {
    language: Language,
    scope: ScopeId,
    highlight: Option<Hold>,
}

#[derive(Debug, Default)]
struct Modes {
    defined: Vec<Mode>,
    cache: HashMap<BufferId, (u64, Vec<Spans>)>,
}

fn modes(editor: &mut Editor) -> Option<&mut Modes> {
    if editor.plugin_state::<Modes>().is_none() {
        editor.set_plugin_state(Modes::default());
    }
    editor.plugin_state_mut::<Modes>()
}

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

fn leak_all(items: Vec<String>) -> &'static [&'static str] {
    Box::leak(
        items
            .into_iter()
            .map(leak)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}

fn texts(heap: &Heap, value: Value) -> Result<Vec<String>, Error> {
    heap.elements(value)
        .map(|items| {
            items
                .into_iter()
                .map(|item| print::display(heap, item))
                .collect()
        })
        .ok_or_else(|| {
            Error::new(format!(
                "define-mode expects a vector of names, got {}",
                print::write(heap, value)
            ))
        })
}

fn name_of(heap: &Heap, value: Value) -> String {
    match value {
        Value::Keyword(name) | Value::Symbol(name) => name.name().to_owned(),
        _ => print::display(heap, value),
    }
}

#[derive(Default)]
struct Options {
    extensions: Vec<String>,
    file_names: Vec<String>,
    comment: Option<String>,
    keys: Vec<(String, String)>,
    highlight: Option<Value>,
}

fn options(heap: &Heap, given: &[Value]) -> Result<Options, Error> {
    let mut found = Options::default();
    for pair in given.chunks(2) {
        match pair {
            [Value::Keyword(option), value] => match option.name() {
                "extensions" => found.extensions = texts(heap, *value)?,
                "file-names" => found.file_names = texts(heap, *value)?,
                "comment" => found.comment = Some(print::display(heap, *value)),
                "keys" => {
                    found.keys = heap
                        .entries(*value)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(key, command)| (name_of(heap, key), name_of(heap, command)))
                        .collect();
                }
                "highlight" => found.highlight = Some(*value),
                _ => return Err(unknown(heap, pair[0])),
            },
            _ => return Err(unknown(heap, pair[0])),
        }
    }
    Ok(found)
}

fn unknown(heap: &Heap, option: Value) -> Error {
    Error::new(format!(
        "define-mode takes :extensions, :file-names, :comment, :keys and :highlight, not {}",
        print::write(heap, option)
    ))
}

fn cached(editor: &Editor, id: BufferId, line: usize) -> Spans {
    editor
        .plugin_state::<Modes>()
        .and_then(|modes| modes.cache.get(&id))
        .and_then(|(_, lines)| lines.get(line))
        .cloned()
        .unwrap_or_default()
}

fn define(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let alias = name_of(heap, arguments[0]);
    let title = print::display(heap, arguments[1]);
    let given = options(heap, &arguments[2..])?;
    let known = editor.plugin_state::<Modes>().and_then(|modes| {
        modes
            .defined
            .iter()
            .find(|mode| mode.language.name() == title)
            .map(|mode| (mode.language, mode.scope))
    });
    let (language, scope) = known.unwrap_or_else(|| {
        let spec = Box::leak(Box::new(LanguageSpec {
            name: leak(title.clone()),
            extensions: leak_all(given.extensions),
            file_names: leak_all(given.file_names),
            aliases: leak_all(vec![alias]),
            line_comment: given.comment.map(leak),
            pairs: &[],
            indentation: Indentation::KEEP,
            grammar: None,
            grammars: &[],
            outline: Outline::NONE,
            features: FEATURES,
        }));
        (Language::new(spec), editor.add_scope(leak(title.clone())))
    });
    editor.add_language(language, Some(scope));
    for (keys, command) in given.keys {
        let id = editor
            .command_named(&command)
            .ok_or_else(|| Error::new(format!("There is no command named {command}")))?;
        editor
            .bind(scope, &keys, id)
            .map_err(|error| Error::new(format!("{keys}: {error}")))?;
    }
    let highlight = given.highlight.map(|function| heap.hold(function));
    if highlight.is_some() {
        editor.set_highlighter(language, cached);
    }
    let Some(modes) = modes(editor) else {
        return Ok(Value::Nil);
    };
    modes.cache.clear();
    match modes
        .defined
        .iter_mut()
        .find(|mode| mode.language == language)
    {
        Some(mode) => {
            if let Some(old) = std::mem::replace(&mut mode.highlight, highlight) {
                heap.release(old);
            }
        }
        None => modes.defined.push(Mode {
            language,
            scope,
            highlight,
        }),
    }
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("define-mode", 2, None, define);
}

fn spans(heap: &Heap, title: &str, value: Value) -> Result<Spans, Error> {
    let wrong = |value: Value| {
        Error::new(format!(
            "{title} highlighting expects [start end face] spans, got {}",
            print::write(heap, value)
        ))
    };
    let number = |value: Value| match value {
        Value::Int(number) => usize::try_from(number).ok(),
        _ => None,
    };
    heap.elements(value)
        .ok_or_else(|| wrong(value))?
        .into_iter()
        .map(|span| {
            let parts = heap.elements(span).ok_or_else(|| wrong(span))?;
            let [start, end, face] = parts.as_slice() else {
                return Err(wrong(span));
            };
            let range = number(*start)
                .zip(number(*end))
                .map(|(start, end)| start..end)
                .ok_or_else(|| wrong(span))?;
            let highlight = match face {
                Value::Keyword(name) => face_named(name.name()),
                _ => None,
            }
            .ok_or_else(|| {
                Error::new(format!(
                    "{title} highlighting doesn't know the face {}",
                    print::write(heap, *face)
                ))
            })?;
            Ok((range, highlight))
        })
        .collect()
}

fn highlight_lines(
    isolate: &mut Isolate,
    editor: &mut Editor,
    title: &str,
    function: Value,
    lines: &[String],
) -> Result<Vec<Spans>, Error> {
    let mut found = Vec::with_capacity(lines.len());
    for (number, text) in lines.iter().enumerate() {
        let shown = isolate
            .heap_mut()
            .string(text)
            .map_err(|full| Error::new(full.to_string()))?;
        let line = Value::Int(i64::try_from(number).unwrap_or(i64::MAX));
        let result = isolate.call_with(editor, function, &[line, shown])?;
        found.push(spans(isolate.heap_mut(), title, result)?);
    }
    Ok(found)
}

pub fn highlight(isolate: &mut Isolate, editor: &mut Editor) {
    let Some(defined) = editor.plugin_state::<Modes>().map(|modes| {
        modes
            .defined
            .iter()
            .filter_map(|mode| Some((mode.language, mode.highlight?)))
            .collect::<Vec<_>>()
    }) else {
        return;
    };
    let ids = editor.buffer_ids();
    let mut work = Vec::new();
    for id in &ids {
        let Some((language, hold)) = editor
            .buffer_language(*id)
            .and_then(|language| defined.iter().find(|(known, _)| *known == language))
        else {
            continue;
        };
        let Some(buffer) = editor.buffer_by_id(*id) else {
            continue;
        };
        let revision = buffer.revision();
        let fresh = editor
            .plugin_state::<Modes>()
            .and_then(|modes| modes.cache.get(id))
            .is_some_and(|(seen, _)| *seen == revision);
        if !fresh {
            let lines: Vec<String> = (0..buffer.len_lines())
                .map(|line| buffer.line_text(line).into_owned())
                .collect();
            work.push((*id, *language, *hold, revision, lines));
        }
    }
    for (id, language, hold, revision, lines) in work {
        let Some(function) = isolate.held(hold) else {
            continue;
        };
        let found = highlight_lines(isolate, editor, language.name(), function, &lines)
            .unwrap_or_else(|error| {
                log::report(editor, &error, None);
                vec![Vec::new(); lines.len()]
            });
        if let Some(modes) = modes(editor) {
            modes.cache.insert(id, (revision, found));
        }
    }
    let alive: HashSet<BufferId> = ids.into_iter().collect();
    if let Some(modes) = modes(editor) {
        modes.cache.retain(|id, _| alive.contains(id));
    }
}
#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::syntax::Language;
    use oxmacs_core::theme::Highlight;

    use crate::Ox;
    use crate::testing::{press, settle};

    const NOTES: &str = r##"(module (import (ox string)))
        (def ran (atom 0))
        (defcommand note-shout "Shout" (swap! ran inc))
        (defmode notes "Notes" :extensions ["note"] :comment "#" :keys {"C-c C-c" 'note-shout}
          :highlight (fn [line text]
                       (if (string/starts-with? text "#") [[0 (string/length text) :comment]] [])))"##;

    #[test]
    fn plugins_define_modes_with_keys_and_highlighting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.note");
        std::fs::write(&path, "# heading\nplain").unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, NOTES).unwrap();

        editor.open_file(&path);
        settle(&mut editor, &mut ox);

        assert_eq!(editor.language().map(Language::name), Some("Notes"));
        assert_eq!(editor.line_style(0).spans, [(0..9, Highlight::Comment)]);
        assert_eq!(editor.line_style(1).spans, []);

        press(&mut editor, &mut ox, "C-n #");
        assert_eq!(editor.line_style(1).spans, [(0..6, Highlight::Comment)]);

        press(&mut editor, &mut ox, "C-c C-c");
        assert_eq!(ox.eval(&mut editor, "@ran").unwrap(), "1");
    }

    #[test]
    fn mistakes_in_highlighting_are_logged_once_and_leave_lines_plain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bad");
        std::fs::write(&path, "x").unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(
            &mut editor,
            r#"(defmode bad "Bad" :extensions ["bad"] :highlight (fn [line text] [[0 1 :nope]]))"#,
        )
        .unwrap();

        editor.open_file(&path);
        settle(&mut editor, &mut ox);

        assert_eq!(editor.line_style(0).spans, []);
        assert_eq!(
            editor.message(),
            Some("Ox: Bad highlighting doesn't know the face :nope")
        );
    }
}
