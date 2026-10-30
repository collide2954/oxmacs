use std::collections::{HashMap, HashSet};

use regex::Regex;

use ox_lang::heap::{Heap, Hold};
use ox_lang::native::{field, given, name_of};
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::command::{ScopeId, intern};
use oxmacs_core::editor::{Editor, Service, Source};
use oxmacs_core::syntax::{Language, LanguageSpec, Roots};
use oxmacs_core::text::{Buffer, BufferId};

use crate::ide::log;
use crate::runtime::lexing::{Lexed, LineLexer, Spans, relex};
use oxmacs_core::theme::face_named;

#[derive(Debug, Clone, Default)]
struct Rules {
    width: usize,
    after: Option<Regex>,
    dedent: Option<Regex>,
    tabs: Option<bool>,
}

#[derive(Debug)]
struct Mode {
    language: Language,
    scope: ScopeId,
    highlight: Option<Hold>,
    plugin: Option<String>,
    indent: Rules,
}

fn indent_by_rules(editor: &Editor, id: BufferId, line: usize) -> Option<usize> {
    let language = editor.buffer_language(id)?;
    let rules = editor
        .plugin_state::<Modes>()?
        .defined
        .iter()
        .find(|mode| mode.language == language)?
        .indent
        .clone();
    let buffer = editor.buffer_by_id(id)?;
    let tab = editor.tab_width_of(id).max(1);
    let width = editor
        .indent_size_of(id)
        .unwrap_or(if editor.indent_tabs_of(id) {
            tab
        } else {
            rules.width
        });
    let Some(above) = (0..line)
        .rev()
        .map(|number| buffer.line_text(number))
        .find(|text| !text.trim().is_empty())
    else {
        return Some(0);
    };
    let mut column = above
        .chars()
        .take_while(|c| matches!(c, ' ' | '\t'))
        .fold(0, |column, c| {
            if c == '\t' {
                column + tab - column % tab
            } else {
                column + 1
            }
        });
    if rules
        .after
        .as_ref()
        .is_some_and(|after| after.is_match(&above))
    {
        column += width;
    }
    if rules
        .dedent
        .as_ref()
        .is_some_and(|dedent| dedent.is_match(&buffer.line_text(line)))
    {
        column = column.saturating_sub(width);
    }
    Some(column)
}

#[derive(Debug, Clone, Default)]
pub struct Declared {
    title: String,
    extensions: Vec<String>,
    file_names: Vec<String>,
    comment: Option<String>,
    roots: Vec<String>,
    locate: Option<Vec<String>>,
    grammar: Option<String>,
}

fn kit(grammar: &str) -> Option<LanguageSpec> {
    match grammar {
        "ox" => Some(crate::ide::ox_mode::spec()),
        _ => oxmacs_core::syntax::kit(grammar),
    }
}

fn grammars() -> String {
    let mut known: Vec<&str> = oxmacs_core::syntax::KITS.to_vec();
    known.push("ox");
    known.join(", ")
}

impl Declared {
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn extensions(&self) -> &[String] {
        &self.extensions
    }

    pub fn file_names(&self) -> &[String] {
        &self.file_names
    }
}

