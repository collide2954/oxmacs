use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, PoisonError};

use ox_lang::forms::{self, Forms};
use ox_lang::syntax::{self, Bracket, Kind, Prefix, tokens};

use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::{Editor, Formatter};
use oxmacs_core::syntax::{Feature, Indentation, Language, LanguageSpec, Outline};
use oxmacs_core::text::{Buffer, BufferId};
use oxmacs_core::theme::Highlight;

static OX: LanguageSpec = LanguageSpec {
    name: "Ox",
    extensions: &["ox"],
    file_names: &[],
    aliases: &["ox"],
    line_comment: Some(";"),
    pairs: &[('(', ')'), ('[', ']'), ('{', '}'), ('"', '"')],
    indentation: Indentation {
        continues: true,
        ..Indentation::KEEP
    },
    grammar: None,
    grammars: &[],
    outline: Outline::NONE,
    features: &[
        Feature::Highlighting,
        Feature::Comments,
        Feature::Pairs,
        Feature::Repl,
    ],
};

const CONTROL: &[&str] = &["raise", "await", "yield", "import", "export", "else"];

pub(crate) static EDITOR_FORMS: LazyLock<Forms> =
    LazyLock::new(|| Forms::built_in().scanned(crate::PRELUDE));

const CONSTANTS: &[&str] = &["nil", "true", "false"];

pub fn language() -> Language {
    Language::new(&OX)
}

type Spans = Vec<(Range<usize>, Highlight)>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct State {
    lexer: syntax::State,
    after_def: bool,
    skipped: Option<usize>,
}
fn number(word: &str) -> bool {
    word.trim_start_matches(['-', '+'])
        .starts_with(|character: char| character.is_ascii_digit())
}

fn word_spans(word: &str, start: usize, after_paren: bool, defining: bool, known: &Forms) -> Spans {
    let end = start + word.chars().count();
    let offset = |index: usize| start + word[..index].chars().count();
    if word.starts_with(':') || word.ends_with(':') || number(word) || CONSTANTS.contains(&word) {
        return vec![(start..end, Highlight::Constant)];
    }
    if defining {
        return vec![(start..end, Highlight::Function)];
    }
    if after_paren && (known.knows(word) || CONTROL.contains(&word)) {
        return vec![(start..end, Highlight::Keyword)];
    }
    if let Some(slash) = word
        .find('/')
        .filter(|slash| *slash > 0 && slash + 1 < word.len())
    {
        let mut spans = vec![(start..offset(slash + 1), Highlight::Type)];
        if after_paren {
            spans.push((offset(slash + 1)..end, Highlight::Function));
        }
        return spans;
    }
    if after_paren {
        return vec![(start..end, Highlight::Function)];
    }
    match word.find('.').filter(|dot| *dot > 0) {
        Some(dot) => vec![(offset(dot)..end, Highlight::Attribute)],
        None => Vec::new(),
    }
}

fn columns(text: &str) -> Option<Vec<usize>> {
    if text.is_ascii() {
        return None;
    }
    let mut found = vec![0; text.len() + 1];
    let mut count = 0;
    for (offset, character) in text.char_indices() {
        found[offset..offset + character.len_utf8()].fill(count);
        count += 1;
    }
    found[text.len()] = count;
    Some(found)
}

fn paint(spans: &mut Spans, range: Range<usize>, face: Highlight) {
    if range.is_empty() {
        return;
    }
    match spans.last_mut() {
        Some((last, known)) if *known == face && last.end == range.start => last.end = range.end,
        _ => spans.push((range, face)),
    }
}

struct Skip {
    start: usize,
    depth: usize,
}

impl Skip {
    fn takes(&mut self, kind: Kind) -> Option<bool> {
        match kind {
            Kind::Space
            | Kind::Comment
            | Kind::BlockComment { .. }
            | Kind::Prefix(_)
            | Kind::Text
            | Kind::Escape
            | Kind::HoleStart
            | Kind::HoleEnd
            | Kind::Raw { closed: false }
            | Kind::Regex { closed: false } => Some(false),
            Kind::Open(_) | Kind::StringStart => {
                self.depth += 1;
                Some(false)
            }
            Kind::Close | Kind::StringEnd if self.depth == 0 => None,
            Kind::Close | Kind::StringEnd => {
                self.depth -= 1;
                Some(self.depth == 0)
            }
            _ => Some(self.depth == 0),
        }
    }
}

