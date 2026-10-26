use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use ox_lang::Isolate;
use ox_lang::describe::{Description, Kind};
use ox_lang::names::{call_head, symbol_at};
use oxmacs_core::command::{Command, ScopeId};
use oxmacs_core::editor::Editor;
use oxmacs_core::plugin::PluginBuffer;
use oxmacs_core::text::BufferId;

use crate::{ide::ox_mode, ide::repl, ide::stacktrace, runtime::plugins};

pub const DOC: &str = "*ox doc*";
pub(crate) const SIGNATURE_PAUSE: Duration = Duration::from_millis(400);
const LOOK_BACK: usize = 400;
const AUTO_PREFIX: usize = 3;

type Spot = (BufferId, u64, usize);

#[derive(Debug, Default)]
pub(crate) struct Help {
    seen: Option<(Spot, Option<String>)>,
    rested: Option<Spot>,
    shown: Option<String>,
    completed: Option<(BufferId, u64)>,
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

pub(crate) fn in_ox(editor: &Editor) -> bool {
    editor.language() == Some(ox_mode::language()) || repl::in_repl(editor)
}

pub(crate) fn module(editor: &Editor, home: Option<&Path>) -> String {
    let path = editor.buffer().file().map(|file| file.path.clone());
    path.zip(home)
        .and_then(|(path, home)| repl::plugin_file(home, &path))
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

fn spot(editor: &Editor) -> Spot {
    let buffer = editor.buffer();
    (buffer.id(), buffer.revision(), buffer.point())
}

pub(crate) fn signature_wait(editor: &Editor, help: &Help) -> Option<Duration> {
    (in_ox(editor) && help.rested != Some(spot(editor))).then_some(SIGNATURE_PAUSE)
}

pub(crate) fn echo_signature(
    isolate: &Isolate,
    editor: &mut Editor,
    home: Option<&Path>,
    help: &mut Help,
) {
    if !in_ox(editor) {
        return;
    }
    let key = spot(editor);
    help.rested = Some(key);
    if editor.minibuffer().is_some()
        || editor.question().is_some()
        || editor.pane().popup().is_some()
    {
        return;
    }
    let ours = editor
        .message()
        .is_none_or(|shown| help.shown.as_deref() == Some(shown));
    if !ours {
        return;
    }
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

fn plain_code(line: &str) -> bool {
    let mut quoted = false;
    let mut chars = line.chars();
    while let Some(character) = chars.next() {
        match character {
            '\\' => {
                chars.next();
            }
            '"' => quoted = !quoted,
            ';' if !quoted => return false,
            _ => {}
        }
    }
    !quoted
}

fn typed_name(editor: &Editor) -> (usize, String) {
    let buffer = editor.buffer();
    let point = buffer.point();
    let typed: String = buffer
        .chars_before(point)
        .take_while(|character| !ox_mode::delimits(*character) && !"'`~@#".contains(*character))
        .collect();
    let typed: String = typed.chars().rev().collect();
    (point - typed.chars().count(), typed)
}

pub(crate) fn complete(
    isolate: &Isolate,
    editor: &mut Editor,
    home: Option<&Path>,
    explicit: bool,
) {
    if !in_ox(editor) {
        return;
    }
    let (start, typed) = typed_name(editor);
    let candidates = isolate.completions(&module(editor, home), &typed);
    editor.complete_at(start, candidates, explicit);
}

pub(crate) fn auto_complete(
    isolate: &Isolate,
    editor: &mut Editor,
    home: Option<&Path>,
    help: &mut Help,
) {
    if !editor.just_typed()
        || editor.minibuffer().is_some()
        || editor.pane().popup().is_some()
        || !in_ox(editor)
    {
        return;
    }
    let buffer = editor.buffer();
    let key = (buffer.id(), buffer.revision());
    if help.completed == Some(key) {
        return;
    }
    help.completed = Some(key);
    let point = buffer.point();
    let line = buffer.text_in(buffer.line_start(buffer.line_of(point))..point);
    let (_, typed) = typed_name(editor);
    if typed.chars().count() >= AUTO_PREFIX && plain_code(&line) {
        complete(isolate, editor, home, false);
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
    use std::time::Instant;

    use oxmacs_core::editor::Editor;

    use super::{DOC, SIGNATURE_PAUSE};
    use crate::Ox;
    use crate::testing::{press, settle, typed};

    const SOURCE: &str = "(def (area w h) \"Width times height. In cells.\" (* w h))
(test \"area of a unit\" (expect (= (area 1 1) 1)))
(+ (area 2 3) 1)
(def limit 5)
";

    fn visiting(home: &Path) -> (Editor, Ox) {
        std::fs::write(home.join("shapes.ox"), SOURCE).unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.start(&mut editor, Some(home));
        settle(&mut editor, &mut ox);
        editor.open_file(&home.join("shapes.ox"));
        press(&mut editor, &mut ox, "C-c C-b");
        (editor, ox)
    }

    fn go(editor: &mut Editor, ox: &mut Ox, point: usize) {
        editor.buffer_mut().set_point(point + 1);
        press(editor, ox, "C-b");
    }

    fn rest(editor: &mut Editor, ox: &mut Ox) {
        let since = Instant::now();
        ox.idle(editor, since, since + SIGNATURE_PAUSE);
    }

    #[test]
    fn the_signature_of_the_call_at_point_is_echoed_without_hiding_results() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = visiting(home.path());
        let call = SOURCE.find("(area 2 3)").unwrap();

        go(&mut editor, &mut ox, call + 7);
        rest(&mut editor, &mut ox);
        assert_eq!(editor.message(), Some("(area w h)  Width times height."));
        go(&mut editor, &mut ox, call + 10);
        rest(&mut editor, &mut ox);
        assert_eq!(
            editor.message(),
            Some("(+ & numbers)  Adds the numbers; (+) is 0.")
        );
        press(&mut editor, &mut ox, "C-x C-e");
        rest(&mut editor, &mut ox);
        assert_eq!(editor.message(), Some("=> 6"));

        let limit = SOURCE.find("(def limit").unwrap();
        go(&mut editor, &mut ox, limit + 6);
        rest(&mut editor, &mut ox);
        assert_eq!(
            editor.message(),
            Some("(def name (doc) value)  Defines name in this module.")
        );
        go(&mut editor, &mut ox, SOURCE.len());
        rest(&mut editor, &mut ox);
        assert_eq!(editor.message(), None);
    }

    #[test]
    fn the_signature_waits_until_point_rests() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = visiting(home.path());
        let call = SOURCE.find("(area 2 3)").unwrap();
        let since = Instant::now();

        go(&mut editor, &mut ox, call + 7);
        assert_eq!(editor.message(), None);
        assert_eq!(ox.idle_wait(&editor, since), Some(SIGNATURE_PAUSE));
        ox.idle(&mut editor, since, since + SIGNATURE_PAUSE / 2);
        assert_eq!(editor.message(), None);

        ox.idle(&mut editor, since, since + SIGNATURE_PAUSE);
        assert_eq!(editor.message(), Some("(area w h)  Width times height."));
        assert_ne!(ox.idle_wait(&editor, since), Some(SIGNATURE_PAUSE));
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

    fn offered(editor: &Editor) -> Option<Vec<String>> {
        editor.pane().popup().map(|view| view.items)
    }

    fn at_end(editor: &mut Editor, text: &str) {
        let end = editor.buffer().len_chars();
        editor.buffer_mut().insert_at(end, text);
        let end = editor.buffer().len_chars();
        editor.buffer_mut().set_point(end);
    }

    #[test]
    fn c_m_i_completes_names_from_the_running_image() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = visiting(home.path());

        at_end(&mut editor, "(are");
        press(&mut editor, &mut ox, "C-M-i");
        assert!(editor.buffer().contents().ends_with("(area"));

        at_end(&mut editor, " (lis");
        press(&mut editor, &mut ox, "C-M-i");
        assert_eq!(
            offered(&editor),
            Some(vec!["list".to_owned(), "list?".to_owned()])
        );
        press(&mut editor, &mut ox, "C-n TAB");
        assert!(editor.buffer().contents().ends_with("(list?"));
    }

    #[test]
    fn tab_indents_first_and_completes_once_the_line_is_indented() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = visiting(home.path());

        at_end(&mut editor, "(lim");
        press(&mut editor, &mut ox, "TAB");
        assert!(editor.buffer().contents().ends_with("(limit"));

        at_end(&mut editor, "\n(foo bar\nlim");
        press(&mut editor, &mut ox, "TAB");
        assert!(editor.buffer().contents().ends_with("\n     lim"));
        assert_eq!(editor.pane().popup(), None);
        press(&mut editor, &mut ox, "TAB");
        assert!(editor.buffer().contents().ends_with("\n     limit"));
    }

    #[test]
    fn three_typed_characters_of_a_name_offer_completions_quietly() {
        let home = tempfile::tempdir().unwrap();
        let (mut editor, mut ox) = visiting(home.path());
        at_end(&mut editor, "\n");

        typed(&mut editor, &mut ox, "(li");
        assert_eq!(offered(&editor), None);
        typed(&mut editor, &mut ox, "m");
        assert_eq!(offered(&editor), Some(vec!["limit".to_owned()]));
        assert_eq!(editor.message(), None);

        press(&mut editor, &mut ox, "C-g");
        typed(&mut editor, &mut ox, " ; lim");
        assert_eq!(offered(&editor), None);
        press(&mut editor, &mut ox, "RET");
        typed(&mut editor, &mut ox, "\"lim");
        assert_eq!(offered(&editor), None);
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
