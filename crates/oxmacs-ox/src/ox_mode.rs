use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, PoisonError};

use ox::forms::{self, Forms};

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
    indentation: Indentation::KEEP,
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

pub(crate) fn delimits(character: char) -> bool {
    character.is_whitespace() || "()[]{}\";".contains(character)
}

pub(crate) fn starts_symbol(character: char) -> bool {
    !delimits(character) && !"'`~@#,\\".contains(character)
}

type Spans = Vec<(Range<usize>, Highlight)>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct State {
    in_string: bool,
    after_def: bool,
    block: usize,
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

fn string_end(chars: &[char], from: usize) -> (usize, bool) {
    let mut at = from;
    while at < chars.len() {
        match chars[at] {
            '\\' => at += 2,
            '"' => return (at + 1, true),
            _ => at += 1,
        }
    }
    (chars.len(), false)
}

fn string_spans(chars: &[char], start: usize, end: usize, face: Highlight, spans: &mut Spans) {
    let mut piece = start;
    let mut at = start;
    while at < end {
        match chars[at] {
            '\\' => at += 2,
            '{' if chars.get(at + 1) == Some(&'{') => at += 2,
            '{' => match (at + 1..end).find(|index| chars[*index] == '}') {
                Some(close) => {
                    if piece < at {
                        spans.push((piece..at, face));
                    }
                    spans.push((at..close + 1, Highlight::Attribute));
                    at = close + 1;
                    piece = at;
                }
                None => at += 1,
            },
            _ => at += 1,
        }
    }
    if piece < end {
        spans.push((piece..end, face));
    }
}

fn block_end(chars: &[char], mut at: usize, mut depth: usize) -> (usize, usize) {
    while at < chars.len() && depth > 0 {
        match (chars[at], chars.get(at + 1)) {
            ('|', Some('#')) => {
                depth -= 1;
                at += 2;
            }
            ('#', Some('|')) => {
                depth += 1;
                at += 2;
            }
            _ => at += 1,
        }
    }
    (at, depth)
}

fn skipped_end(chars: &[char], mut at: usize, mut depth: usize) -> (usize, Option<State>) {
    while at < chars.len() {
        let skipped = Some(depth);
        match (chars[at], chars.get(at + 1)) {
            ('\\', _) | ('#', Some(';')) => at += 2,
            (';', _) => at = chars.len(),
            ('#', Some('|')) => {
                let (end, block) = block_end(chars, at + 2, 1);
                if block > 0 {
                    let open = State {
                        block,
                        skipped,
                        ..State::default()
                    };
                    return (end, Some(open));
                }
                at = end;
            }
            ('"', _) => {
                let (end, closed) = string_end(chars, at + 1);
                if !closed {
                    let open = State {
                        in_string: true,
                        skipped,
                        ..State::default()
                    };
                    return (end, Some(open));
                }
                at = end;
            }
            ('(' | '[' | '{', _) => {
                depth += 1;
                at += 1;
            }
            (')' | ']' | '}', _) => {
                depth -= 1;
                at += 1;
                if depth == 0 {
                    return (at, None);
                }
            }
            _ => at += 1,
        }
    }
    let open = State {
        skipped: Some(depth),
        ..State::default()
    };
    (chars.len(), Some(open))
}

fn datum_end(chars: &[char], from: usize) -> (usize, Option<State>) {
    let Some(at) = (from..chars.len())
        .find(|at| !(chars[*at].is_whitespace() || ",'`~@#".contains(chars[*at])))
    else {
        return (chars.len(), None);
    };
    match chars[at] {
        '(' | '[' | '{' => skipped_end(chars, at + 1, 1),
        '"' => {
            let (end, closed) = string_end(chars, at + 1);
            let open = State {
                in_string: true,
                skipped: Some(0),
                ..State::default()
            };
            (end, (!closed).then_some(open))
        }
        '\\' => (word_end(chars, at + 2), None),
        _ => (word_end(chars, at), None),
    }
}

fn resume(chars: &[char], state: State, spans: &mut Spans) -> Result<usize, State> {
    let mut at = 0;
    let mut open = None;
    if state.block > 0 {
        let (end, block) = block_end(chars, 0, state.block);
        at = end;
        if block > 0 {
            open = Some(State {
                block,
                skipped: state.skipped,
                ..State::default()
            });
        }
    } else if state.in_string {
        let (end, closed) = string_end(chars, 0);
        if state.skipped.is_none() {
            string_spans(chars, 0, end, Highlight::String, spans);
        }
        at = end;
        if !closed {
            open = Some(State {
                in_string: true,
                skipped: state.skipped,
                ..State::default()
            });
        }
    }
    if let (None, Some(depth @ 1..)) = (open, state.skipped) {
        (at, open) = skipped_end(chars, at, depth);
    }
    if (state.block > 0 || state.skipped.is_some()) && at > 0 {
        spans.push((0..at, Highlight::Comment));
    }
    open.map_or(Ok(at), Err)
}

fn word_end(chars: &[char], from: usize) -> usize {
    (from..chars.len())
        .find(|index| delimits(chars[*index]))
        .unwrap_or(chars.len())
}

fn closing_brace(chars: &[char], open: usize) -> usize {
    let mut depth = 0_usize;
    for (index, character) in chars.iter().enumerate().skip(open) {
        match character {
            '{' => depth += 1,
            '}' if depth <= 1 => return index + 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    chars.len()
}

fn reader_token(chars: &[char], at: usize) -> Option<(usize, Highlight)> {
    let next = chars.get(at + 1).copied();
    match chars[at] {
        '^' if next == Some('{') => Some((closing_brace(chars, at + 1), Highlight::Attribute)),
        '^' if next == Some(':') => Some((word_end(chars, at + 1), Highlight::Attribute)),
        '\\' if next.is_some() => Some((word_end(chars, at + 2), Highlight::Constant)),
        '\'' if next.is_some_and(starts_symbol) => {
            Some((word_end(chars, at + 1), Highlight::Constant))
        }
        '~' if next == Some('@') => Some((at + 2, Highlight::Macro)),
        '\'' | '`' | '~' | '@' => Some((at + 1, Highlight::Macro)),
        _ => None,
    }
}

fn highlight_with(text: &str, state: State) -> (Spans, State) {
    highlight_in(text, state, &EDITOR_FORMS)
}

fn highlight_in(text: &str, state: State, known: &Forms) -> (Spans, State) {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut at = match resume(&chars, state, &mut spans) {
        Ok(end) => end,
        Err(open) => return (spans, open),
    };
    let mut defining = false;
    let mut summary = false;
    let mut head: Option<String> = None;
    let mut after_paren = false;
    let mut first = at == 0;
    let next = State {
        after_def: text.trim_start().starts_with("(def"),
        ..State::default()
    };
    while at < chars.len() {
        let character = chars[at];
        let start = at;
        let comment = match (character, chars.get(at + 1)) {
            ('#', Some('|')) => {
                let (end, block) = block_end(&chars, at + 2, 1);
                let open = State {
                    block,
                    ..State::default()
                };
                Some((end, (block > 0).then_some(open)))
            }
            ('#', Some(';')) => Some(datum_end(&chars, at + 2)),
            _ => None,
        };
        if let Some((end, open)) = comment {
            spans.push((start..end, Highlight::Comment));
            if let Some(open) = open {
                return (spans, open);
            }
            at = end;
            continue;
        }
        if character == ';' {
            spans.push((start..chars.len(), Highlight::Comment));
            break;
        }
        if character == '"' {
            let (end, closed) = string_end(&chars, at + 1);
            let face = if summary || (first && state.after_def) {
                Highlight::DocComment
            } else {
                Highlight::String
            };
            string_spans(&chars, start, end, face, &mut spans);
            if !closed {
                let open = State {
                    in_string: true,
                    ..State::default()
                };
                return (spans, open);
            }
            at = end;
            (after_paren, summary, defining, first) = (false, false, false, false);
            continue;
        }
        if let Some((end, face)) = reader_token(&chars, at) {
            spans.push((start..end, face));
            at = end;
            (after_paren, first) = (false, false);
            continue;
        }
        if starts_symbol(character) {
            while at < chars.len() && !delimits(chars[at]) {
                at += 1;
            }
            let word: String = chars[start..at].iter().collect();
            spans.extend(word_spans(&word, start, after_paren, defining, known));
            summary = defining && head.as_deref().is_some_and(|head| !forms::special(head));
            defining = after_paren && known.defines(&word) == Some(0);
            if defining {
                head = Some(word);
            }
            (after_paren, first) = (false, false);
            continue;
        }
        after_paren = character == '(';
        if !character.is_whitespace() {
            first = false;
        }
        at += 1;
    }
    (spans, next)
}

#[cfg(test)]
#[cfg(test)]
pub(crate) fn highlight_line(text: &str) -> Spans {
    highlight_with(text, State::default()).0
}

pub(crate) fn highlight_lines(lines: &[&str]) -> Vec<Spans> {
    let mut state = State::default();
    lines
        .iter()
        .map(|line| {
            let (spans, next) = highlight_with(line, state);
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

fn after(text: &str, state: State) -> State {
    let state = if top_level(text) {
        State::default()
    } else {
        state
    };
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
    let state = highlight_with(&buffer.line_text(first), State::default()).1;
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
            let state = self.states[self.states.len() - 1];
            self.states.push(after(&buffer.line_text(last), state));
        }
        self.states[line - self.first]
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
    highlight_in(&buffer.line_text(line), state, forms_of(editor, buffer)).0
}
const LOOK_BACK: usize = 2000;

const OX_FMT: Formatter = Formatter {
    name: "ox fmt",
    format: |editor, request| {
        let known = forms_at(editor, Some(request.path))
            .clone()
            .scanned(request.text);
        ox::format::format_with(request.text, &known)
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
    ox::format::indentation_with(&before, forms_of(editor, buffer))
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
                (27..33, Highlight::Attribute),
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
            super::highlight_with(&buffer.line_text(earlier), state).1
        });
        super::highlight_with(&buffer.line_text(line), state).0
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