fn highlight_with(text: &str, state: &State) -> (Spans, State) {
    highlight_in(text, state, &EDITOR_FORMS)
}

#[allow(clippy::too_many_lines)]
fn highlight_in(text: &str, state: &State, known: &Forms) -> (Spans, State) {
    let map = columns(text);
    let column = |byte: usize| map.as_ref().map_or(byte, |found| found[byte]);
    let (found, lexer) = tokens(text, &state.lexer);
    let mut spans = Vec::new();
    let mut skip = state.skipped.map(|depth| Skip { start: 0, depth });
    let mut faces: Vec<Highlight> = Vec::new();
    let mut meta: Option<(usize, usize)> = None;
    let mut quoted: Option<usize> = None;
    let mut defining = false;
    let mut summary = false;
    let mut head: Option<&str> = None;
    let mut first = state.lexer.is_plain() && state.skipped.is_none();
    let mut previous: Option<Kind> = None;
    for (index, token) in found.iter().enumerate() {
        let (start, end) = (column(token.starts()), column(token.ends()));
        let after_paren = previous == Some(Kind::Open(Bracket::List));
        previous = Some(token.kind);
        if let Some(skipping) = skip.as_mut() {
            match skipping.takes(token.kind) {
                Some(false) => continue,
                Some(true) => {
                    paint(&mut spans, skipping.start..end, Highlight::Comment);
                    skip = None;
                    continue;
                }
                None => {
                    paint(&mut spans, skipping.start..start, Highlight::Comment);
                    skip = None;
                }
            }
        }
        if let Some((from, depth)) = meta.as_mut() {
            match token.kind {
                Kind::Open(_) => *depth += 1,
                Kind::Close => {
                    *depth -= 1;
                    if *depth == 0 {
                        paint(&mut spans, *from..end, Highlight::Attribute);
                        meta = None;
                    }
                }
                _ => {}
            }
            continue;
        }
        if token.kind != Kind::Space {
            first &= matches!(token.kind, Kind::StringStart);
        }
        match token.kind {
            Kind::Prefix(Prefix::Skip) => {
                skip = Some(Skip { start, depth: 0 });
            }
            Kind::Prefix(Prefix::Quote)
                if found.get(index + 1).is_some_and(|next| {
                    next.starts() == token.ends()
                        && matches!(next.kind, Kind::Symbol | Kind::Keyword | Kind::Number)
                }) =>
            {
                quoted = Some(start);
            }
            Kind::Prefix(_) => paint(&mut spans, start..end, Highlight::Macro),
            Kind::Comment | Kind::BlockComment { .. } => {
                paint(&mut spans, start..end, Highlight::Comment);
            }
            Kind::Open(Bracket::Meta) => meta = Some((start, 1)),
            Kind::Flag => paint(&mut spans, start..end, Highlight::Attribute),
            Kind::Char => paint(&mut spans, start..end, Highlight::Constant),
            Kind::Raw { .. } | Kind::Regex { .. } => {
                paint(&mut spans, start..end, Highlight::String);
            }
            Kind::StringStart => {
                let face = if summary || (first && state.after_def) {
                    Highlight::DocComment
                } else {
                    Highlight::String
                };
                (summary, defining, first) = (false, false, false);
                faces.push(face);
                paint(&mut spans, start..end, face);
            }
            Kind::Text | Kind::Escape => {
                let face = faces.last().copied().unwrap_or(Highlight::String);
                paint(&mut spans, start..end, face);
            }
            Kind::StringEnd => {
                let face = faces.pop().unwrap_or(Highlight::String);
                paint(&mut spans, start..end, face);
            }
            Kind::HoleStart | Kind::HoleEnd => {
                paint(&mut spans, start..end, Highlight::Attribute);
            }
            Kind::Symbol | Kind::Keyword | Kind::Number => {
                let word = token.text(text);
                match quoted.take() {
                    Some(from) => paint(&mut spans, from..end, Highlight::Constant),
                    None => {
                        for (range, face) in word_spans(word, start, after_paren, defining, known) {
                            paint(&mut spans, range, face);
                        }
                    }
                }
                summary = defining && head.is_some_and(|head| !forms::special(head));
                defining = after_paren && known.defines(word) == Some(0);
                if defining {
                    head = Some(word);
                }
            }
            _ => {}
        }
    }
    let line_end = column(text.len());
    if let Some((from, _)) = meta {
        paint(&mut spans, from..line_end, Highlight::Attribute);
    }
    let skipped = skip.and_then(|skipping| {
        paint(&mut spans, skipping.start..line_end, Highlight::Comment);
        (skipping.depth > 0 || !lexer.in_code()).then_some(skipping.depth)
    });
    let next = State {
        lexer,
        after_def: text.trim_start().starts_with("(def"),
        skipped,
    };
    (spans, next)
}
#[cfg(test)]
pub(crate) fn highlight_line(text: &str) -> Spans {
    highlight_with(text, &State::default()).0
}