pub fn declared(heap: &Heap, value: Value) -> Result<Vec<Declared>, String> {
    let wrong = || {
        ":modes must be a vector of maps with a title, extensions, file-names and roots".to_owned()
    };
    let names = |value: Value| texts(heap, value).map_err(|error| error.message);
    heap.elements(value)
        .ok_or_else(wrong)?
        .into_iter()
        .map(|item| {
            let entries = heap.entries(item).ok_or_else(wrong)?;
            let mut found = Declared::default();
            for (key, value) in entries {
                match key {
                    Value::Keyword(key) if key.name() == "title" => {
                        found.title = print::display(heap, value);
                    }
                    Value::Keyword(key) if key.name() == "extensions" => {
                        found.extensions = names(value)?;
                    }
                    Value::Keyword(key) if key.name() == "file-names" => {
                        found.file_names = names(value)?;
                    }
                    Value::Keyword(key) if key.name() == "comment" => {
                        found.comment = Some(print::display(heap, value));
                    }
                    Value::Keyword(key) if key.name() == "roots" => {
                        found.roots = names(value)?;
                    }
                    Value::Keyword(key) if key.name() == "locate" => {
                        found.locate = Some(names(value)?);
                    }
                    Value::Keyword(key) if key.name() == "grammar" => {
                        let grammar = print::display(heap, value);
                        if kit(&grammar).is_none() {
                            return Err(format!(
                                ":modes names no built-in grammar {grammar}; there are {}",
                                grammars()
                            ));
                        }
                        found.grammar = Some(grammar);
                    }
                    Value::Keyword(key) if key.name() == "name" => {
                        return Err(":modes names a mode with title:, not name:".to_owned());
                    }
                    key => {
                        return Err(format!(
                            ":modes takes title, grammar, extensions, file-names, comment, roots and locate, not {}",
                            print::brief(heap, key)
                        ));
                    }
                }
            }
            if found.title.is_empty() {
                return Err(wrong());
            }
            Ok(found)
        })
        .collect()
}

fn spec(title: &str, alias: String, declared: Declared) -> Language {
    let base = declared
        .grammar
        .as_deref()
        .and_then(kit)
        .unwrap_or_else(|| LanguageSpec::named(title));
    let mut aliases = vec![alias];
    for alias in &base.aliases {
        if !aliases.contains(alias) {
            aliases.push(alias.clone());
        }
    }
    Language::new(LanguageSpec {
        name: title.to_owned(),
        extensions: declared.extensions,
        file_names: declared.file_names,
        aliases,
        line_comment: declared.comment,
        roots: Roots {
            markers: declared.roots,
            locate: declared.locate,
        },
        ..base
    })
}

pub fn declare(editor: &mut Editor, plugin: &str, declared: &Declared) {
    let known = editor.plugin_state::<Modes>().is_some_and(|modes| {
        modes
            .defined
            .iter()
            .any(|mode| mode.language.name() == declared.title)
    });
    if known {
        return;
    }
    let alias = declared.title.to_lowercase().replace(' ', "-");
    let language = spec(&declared.title, alias, declared.clone());
    let scope = editor.scope_named(&declared.title);
    editor.add_language(language.clone(), Some(scope));
    match declared.grammar.as_deref() {
        None => editor.set_indenter(&language, indent_by_rules),
        Some("ox") => crate::ide::ox_mode::attach(editor, &language),
        Some(_) => {}
    }
    if let Some(modes) = modes(editor) {
        modes.defined.push(Mode {
            language,
            scope,
            highlight: None,
            plugin: Some(plugin.to_owned()),
            indent: Rules::default(),
        });
    }
}

pub fn wakes(editor: &Editor) -> Vec<String> {
    let Some(modes) = editor.plugin_state::<Modes>() else {
        return Vec::new();
    };
    let mut found: Vec<String> = editor
        .buffer_ids()
        .into_iter()
        .filter_map(|id| editor.buffer_language(id))
        .filter_map(|language| {
            modes
                .defined
                .iter()
                .find(|mode| mode.language == language && mode.highlight.is_none())
                .and_then(|mode| mode.plugin.clone())
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

#[derive(Debug)]
struct Cached {
    lexed: Lexed<Value>,
    rooted: Option<Hold>,
}

#[derive(Debug, Default)]
struct Modes {
    defined: Vec<Mode>,
    cache: HashMap<BufferId, Cached>,
}

impl Modes {
    fn forget(&mut self, heap: &mut Heap, keep: impl Fn(&BufferId) -> bool) {
        let gone: Vec<BufferId> = self.cache.keys().filter(|id| !keep(id)).copied().collect();
        for id in gone {
            if let Some(rooted) = self.cache.remove(&id).and_then(|cached| cached.rooted) {
                heap.release(rooted);
            }
        }
    }
}

fn modes(editor: &mut Editor) -> Option<&mut Modes> {
    if editor.plugin_state::<Modes>().is_none() {
        editor.set_plugin_state(Modes::default());
    }
    editor.plugin_state_mut::<Modes>()
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
                print::brief(heap, value)
            ))
        })
}

