use std::ops::Range;
use std::path::{Path, PathBuf};

use ox::heap::{Heap, Hold};
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
    moved: Option<Hold>,
    resized: Option<Hold>,
    seen_line: Option<usize>,
    seen_width: Option<usize>,
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

fn fill(editor: &mut Editor, id: BufferId, rendered: Rendered, placed: Option<usize>) {
    let Some(buffer) = editor.buffer_by_id(id) else {
        return;
    };
    let here = buffer.line_of(buffer.point());
    let column = buffer.point() - buffer.line_start(here);
    let (line, column) = placed.map_or((here, column), |line| (line, 0));
    if buffer.contents() != rendered.text {
        editor.fill_buffer(id, &rendered.text);
    }
    if let Some((buffer, view)) = editor.applet_buffer_mut::<LinesBuffer>(id) {
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
                print::write(heap, value)
            ))
        })
}

#[derive(Default)]
struct Options {
    bindings: Vec<(String, String)>,
    directory: Option<PathBuf>,
    watched: Option<PathBuf>,
    beside: bool,
    moved: Option<Value>,
    resized: Option<Value>,
    line: Option<usize>,
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
                "on-move" => found.moved = Some(*value),
                "on-resize" => found.resized = Some(*value),
                "line" => found.line = Some(number(heap, *value)?),
                _ => return Err(unknown(heap, pair[0])),
            },
            _ => return Err(unknown(heap, pair[0])),
        }
    }
    Ok(found)
}

fn unknown(heap: &Heap, option: Value) -> Error {
    Error::new(format!(
        "open-buffer takes :keys, :directory, :watch, :beside, :on-move, :on-resize and :line, not {}",
        print::write(heap, option)
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
        .applet_buffer::<LinesBuffer>(id)
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

fn reveal(editor: &mut Editor, id: BufferId, beside: bool) {
    if !beside {
        editor.show_buffer(id);
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
        editor.add_applet_buffer_with(&name, |buffer| {
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
            }
        })
    });
    fill(editor, id, rendered, given.line.take());
    if let Some((buffer, view)) = editor.applet_buffer_mut::<LinesBuffer>(id) {
        view.seen_line = Some(buffer.line_of(buffer.point()));
        if given.directory.is_some() {
            view.directory = given.directory;
        }
        if given.watched.is_some() {
            view.watched = given.watched;
        }
        view.moved = hold(heap, view.moved, given.moved);
        view.resized = hold(heap, view.resized, given.resized);
    }
    bind(editor, id, given.bindings)?;
    reveal(editor, id, given.beside);
    Ok(Value::Nil)
}

pub fn watch(isolate: &mut Isolate, editor: &mut Editor) {
    let mut calls: Vec<(Hold, usize)> = Vec::new();
    let current = editor.buffer().id();
    for id in editor.applet_buffers::<LinesBuffer>() {
        let width = editor.buffer_width(id);
        let Some((buffer, view)) = editor.applet_buffer_mut::<LinesBuffer>(id) else {
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
    let line = match &arguments[2..] {
        [] => None,
        [Value::Keyword(option), value] if option.name() == "line" => Some(number(heap, *value)?),
        _ => return Err(Error::new("set-lines takes only :line")),
    };
    fill(editor, id, rendered, line);
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
    isolate.define_host_native::<Editor>("set-lines", 2, Some(4), set_lines);
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
        ox.tick(&mut editor);
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
