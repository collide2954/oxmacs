use std::ops::Range;
use std::path::{Path, PathBuf};

use ox::heap::Heap;
use ox::value::{Value, print};
use ox::{Error, Isolate};
use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::Editor;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::views::face_named;

type Pills = Vec<(Range<usize>, Highlight)>;

#[derive(Debug)]
pub struct LinesBuffer {
    scope: ScopeId,
    styles: Vec<LineStyle>,
    pills: Vec<Pills>,
    directory: Option<PathBuf>,
    watched: Option<PathBuf>,
}

impl AppletBuffer for LinesBuffer {
    fn scope(&self) -> ScopeId {
        self.scope
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
                print::write(heap, value)
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
            print::write(heap, value)
        ))
    })
}

fn render(heap: &Heap, name: &str, lines: Value) -> Result<Rendered, Error> {
    let lines = heap.elements(lines).ok_or_else(|| {
        Error::new(format!(
            "{name} expects a vector of lines, got {}",
            print::write(heap, lines)
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
        .applet_buffers::<LinesBuffer>()
        .into_iter()
        .find(|id| {
            editor
                .buffer_by_id(*id)
                .is_some_and(|buffer| buffer.name() == name)
        })
}

fn fill(editor: &mut Editor, id: BufferId, rendered: Rendered) {
    let line = editor
        .buffer_by_id(id)
        .map_or(0, |buffer| buffer.line_of(buffer.point()));
    editor.fill_buffer(id, &rendered.text);
    if let Some((buffer, view)) = editor.applet_buffer_mut::<LinesBuffer>(id) {
        view.styles = rendered.styles;
        view.pills = rendered.pills;
        let line = line.min(buffer.len_lines().saturating_sub(1));
        let start = buffer.line_start(line);
        buffer.set_point(start);
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
                print::write(heap, value)
            ))
        })
}

fn open(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = crate::views::buffer_name(heap, "open-buffer", arguments[0])?;
    let rendered = render(heap, "open-buffer", arguments[1])?;
    let mut bindings = Vec::new();
    let mut directory = None;
    let mut watched = None;
    for pair in arguments[2..].chunks(2) {
        match pair {
            [Value::Keyword(option), value] if option.name() == "keys" => {
                bindings = keys(heap, *value)?;
            }
            [Value::Keyword(option), value] if option.name() == "directory" => {
                directory = Some(PathBuf::from(print::display(heap, *value)));
            }
            [Value::Keyword(option), value] if option.name() == "watch" => {
                watched = Some(PathBuf::from(print::display(heap, *value)));
            }
            _ => {
                return Err(Error::new(format!(
                    "open-buffer takes :keys, :directory and :watch, not {}",
                    print::write(heap, pair[0])
                )));
            }
        }
    }
    let id = plugin_buffer(editor, &name).unwrap_or_else(|| {
        let scope = editor.add_scope(Box::leak(name.clone().into_boxed_str()));
        editor.add_applet_buffer_with(&name, |buffer| {
            buffer.set_read_only(true);
            LinesBuffer {
                scope,
                styles: Vec::new(),
                pills: Vec::new(),
                directory: None,
                watched: None,
            }
        })
    });
    fill(editor, id, rendered);
    if let Some((_, view)) = editor.applet_buffer_mut::<LinesBuffer>(id) {
        if directory.is_some() {
            view.directory = directory;
        }
        if watched.is_some() {
            view.watched = watched;
        }
    }
    let scope = editor
        .applet_buffer::<LinesBuffer>(id)
        .map(|(_, view)| view.scope);
    if let Some(scope) = scope {
        for (keys, command) in bindings {
            let command_id = editor
                .command_named(&command)
                .ok_or_else(|| Error::new(format!("There is no command named {command}")))?;
            editor
                .bind(scope, &keys, command_id)
                .map_err(|error| Error::new(format!("{keys}: {error}")))?;
        }
    }
    editor.show_buffer(id);
    Ok(Value::Nil)
}

fn set_lines(heap: &mut Heap, editor: &mut Editor, arguments: &[Value]) -> Result<Value, Error> {
    let name = crate::views::buffer_name(heap, "set-lines", arguments[0])?;
    let id = plugin_buffer(editor, &name)
        .ok_or_else(|| Error::new(format!("There is no plugin buffer named {name}")))?;
    let rendered = render(heap, "set-lines", arguments[1])?;
    fill(editor, id, rendered);
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
            print::write(heap, value)
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

pub fn install(isolate: &mut Isolate) {
    isolate.define_host_native::<Editor>("open-buffer", 2, None, open);
    isolate.define_host_native::<Editor>("set-lines", 2, Some(2), set_lines);
    isolate.define_host_native::<Editor>("current-line", 0, Some(0), current_line);
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
    isolate.define_host_native::<Editor>("buffer-name", 0, Some(0), |heap, editor, _| {
        heap.string(editor.buffer().name())
            .map_err(|full| Error::new(full.to_string()))
    });
}

#[cfg(test)]
mod tests {
    use oxmacs_core::applet::AppletBuffer;
    use oxmacs_core::display::LineStyle;
    use oxmacs_core::editor::Editor;
    use oxmacs_core::input::KeySequence;
    use oxmacs_core::theme::Highlight;

    use crate::Ox;

    fn opened(source: &str) -> (Editor, Ox) {
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, source).unwrap();
        ox.tick(&mut editor);
        (editor, ox)
    }

    fn press(editor: &mut Editor, ox: &mut Ox, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
        ox.tick(editor);
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
        let (_, view) = editor.applet_buffer::<super::LinesBuffer>(id).unwrap();
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