#[derive(Default)]
struct Options {
    extensions: Vec<String>,
    file_names: Vec<String>,
    comment: Option<String>,
    roots: Vec<String>,
    keys: Vec<(String, String)>,
    highlight: Option<Value>,
    provides: Vec<(Service, Value)>,
    indent: Rules,
}

fn rules(heap: &Heap, value: Value) -> Result<Rules, Error> {
    let entries = heap.entries(value).ok_or_else(|| {
        Error::new(format!(
            "define-mode's :indent is a map of :width, :after, :dedent and :tabs, not {}",
            print::brief(heap, value)
        ))
    })?;
    let pattern = |value: Value| {
        if let Some(found) = heap.foreign_of::<Regex>(value) {
            return Ok(found.clone());
        }
        let text = print::display(heap, value);
        Regex::new(&text)
            .map_err(|error| Error::new(format!("define-mode's :indent pattern {text:?}: {error}")))
    };
    let mut found = Rules::default();
    for (key, value) in entries {
        match (key, value) {
            (Value::Keyword(key), Value::Int(width)) if key.name() == "width" => {
                found.width = usize::try_from(width).unwrap_or(0);
            }
            (Value::Keyword(key), value) if key.name() == "after" => {
                found.after = Some(pattern(value)?);
            }
            (Value::Keyword(key), value) if key.name() == "dedent" => {
                found.dedent = Some(pattern(value)?);
            }
            (Value::Keyword(key), Value::Bool(tabs)) if key.name() == "tabs" => {
                found.tabs = Some(tabs);
            }
            (key, _) => {
                return Err(Error::new(format!(
                    "define-mode's :indent takes :width, :after, :dedent and :tabs, not {}",
                    print::brief(heap, key)
                )));
            }
        }
    }
    Ok(found)
}

const DEFINE_MODE: [&str; 13] = [
    "extensions",
    "file-names",
    "comment",
    "roots",
    "keys",
    "highlight",
    "indent",
    "items",
    "complete",
    "signature",
    "definition",
    "references",
    "docs",
];

fn options(heap: &Heap, slots: &[Value]) -> Result<Options, Error> {
    let mut found = Options::default();
    for (option, value) in given(&DEFINE_MODE, slots) {
        match option {
            "extensions" => found.extensions = texts(heap, value)?,
            "file-names" => found.file_names = texts(heap, value)?,
            "comment" => found.comment = Some(print::display(heap, value)),
            "roots" => found.roots = texts(heap, value)?,
            "keys" => {
                found.keys = heap
                    .entries(value)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(key, command)| (name_of(heap, key), name_of(heap, command)))
                    .collect();
            }
            "highlight" => found.highlight = Some(value),
            "indent" => found.indent = rules(heap, value)?,
            option => found
                .provides
                .extend(crate::api::provided::hook(option, value)),
        }
    }
    Ok(found)
}

fn cached(editor: &Editor, id: BufferId, line: usize) -> Spans {
    editor
        .plugin_state::<Modes>()
        .and_then(|modes| modes.cache.get(&id))
        .map(|cached| cached.lexed.spans(line))
        .unwrap_or_default()
}

