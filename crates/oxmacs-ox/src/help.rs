use std::fmt::Write as _;
use std::path::Path;

use ox::Isolate;
use ox::describe::{Description, Kind};
use oxmacs_core::command::{Command, ScopeId};
use oxmacs_core::editor::Editor;
use oxmacs_core::plugin::PluginBuffer;
use oxmacs_core::text::BufferId;

use crate::{mode, plugins, stacktrace, tools};

pub const DOC: &str = "*ox doc*";
const LOOK_BACK: usize = 400;

#[derive(Debug, Default)]
pub(crate) struct Help {
    seen: Option<((BufferId, u64, usize), Option<String>)>,
    shown: Option<String>,
}

#[derive(Debug)]
struct DocView {
    scope: ScopeId,
}

impl PluginBuffer for DocView {
    fn scope(&self) -> ScopeId {
        self.scope
    }
}

#[derive(Debug, Clone, Copy)]
struct DocScope(ScopeId);

pub(crate) fn call_head(before: &str) -> Option<String> {
    let chars: Vec<char> = before.chars().collect();
    let mut open: Vec<(char, usize)> = Vec::new();
    let mut quoted = false;
    let mut at = 0;
    while at < chars.len() {
        let character = chars[at];
        if quoted {
            match character {
                '\\' => at += 1,
                '"' => quoted = false,
                _ => {}
            }
        } else {
            match character {
                '"' => quoted = true,
                ';' => {
                    while at < chars.len() && chars[at] != '\n' {
                        at += 1;
                    }
                }
                '\\' => at += 1,
                '(' | '[' | '{' => open.push((character, at)),
                ')' | ']' | '}' => {
                    open.pop();
                }
                _ => {}
            }
        }
        at += 1;
    }
    if quoted {
        return None;
    }
    let &(bracket, at) = open.last()?;
    if bracket != '(' {
        return None;
    }
    let head: String = chars[at + 1..]
        .iter()
        .take_while(|character| !mode::delimits(**character))
        .collect();
    head.chars()
        .next()
        .is_some_and(mode::starts_symbol)
        .then_some(head)
}

pub(crate) fn symbol_at(line: &str, column: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let symbolic = |at: usize| {
        chars
            .get(at)
            .is_some_and(|character| !mode::delimits(*character))
    };
    let mut start = column.min(chars.len());
    while start > 0 && symbolic(start - 1) {
        start -= 1;
    }
    let mut end = column.min(chars.len());
    while symbolic(end) {
        end += 1;
    }
    let word: String = chars[start..end].iter().collect();
    let word = word.trim_start_matches(['\'', '`', '~', '@', '#']);
    let number = word
        .trim_start_matches(['-', '+'])
        .starts_with(|character: char| character.is_ascii_digit());
    let keyword = word.starts_with(':') || (word.len() > 1 && word.ends_with(':'));
    (!word.is_empty() && !number && !keyword).then(|| word.to_owned())
}

pub(crate) fn in_ox(editor: &Editor) -> bool {
    editor.language() == Some(mode::language()) || tools::in_repl(editor)
}

pub(crate) fn module(editor: &Editor, home: Option<&Path>) -> String {
    let path = editor.buffer().file().map(|file| file.path.clone());
    path.zip(home)
        .and_then(|(path, home)| tools::plugin_file(home, &path))
        .map_or_else(
            || "user".to_owned(),
            |(plugin, file)| plugins::module_of(editor, &plugin, &file),
        )
}

fn before_point(editor: &Editor) -> String {
    let buffer = editor.buffer();
    let point = buffer.point();
    let line = buffer.line_of(point);
    let first = (line.saturating_sub(LOOK_BACK)..=line)
        .rev()
        .find(|candidate| buffer.line_text(*candidate).starts_with('('))
        .unwrap_or(line.saturating_sub(LOOK_BACK));
    buffer.text_in(buffer.line_start(first)..point)
}

fn first_sentence(doc: &str) -> &str {
    let line = doc.lines().next().unwrap_or_default();
    line.find(". ").map_or(line, |end| &line[..=end])
}

fn summary(description: &Description) -> String {
    match description.doc.as_deref().map(first_sentence) {
        Some(sentence) if !sentence.is_empty() => {
            format!("{}  {sentence}", description.signature)
        }
        _ => description.signature.clone(),
    }
}