pub(crate) fn highlight_lines(lines: &[&str]) -> Vec<Spans> {
    let mut state = State::default();
    lines
        .iter()
        .map(|line| {
            let (spans, next) = highlight_with(line, &state);
            state = next;
            spans
        })
        .collect()
}

const HIGHLIGHT_LOOK_BACK: usize = 300;
const LEXED_BUFFERS: usize = 8;

fn top_level(line: &str) -> bool {
    ["(def", "(module", "(plugin", "(use-plugin"]
        .iter()
        .any(|head| line.starts_with(head))
}

fn after(text: &str, state: &State) -> State {
    if top_level(text) {
        return highlight_with(text, &State::default()).1;
    }
    highlight_with(text, state).1
}

fn starting(buffer: &Buffer, line: usize) -> (usize, State) {
    let floor = line.saturating_sub(HIGHLIGHT_LOOK_BACK);
    let first = (floor..line)
        .rev()
        .find(|candidate| top_level(&buffer.line_text(*candidate)))
        .unwrap_or(floor);
    if first == line {
        return (line, State::default());
    }
    let state = highlight_with(&buffer.line_text(first), &State::default()).1;
    (first + 1, state)
}

#[derive(Debug)]
struct Starts {
    buffer: BufferId,
    revision: u64,
    first: usize,
    states: Vec<State>,
}

impl Starts {
    fn new(buffer: &Buffer, line: usize) -> Self {
        let (first, state) = starting(buffer, line);
        Self {
            buffer: buffer.id(),
            revision: buffer.revision(),
            first,
            states: vec![state],
        }
    }

    fn state(&mut self, buffer: &Buffer, line: usize) -> State {
        let end = self.first + self.states.len();
        if line < self.first || line > end + HIGHLIGHT_LOOK_BACK {
            *self = Self::new(buffer, line);
        }
        while self.first + self.states.len() <= line {
            let last = self.first + self.states.len() - 1;
            let next = after(&buffer.line_text(last), &self.states[self.states.len() - 1]);
            self.states.push(next);
        }
        self.states[line - self.first].clone()
    }
}

#[derive(Debug, Default)]
struct Lexed(Mutex<Vec<Starts>>);

impl Lexed {
    fn state(&self, buffer: &Buffer, line: usize) -> State {
        let mut known = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        known.retain(|starts| starts.buffer != buffer.id() || starts.revision == buffer.revision());
        if let Some(at) = known.iter().position(|starts| starts.buffer == buffer.id()) {
            return known[at].state(buffer, line);
        }
        if known.len() == LEXED_BUFFERS {
            known.remove(0);
        }
        let mut starts = Starts::new(buffer, line);
        let state = starts.state(buffer, line);
        known.push(starts);
        state
    }
}

#[derive(Debug, Default)]
struct FileForms(HashMap<PathBuf, Forms>);

pub(crate) fn remember_forms(editor: &mut Editor, path: &Path, forms: Forms) {
    if editor.plugin_state::<FileForms>().is_none() {
        editor.set_plugin_state(FileForms::default());
    }
    if let Some(known) = editor.plugin_state_mut::<FileForms>() {
        known.0.insert(path.to_path_buf(), forms);
    }
}

fn forms_at<'e>(editor: &'e Editor, path: Option<&Path>) -> &'e Forms {
    path.and_then(|path| editor.plugin_state::<FileForms>()?.0.get(path))
        .unwrap_or(&EDITOR_FORMS)
}

fn forms_of<'e>(editor: &'e Editor, buffer: &Buffer) -> &'e Forms {
    forms_at(editor, buffer.file().map(|file| file.path.as_path()))
}