fn repeated(title: &str, plugin: &str, given: &Options) -> Result<(), Error> {
    let repeated: Vec<&str> = [
        (":extensions", !given.extensions.is_empty()),
        (":file-names", !given.file_names.is_empty()),
        (":comment", given.comment.is_some()),
        (":roots", !given.roots.is_empty()),
        (":mode-keys", !given.keys.is_empty()),
    ]
    .into_iter()
    .filter_map(|(option, set)| set.then_some(option))
    .collect();
    match repeated.as_slice() {
        [] => Ok(()),
        options => Err(Error::new(format!(
            "{plugin}'s plugin.ox declares {title}, so its {} belong there",
            options.join(" and ")
        ))),
    }
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
            .map(|mode| (mode.language.clone(), mode.scope, mode.plugin.clone()))
    });
    if let Some((_, _, Some(plugin))) = &known {
        repeated(&title, plugin, &given)?;
    }
    let fresh = known.is_none();
    let known = known.map(|(language, scope, _)| (language, scope));
    let (language, scope) = known.unwrap_or_else(|| {
        let declared = Declared {
            title: title.clone(),
            extensions: given.extensions,
            file_names: given.file_names,
            comment: given.comment,
            roots: given.roots,
            locate: None,
            grammar: None,
        };
        (
            spec(&title, alias, declared),
            editor.add_scope(intern(&title)),
        )
    });
    editor.add_language(language.clone(), Some(scope));
    if fresh {
        editor.set_indenter(&language, indent_by_rules);
    }
    if let Some(tabs) = given.indent.tabs {
        editor.set_indent_tabs(&language, tabs);
    }
    crate::api::commands::bind_keys(editor, scope, &given.keys)?;
    for (service, hook) in given.provides {
        crate::api::provided::provide(heap, editor, service, Source::Mode(title.clone()), hook);
    }
    let highlight = given.highlight.map(|function| heap.hold(function));
    if highlight.is_some() {
        editor.set_highlighter(&language, cached);
    }
    let Some(modes) = modes(editor) else {
        return Ok(Value::Nil);
    };
    modes.forget(heap, |_| false);
    match modes
        .defined
        .iter_mut()
        .find(|mode| mode.language == language)
    {
        Some(mode) => {
            mode.indent = given.indent;
            if let Some(old) = std::mem::replace(&mut mode.highlight, highlight) {
                heap.release(old);
            }
        }
        None => modes.defined.push(Mode {
            language,
            scope,
            highlight,
            plugin: None,
            indent: given.indent,
        }),
    }
    Ok(Value::Nil)
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native("ox", "define-mode")
        .arity(2, Some(2))
        .keywords(&DEFINE_MODE)
        .host::<Editor>(define);
}

fn spans(heap: &Heap, title: &str, value: Value) -> Result<Spans, Error> {
    let wrong = |value: Value| {
        Error::new(format!(
            "{title} highlighting expects [start end face] spans, got {}",
            print::brief(heap, value)
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
                    print::brief(heap, *face)
                ))
            })?;
            Ok((range, highlight))
        })
        .collect()
}

fn takes_state(heap: &Heap, function: Value) -> bool {
    if let Some(closure) = heap.closure_of(function) {
        let arity = &closure.code.arity;
        return arity.required <= 3 && (arity.rest || arity.required + arity.optional >= 3);
    }
    heap.native_of(function)
        .is_some_and(|native| native.least <= 3 && native.most.is_none_or(|most| most >= 3))
}

struct Highlighter<'a> {
    isolate: &'a mut Isolate,
    editor: &'a mut Editor,
    title: &'a str,
    function: Value,
    stateful: bool,
    fresh: Vec<Hold>,
}