pub(crate) fn eldoc(isolate: &Isolate, editor: &mut Editor, home: Option<&Path>, help: &mut Help) {
    if editor.minibuffer().is_some()
        || editor.question().is_some()
        || editor.pane().popup().is_some()
        || !in_ox(editor)
    {
        return;
    }
    let ours = editor
        .message()
        .is_none_or(|shown| help.shown.as_deref() == Some(shown));
    if !ours {
        return;
    }
    let buffer = editor.buffer();
    let key = (buffer.id(), buffer.revision(), buffer.point());
    let text = match &help.seen {
        Some((seen, text)) if *seen == key => text.clone(),
        _ => {
            let text = call_head(&before_point(editor))
                .and_then(|head| isolate.describe(&module(editor, home), &head))
                .filter(|description| description.kind != Kind::Value)
                .map(|description| summary(&description));
            help.seen = Some((key, text.clone()));
            text
        }
    };
    if let Some(text) = text
        && editor.message() != Some(text.as_str())
    {
        editor.show_message(text.clone());
        help.shown = Some(text);
    }
}

pub(crate) fn name_at_point(editor: &Editor) -> Option<String> {
    let buffer = editor.buffer();
    let point = buffer.point();
    let line = buffer.line_of(point);
    symbol_at(&buffer.line_text(line), point - buffer.line_start(line))
        .or_else(|| call_head(&before_point(editor)))
}

fn kind_words(kind: Kind) -> &'static str {
    match kind {
        Kind::Function => "A function",
        Kind::Macro => "A macro",
        Kind::Native => "Built into Ox",
        Kind::Record => "A record type",
        Kind::Special => "A special form",
        Kind::Value => "A value",
    }
}

pub(crate) fn render(description: &Description, examples: &[String]) -> String {
    let mut text = format!("{}\n{}", description.name, kind_words(description.kind));
    if let Some(place) = &description.place {
        let (line, _) = place.line_column();
        let _ = write!(
            text,
            ", defined at {}:{line}",
            stacktrace::shown_file(place.file())
        );
    }
    text.push_str("\n\n");
    if description.kind != Kind::Value {
        text.push_str(&description.signature);
        text.push_str("\n\n");
    }
    text.push_str(
        description
            .doc
            .as_deref()
            .unwrap_or("No docs yet: a string first in its definition's body would be its doc."),
    );
    text.push('\n');
    if !examples.is_empty() {
        text.push_str("\nExamples\n");
        for example in examples {
            text.push('\n');
            text.push_str(example);
            text.push('\n');
        }
    }
    text
}

fn doc_scope(editor: &mut Editor) -> ScopeId {
    if let Some(DocScope(scope)) = editor.plugin_state::<DocScope>() {
        return *scope;
    }
    let scope = editor.add_scope("ox doc");
    editor.bind(scope, "q", Command::QuitWindow).ok();
    editor.set_plugin_state(DocScope(scope));
    scope
}