fn highlight(editor: &Editor, id: BufferId, line: usize) -> Spans {
    let Some(buffer) = editor.buffer_by_id(id) else {
        return Vec::new();
    };
    let state = editor.plugin_state::<Lexed>().map_or_else(
        || Starts::new(buffer, line).state(buffer, line),
        |lexed| lexed.state(buffer, line),
    );
    highlight_in(&buffer.line_text(line), &state, forms_of(editor, buffer)).0
}
const LOOK_BACK: usize = 2000;

const OX_FMT: Formatter = Formatter {
    name: "ox fmt",
    format: |editor, request| {
        let known = forms_at(editor, Some(request.path))
            .clone()
            .scanned(request.text);
        ox_lang::format::format_with(request.text, &known)
            .map_err(|problem| format!("ox fmt: {problem}"))
    },
    regions: false,
    on_save: true,
};

fn indent(editor: &Editor, id: BufferId, line: usize) -> Option<usize> {
    let buffer = editor.buffer_by_id(id)?;
    let floor = line.saturating_sub(LOOK_BACK);
    let first = (floor..line)
        .rev()
        .find(|candidate| buffer.line_text(*candidate).starts_with('('))
        .unwrap_or(floor);
    let before = buffer.text_in(buffer.line_start(first)..buffer.line_start(line));
    ox_lang::format::indentation_with(&before, forms_of(editor, buffer))
}