impl LineLexer for Highlighter<'_> {
    type State = Value;
    type Error = Error;

    fn lex(&mut self, line: usize, text: &str, state: &Value) -> Result<(Spans, Value), Error> {
        let shown = self.isolate.heap_mut().string(text)?;
        let number = Value::Int(i64::try_from(line).unwrap_or(i64::MAX));
        let arguments = if self.stateful {
            vec![number, shown, *state]
        } else {
            vec![number, shown]
        };
        let (editor, function) = (&mut *self.editor, self.function);
        let result = self.isolate.within(crate::QUICK, |isolate| {
            isolate.call_with(editor, function, &arguments)
        })?;
        let heap = self.isolate.heap_mut();
        let (shape, next) = match field(heap, result, "spans") {
            Some(shape) => (shape, field(heap, result, "state").unwrap_or(Value::Nil)),
            None => (result, Value::Nil),
        };
        let found = spans(heap, self.title, shape)?;
        self.fresh.push(heap.hold(next));
        Ok((found, next))
    }

    fn same(&self, a: &Value, b: &Value) -> bool {
        ox_lang::value::equal(self.isolate.heap(), *a, *b)
    }
}

pub fn highlight(isolate: &mut Isolate, editor: &mut Editor) {
    let Some(defined) = editor.plugin_state::<Modes>().map(|modes| {
        modes
            .defined
            .iter()
            .filter_map(|mode| Some((mode.language.clone(), mode.highlight?)))
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
        let Some(revision) = editor.buffer_by_id(*id).map(Buffer::revision) else {
            continue;
        };
        let fresh = editor
            .plugin_state::<Modes>()
            .and_then(|modes| modes.cache.get(id))
            .is_some_and(|cached| cached.lexed.revision == revision);
        if fresh {
            continue;
        }
        let old = editor
            .plugin_state_mut::<Modes>()
            .and_then(|modes| modes.cache.remove(id));
        let lines: Vec<String> = editor
            .buffer_by_id(*id)
            .map(|buffer| {
                (0..buffer.len_lines())
                    .map(|line| buffer.line_text(line).into_owned())
                    .collect()
            })
            .unwrap_or_default();
        work.push((*id, language.clone(), *hold, revision, lines, old));
    }
    for (id, language, hold, revision, lines, old) in work {
        let Some(function) = isolate.held(hold) else {
            continue;
        };
        let (old, old_rooted) = old.map_or((None, None), |old| (Some(old.lexed), old.rooted));
        let stateful = takes_state(isolate.heap(), function);
        let mut highlighter = Highlighter {
            isolate: &mut *isolate,
            editor: &mut *editor,
            title: language.name(),
            function,
            stateful,
            fresh: Vec::new(),
        };
        let (lexed, failed) = relex(old, revision, lines, &Value::Nil, &mut highlighter);
        let fresh = highlighter.fresh;
        if let Some(error) = failed {
            log::report(editor, &error, None);
        }
        let vector = ox_lang::build::make_vector(isolate.heap_mut(), lexed.states()).ok();
        let rooted = vector.map(|vector| isolate.hold(vector));
        for hold in fresh.into_iter().chain(old_rooted) {
            isolate.release(hold);
        }
        if let Some(modes) = modes(editor) {
            modes.cache.insert(id, Cached { lexed, rooted });
        }
    }
    let alive: HashSet<BufferId> = ids.into_iter().collect();
    let heap = isolate.heap_mut();
    if let Some(modes) = modes(editor) {
        modes.forget(heap, |id| alive.contains(id));
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

        assert_eq!(
            editor.language().as_ref().map(Language::name),
            Some("Notes")
        );
        assert_eq!(editor.line_style(0).spans, [(0..9, Highlight::Comment)]);
        assert_eq!(editor.line_style(1).spans, []);

        press(&mut editor, &mut ox, "C-n #");
        assert_eq!(editor.line_style(1).spans, [(0..6, Highlight::Comment)]);

        press(&mut editor, &mut ox, "C-c C-c");
        assert_eq!(ox.eval(&mut editor, "@ran").unwrap(), "1");
    }

    const BLOCKS: &str = r#"(module (import (ox string)))
        (defmode blocks "Blocks" :extensions ["blk"]
          :highlight (fn [line text open]
                       (let [inside (or open (string/starts-with? text "<<"))]
                         {spans: (if inside [[0 (string/length text) :string]] [])
                          state: (and inside (not (string/ends-with? text ">>")))})))"#;

    #[test]
    fn highlighters_carry_state_from_one_line_to_the_next() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.blk");
        std::fs::write(&path, "plain\n<< one\ntwo\nthree >>\nafter").unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(&mut editor, BLOCKS).unwrap();

        editor.open_file(&path);
        settle(&mut editor, &mut ox);

        let styled = |editor: &Editor| -> Vec<bool> {
            (0..5)
                .map(|line| !editor.line_style(line).spans.is_empty())
                .collect()
        };
        assert_eq!(styled(&editor), [false, true, true, true, false]);

        press(&mut editor, &mut ox, "C-n C-a C-d C-d");
        assert_eq!(styled(&editor), [false, false, false, false, false]);

        press(&mut editor, &mut ox, "C-n < <");
        assert_eq!(styled(&editor), [false, false, true, true, false]);
    }

    #[test]
    fn an_edit_highlights_only_the_lines_whose_state_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.cnt");
        std::fs::write(&path, "plain\n".repeat(40)).unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.eval(
            &mut editor,
            r#"(def lexed (atom 0))
               (defmode counted "Counted" :extensions ["cnt"]
                 :highlight (fn [line text open] (swap! lexed inc) {spans: [] state: open}))"#,
        )
        .unwrap();
        editor.open_file(&path);
        settle(&mut editor, &mut ox);
        assert_eq!(ox.eval(&mut editor, "@lexed").unwrap(), "41");

        press(&mut editor, &mut ox, "C-n C-n x");
        assert_eq!(ox.eval(&mut editor, "@lexed").unwrap(), "42");
        press(&mut editor, &mut ox, "RET");
        assert_eq!(ox.eval(&mut editor, "@lexed").unwrap(), "44");
    }

    fn opened(ox: &mut Ox, source: &str, name: &str, text: &str) -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        let mut editor = Editor::new();
        ox.eval(&mut editor, source).unwrap();
        editor.open_file(&path);
        settle(&mut editor, ox);
        (dir, editor)
    }

    #[test]
    fn modes_indent_after_opening_lines_and_dedent_closing_ones() {
        let mut ox = Ox::new();
        let (_dir, mut editor) = opened(
            &mut ox,
            r#"(defmode braces "Braces" :extensions ["brc"] :indent {width: 4 after: #"[{:]\s*$" dedent: #"^\s*}"})"#,
            "a.brc",
            "if x {",
        );

        press(&mut editor, &mut ox, "C-e RET y RET }");

        assert_eq!(editor.buffer().contents(), "if x {\n    y\n}");
        press(&mut editor, &mut ox, "C-p C-a C-d TAB");
        assert_eq!(editor.buffer().contents(), "if x {\n    y\n}");
    }

    #[test]
    fn modes_can_indent_with_tabs_unless_editorconfig_says_otherwise() {
        let mut ox = Ox::new();
        let source = r#"(defmode tabbed "Tabbed" :extensions ["tbd"] :indent {width: 4 after: #"\{\s*$" tabs: true})"#;
        let (dir, mut editor) = opened(&mut ox, source, "a.tbd", "if x {");

        press(&mut editor, &mut ox, "C-e RET y");
        assert_eq!(editor.buffer().contents(), "if x {\n\ty");

        std::fs::write(
            dir.path().join(".editorconfig"),
            "[*.tbd]\nindent_style = space\nindent_size = 2\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("b.tbd"), "if x {").unwrap();
        editor.open_file(&dir.path().join("b.tbd"));
        press(&mut editor, &mut ox, "C-e RET y");
        assert_eq!(editor.buffer().contents(), "if x {\n  y");
    }

    #[test]
    fn return_keeps_the_line_above_in_modes_without_rules() {
        let mut ox = Ox::new();
        let (_dir, mut editor) = opened(
            &mut ox,
            r#"(defmode plain "Plain" :extensions ["pln"])"#,
            "a.pln",
            "  a",
        );

        press(&mut editor, &mut ox, "C-e RET b");

        assert_eq!(editor.buffer().contents(), "  a\n  b");
    }

    const NOTES_PLUGIN: crate::Bundle = crate::Bundle {
        name: "notes",
        files: &[
            (
                "plugin.ox",
                r#"(plugin notes
  :commands [note-shout]
  :modes [{title: "Notes" extensions: ["note"] roots: ["notebook.toml"]}])"#,
            ),
            (
                "notes.ox",
                r##"(module (import (ox string)))
(defcommand note-shout "Shout" (message "shout"))
(defmode notes "Notes"
  :highlight (fn [line text] (if (string/starts-with? text "#") [[0 (string/length text) :comment]] [])))"##,
            ),
        ],
    };

    #[test]
    fn opening_a_file_of_a_declared_mode_wakes_its_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.note");
        std::fs::write(&path, "# heading\nplain").unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::with_bundled(&[NOTES_PLUGIN]);
        ox.start(&mut editor, Some(dir.path()));

        editor.open_file(&path);
        settle(&mut editor, &mut ox);

        assert_eq!(
            editor.language().as_ref().map(Language::name),
            Some("Notes")
        );
        assert_eq!(editor.line_style(0).spans, [(0..9, Highlight::Comment)]);
    }

    const CRAB_PLUGIN: crate::Bundle = crate::Bundle {
        name: "crab",
        files: &[
            (
                "plugin.ox",
                r#"(plugin crab
  :modes [{title: "Crab" grammar: "rust" extensions: ["crab"] comment: "//"}])"#,
            ),
            ("crab.ox", ""),
        ],
    };

    #[test]
    fn a_manifest_mode_can_borrow_a_built_in_grammar() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shell.crab");
        std::fs::write(&path, "fn main() {\nlet x = 1;\n}\n").unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::with_bundled(&[CRAB_PLUGIN]);
        ox.start(&mut editor, Some(dir.path()));

        editor.open_file(&path);
        settle(&mut editor, &mut ox);

        assert_eq!(editor.language().as_ref().map(Language::name), Some("Crab"));
        press(&mut editor, &mut ox, "C-e");
        crate::testing::typed(&mut editor, &mut ox, " ");
        assert_eq!(editor.line_style(0).spans[0], (0..2, Highlight::Keyword));
        press(&mut editor, &mut ox, "C-n TAB");
        assert_eq!(editor.buffer().line_text(1), "    let x = 1;");
        press(&mut editor, &mut ox, "M-;");
        assert!(editor.buffer().line_text(1).ends_with("// "));
    }

    #[test]
    fn a_manifest_mode_s_roots_mark_its_projects() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("book").join("chapters");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(dir.path().join("book").join("notebook.toml"), "").unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::with_bundled(&[NOTES_PLUGIN]);
        ox.start(&mut editor, Some(dir.path()));

        let found = ox
            .eval(
                &mut editor,
                &format!(
                    "(= (language-root {:?} \"Notes\") {:?})",
                    inner.display().to_string(),
                    dir.path().join("book").display().to_string()
                ),
            )
            .unwrap();

        assert_eq!(found, "true");
    }

    #[test]
    fn a_mode_a_manifest_declares_keeps_its_file_data_there() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::with_bundled(&[NOTES_PLUGIN]);
        ox.start(&mut editor, Some(dir.path()));

        let error = ox
            .eval(
                &mut editor,
                r#"(defmode notes "Notes" :extensions ["note"] :keys {"C-c n" 'note-shout})"#,
            )
            .unwrap_err();

        assert_eq!(
            error.message,
            "notes's plugin.ox declares Notes, so its :extensions and :mode-keys belong there"
        );
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
            Some("Bad highlighting doesn't know the face :nope")
        );
    }
}