pub(crate) fn show_doc(isolate: &Isolate, editor: &mut Editor, home: Option<&Path>) {
    let Some(name) = name_at_point(editor) else {
        editor.show_message("There is no Ox name at point");
        return;
    };
    let Some(description) = isolate.describe(&module(editor, home), &name) else {
        editor.show_message(format!("{name} is not defined"));
        return;
    };
    let text = render(&description, &isolate.examples(&name));
    let scope = doc_scope(editor);
    let id = editor
        .plugin_buffers::<DocView>()
        .first()
        .copied()
        .unwrap_or_else(|| {
            editor.add_plugin_buffer_with(DOC, |buffer| {
                buffer.set_read_only(true);
                DocView { scope }
            })
        });
    editor.fill_buffer(id, &text);
    if let Some((buffer, _)) = editor.plugin_buffer_mut::<DocView>(id) {
        buffer.set_point(0);
    }
    editor.show_buffer_beside(id);
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use oxmacs_core::editor::Editor;
    use oxmacs_core::input::KeySequence;

    use super::{DOC, call_head, symbol_at};
    use crate::Ox;

    #[test]
    fn the_innermost_open_call_names_its_head() {
        let head = |text: &str| call_head(text);

        assert_eq!(head("(area 2 ").as_deref(), Some("area"));
        assert_eq!(head("(+ (area 2 3) ").as_deref(), Some("+"));
        assert_eq!(head("(+ (area 2 3").as_deref(), Some("area"));
        assert_eq!(head("(s/upper \"a(b\" ").as_deref(), Some("s/upper"));
        assert_eq!(head("(f \\( ").as_deref(), Some("f"));
        assert_eq!(head("(f ; (g\n x").as_deref(), Some("f"));
        assert_eq!(head("(f [1 2 "), None);
        assert_eq!(head("(f \"open"), None);
        assert_eq!(head("(f x)"), None);
        assert_eq!(head("( f"), None);
        assert_eq!(head("('quoted"), None);
    }

    #[test]
    fn the_symbol_at_a_column_skips_quotes_numbers_and_keywords() {
        assert_eq!(symbol_at("(area 2 3)", 3).as_deref(), Some("area"));
        assert_eq!(symbol_at("(area 2 3)", 5).as_deref(), Some("area"));
        assert_eq!(symbol_at("(s/upper x)", 1).as_deref(), Some("s/upper"));
        assert_eq!(symbol_at("'foo", 2).as_deref(), Some("foo"));
        assert_eq!(symbol_at("@counter", 0).as_deref(), Some("counter"));
        assert_eq!(symbol_at("(- 4 1)", 1).as_deref(), Some("-"));
        assert_eq!(symbol_at("(f -12)", 4), None);
        assert_eq!(symbol_at("{size: 1}", 2), None);
        assert_eq!(symbol_at("(f :size)", 4), None);
        assert_eq!(symbol_at("(f  x)", 3), None);
    }

    const SOURCE: &str = "(def (area w h) \"Width times height. In cells.\" (* w h))
(test \"area of a unit\" (expect (= (area 1 1) 1)))
(+ (area 2 3) 1)
(def limit 5)
";

    fn press(editor: &mut Editor, ox: &mut Ox, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
        ox.tick(editor);
    }

    fn visiting(home: &Path) -> (Editor, Ox) {
        std::fs::write(home.join("shapes.ox"), SOURCE).unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.start(&mut editor, Some(home));
        editor.open_file(&home.join("shapes.ox"));
        press(&mut editor, &mut ox, "C-c C-b");
        (editor, ox)
    }

    fn go(editor: &mut Editor, ox: &mut Ox, point: usize) {
        editor.buffer_mut().set_point(point + 1);
        press(editor, ox, "C-b");
    }

    #[test]
    fn the_signature_of_the_call_at_point_is_echoed_without_hiding_results() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = visiting(home.path());
        let call = SOURCE.find("(area 2 3)").unwrap();

        go(&mut editor, &mut ox, call + 7);
        assert_eq!(editor.message(), Some("(area w h)  Width times height."));
        go(&mut editor, &mut ox, call + 10);
        assert_eq!(
            editor.message(),
            Some("(+ & numbers)  Adds the numbers; (+) is 0.")
        );
        press(&mut editor, &mut ox, "C-x C-e");
        assert_eq!(editor.message(), Some("=> 6"));

        let limit = SOURCE.find("(def limit").unwrap();
        go(&mut editor, &mut ox, limit + 6);
        assert_eq!(
            editor.message(),
            Some("(def name (doc) value)  Defines name in this module.")
        );
        go(&mut editor, &mut ox, SOURCE.len());
        assert_eq!(editor.message(), None);
    }

    #[test]
    fn c_c_c_d_shows_the_docs_of_the_name_at_point_with_its_examples() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = visiting(home.path());
        let call = SOURCE.find("(area 2 3)").unwrap();

        go(&mut editor, &mut ox, call + 2);
        press(&mut editor, &mut ox, "C-c C-d");

        assert_eq!(editor.buffer().name(), DOC);
        assert_eq!(
            editor.buffer().contents(),
            "area\nA function, defined at shapes.ox:1\n\n(area w h)\n\nWidth times height. In cells.\n\nExamples\n\n(test \"area of a unit\" (expect (= (area 1 1) 1)))\n"
        );
        press(&mut editor, &mut ox, "q");
        assert_eq!(editor.buffer().name(), "shapes.ox");

        let limit = SOURCE.find("limit").unwrap();
        go(&mut editor, &mut ox, limit);
        press(&mut editor, &mut ox, "C-c C-d");
        assert_eq!(
            editor.buffer().contents(),
            "limit\nA value, defined at shapes.ox:4\n\nNo docs yet: a string first in its definition's body would be its doc.\n"
        );
    }

    #[test]
    fn c_c_c_d_says_when_a_name_is_unknown() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = visiting(home.path());
        editor.buffer_mut().insert_at(0, "(nowhere 1)\n");

        go(&mut editor, &mut ox, 3);
        press(&mut editor, &mut ox, "C-c C-d");

        assert_eq!(editor.message(), Some("nowhere is not defined"));
    }
}