pub fn install(editor: &mut Editor, scope: ScopeId) {
    editor.add_language(language(), Some(scope));
    editor.set_scratch_language(Some(language()));
    editor.set_plugin_state(Lexed::default());
    editor.set_highlighter(language(), highlight);
    editor.set_indenter(language(), indent);
    editor.add_formatter(language(), OX_FMT);
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use oxmacs_core::editor::Editor;
    use oxmacs_core::syntax::Language;
    use oxmacs_core::theme::Highlight;

    use super::highlight_line;
    use crate::Ox;
    use crate::testing::keys;

    #[test]
    fn ox_files_open_in_ox_mode() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("tools.ox");
        std::fs::write(&path, "(def x 1)\n").unwrap();
        let mut editor = Editor::new();
        Ox::new().start(&mut editor, Some(home.path()));

        editor.open_file(&path);

        assert_eq!(editor.language().map(Language::name), Some("Ox"));
        assert_eq!(
            editor.line_style(0).spans,
            [
                (1..4, Highlight::Keyword),
                (5..6, Highlight::Function),
                (7..8, Highlight::Constant)
            ]
        );
    }

    fn opened(text: &str) -> (tempfile::TempDir, Editor) {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("tools.ox");
        std::fs::write(&path, text).unwrap();
        let mut editor = Editor::new();
        Ox::new().start(&mut editor, Some(home.path()));
        editor.open_file(&path);
        (home, editor)
    }

    #[test]
    fn scratch_starts_in_ox_mode_and_ox_mode_is_a_command() {
        let home = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        let mut ox = Ox::new();
        ox.start(&mut editor, Some(home.path()));
        let scratch = editor.buffer_named("*scratch*").unwrap();
        editor.show_buffer(scratch);

        assert_eq!(editor.language(), Some(super::language()));
        editor.buffer_mut().insert_at(0, "(+ 1 2)");
        editor.buffer_mut().set_point(7);
        crate::testing::press(&mut editor, &mut ox, "C-x C-e");
        assert_eq!(editor.message(), Some("=> 3"));

        crate::testing::press(&mut editor, &mut ox, "M-x t e x t - m o d e RET");
        assert_eq!(editor.language(), None);
        crate::testing::press(&mut editor, &mut ox, "M-x o x - m o d e RET");
        assert_eq!(editor.language(), Some(super::language()));
    }

    #[test]
    fn tab_and_return_indent_like_the_clojure_style_guide() {
        let (_home, mut editor) = opened("(def (f x)\nx)\n(foo bar");

        keys(&mut editor, "C-n TAB");
        assert_eq!(editor.buffer().line_text(1), "  x)");
        keys(&mut editor, "M-> RET");
        assert_eq!(editor.buffer().line_text(3), "     ");
    }

    #[test]
    fn ox_files_format_on_c_c_c_f_and_on_every_save() {
        let (home, mut editor) = opened("(def (f x)\nx)\n");

        keys(&mut editor, "C-c C-f");
        assert_eq!(editor.buffer().contents(), "(def (f x)\n  x)\n");
        assert_eq!(editor.message(), Some("Formatted with ox fmt"));

        keys(&mut editor, "M-> ( g SPC SPC ) C-x C-s");
        assert_eq!(
            std::fs::read_to_string(home.path().join("tools.ox")).unwrap(),
            "(def (f x)\n  x)\n(g)\n"
        );
    }

    #[test]
    fn lines_are_highlighted_by_what_they_hold() {
        let spans = highlight_line(r#"(def (greet name) (str "Hi {name}" :loud 42 nil)) ; say hi"#);

        assert_eq!(
            spans,
            [
                (1..4, Highlight::Keyword),
                (6..11, Highlight::Function),
                (19..22, Highlight::Function),
                (23..27, Highlight::String),
                (27..28, Highlight::Attribute),
                (32..33, Highlight::Attribute),
                (33..34, Highlight::String),
                (35..40, Highlight::Constant),
                (41..43, Highlight::Constant),
                (44..47, Highlight::Constant),
                (50..58, Highlight::Comment),
            ]
        );
    }

    #[test]
    fn calls_namespaces_fields_quotes_and_derefs_are_told_apart() {
        assert_eq!(
            highlight_line("(ui/text (str now.gen) :face 'quit @world)"),
            [
                (1..4, Highlight::Type),
                (4..8, Highlight::Function),
                (10..13, Highlight::Function),
                (17..21, Highlight::Attribute),
                (23..28, Highlight::Constant),
                (29..34, Highlight::Constant),
                (35..36, Highlight::Macro),
            ]
        );
        assert_eq!(
            highlight_line("(* x 0.5)"),
            [(1..2, Highlight::Function), (5..8, Highlight::Constant)]
        );
        assert_eq!(
            highlight_line(r#""{{literal}""#),
            [(0..12, Highlight::String)]
        );
    }

    #[test]
    fn strings_and_docstrings_carry_across_lines() {
        let lines = super::highlight_lines(&[
            "(def query \"<a>",
            "(not code)",
            "</a>\")",
            "(def (step cells)",
            "  \"The next generation.\"",
            "  cells)",
            "(defcommand life \"Watch it\" (go))",
        ]);

        assert_eq!(lines[1], [(0..10, Highlight::String)]);
        assert_eq!(lines[2], [(0..5, Highlight::String)]);
        assert_eq!(lines[4], [(2..24, Highlight::DocComment)]);
        assert_eq!(lines[5], Vec::new());
        assert_eq!(
            lines[6],
            [
                (1..11, Highlight::Keyword),
                (12..16, Highlight::Function),
                (17..27, Highlight::DocComment),
                (29..31, Highlight::Function),
            ]
        );
    }

    #[test]
    fn metadata_is_an_attribute_and_the_name_after_it_is_still_defined() {
        assert_eq!(
            highlight_line("(def ^:shadow (each f xs) 1)"),
            [
                (1..4, Highlight::Keyword),
                (5..13, Highlight::Attribute),
                (15..19, Highlight::Function),
                (26..27, Highlight::Constant),
            ]
        );
        assert_eq!(
            highlight_line("(defmacro ^{indent: 1} twice [x] x)"),
            [
                (1..9, Highlight::Keyword),
                (10..22, Highlight::Attribute),
                (23..28, Highlight::Function),
            ]
        );
    }

    #[test]
    fn block_and_form_comments_are_comments() {
        let lines = super::highlight_lines(&[
            "(f #| a (b |# x)",
            "#| outer (unbalanced",
            "   #| inner |# \"still",
            "|# (g y)",
            "#;(old",
            "   \"a ) b",
            "c\" [1 2]) (h z)",
            "(k #;skip w) #;",
            "(m)",
        ]);

        assert_eq!(
            lines[0],
            [(1..2, Highlight::Function), (3..13, Highlight::Comment)]
        );
        assert_eq!(lines[1], [(0..20, Highlight::Comment)]);
        assert_eq!(lines[2], [(0..21, Highlight::Comment)]);
        assert_eq!(
            lines[3],
            [(0..2, Highlight::Comment), (4..5, Highlight::Function)]
        );
        assert_eq!(lines[4], [(0..6, Highlight::Comment)]);
        assert_eq!(lines[5], [(0..9, Highlight::Comment)]);
        assert_eq!(
            lines[6],
            [(0..9, Highlight::Comment), (11..12, Highlight::Function)]
        );
        assert_eq!(
            lines[7],
            [
                (1..2, Highlight::Function),
                (3..9, Highlight::Comment),
                (13..15, Highlight::Comment),
            ]
        );
        assert_eq!(lines[8], [(1..2, Highlight::Function)]);
    }

    fn lexed_from_the_form_start(editor: &Editor, line: usize) -> Vec<(Range<usize>, Highlight)> {
        let buffer = editor.buffer();
        let floor = line.saturating_sub(super::HIGHLIGHT_LOOK_BACK);
        let first = (floor..line)
            .rev()
            .find(|candidate| super::top_level(&buffer.line_text(*candidate)))
            .unwrap_or(floor);
        let state = (first..line).fold(super::State::default(), |state, earlier| {
            super::highlight_with(&buffer.line_text(earlier), &state).1
        });
        super::highlight_with(&buffer.line_text(line), &state).0
    }

    fn all_lines_match(editor: &Editor, order: &[usize]) {
        for line in order {
            assert_eq!(
                editor.line_style(*line).spans,
                lexed_from_the_form_start(editor, *line),
                "line {line}"
            );
        }
    }

    #[test]
    fn lines_highlight_the_same_in_any_order_and_after_edits() {
        let form = "(def query \"<a>\n(not code)\n</a>\")\n(def (step cells)\n  \"The next generation.\"\n  cells)\n(defcommand life \"Watch it\" (go))\n#| a (note\n  #| b |# (x\n|# #;(old\n  \"str\n\") (live)\n";
        let (_home, mut editor) = opened(&form.repeat(6));
        let lines = editor.buffer().len_lines();
        let forward: Vec<usize> = (0..lines).collect();
        let backward: Vec<usize> = (0..lines).rev().collect();
        let scattered: Vec<usize> = (0..lines).map(|at| at * 17 % lines).collect();

        for order in [&forward, &backward, &scattered] {
            all_lines_match(&editor, order);
        }
        let start = editor.buffer().line_start(8);
        editor.buffer_mut().insert_at(start, "\"");
        all_lines_match(&editor, &scattered);
        all_lines_match(&editor, &forward);
        editor.buffer_mut().delete_range(start..start + 1);
        all_lines_match(&editor, &backward);
    }

    #[test]
    fn raw_strings_regexes_and_strings_inside_holes_are_colored_whole() {
        assert_eq!(
            highlight_line("(f #r#\"say \"hi\"\"# x)"),
            [(1..2, Highlight::Function), (3..17, Highlight::String)]
        );
        assert_eq!(
            highlight_line("(re #\"\\d{4}\")"),
            [(1..3, Highlight::Function), (4..12, Highlight::String)]
        );
        assert_eq!(
            highlight_line("\"{(if on \"Moved\" \"Copied\")} it\""),
            [
                (0..1, Highlight::String),
                (1..2, Highlight::Attribute),
                (3..5, Highlight::Keyword),
                (9..16, Highlight::String),
                (17..25, Highlight::String),
                (26..27, Highlight::Attribute),
                (27..31, Highlight::String),
            ]
        );
    }

    #[test]
    fn structural_motion_skips_brackets_in_strings_comments_and_characters() {
        let (_home, mut editor) = opened("(f \")\" \\( ; (\n x) (g)\n");

        keys(&mut editor, "C-M-f");
        assert_eq!(editor.buffer().point(), 17);
        keys(&mut editor, "C-M-b");
        assert_eq!(editor.buffer().point(), 0);
        editor.prepare_display();
        assert_eq!(
            editor.line_marks(0),
            [(0..1, Highlight::ParenMatch)],
            "the closer sits on the next line"
        );
        assert_eq!(editor.line_marks(1), [(2..3, Highlight::ParenMatch)]);
        editor.buffer_mut().set_point(5);
        keys(&mut editor, "C-M-u");
        assert_eq!(editor.buffer().point(), 0);
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        assert_eq!(
            highlight_line("(f \"λé\" :k)"),
            [
                (1..2, Highlight::Function),
                (3..7, Highlight::String),
                (8..10, Highlight::Constant),
            ]
        );
    }

    #[test]
    fn map_keys_characters_and_escaped_quotes_are_found() {
        assert_eq!(
            highlight_line(r#"{name: "a\"b" key: \x}"#),
            [
                (1..6, Highlight::Constant),
                (7..13, Highlight::String),
                (14..18, Highlight::Constant),
                (19..21, Highlight::Constant),
            ]
        );
        assert_eq!(highlight_line("(if x"), [(1..3, Highlight::Keyword)]);
        assert_eq!(highlight_line("[if x]"), Vec::new());
    }
}
