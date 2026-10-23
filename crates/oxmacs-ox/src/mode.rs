use std::ops::Range;

use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::{Editor, Formatter};
use oxmacs_core::syntax::{Feature, Indentation, Language, LanguageSpec, Outline};
use oxmacs_core::text::BufferId;
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

const SPECIAL: &[&str] = &[
    "def",
    "defmacro",
    "defrecord",
    "deferror",
    "defonce",
    "defcommand",
    "defrule",
    "defrules",
    "fn",
    "let",
    "if",
    "do",
    "when",
    "unless",
    "cond",
    "match",
    "try",
    "catch",
    "finally",
    "raise",
    "set!",
    "quote",
    "module",
    "import",
    "export",
    "and",
    "or",
    "spawn",
    "gen",
    "await",
    "yield",
    "when-let",
    "if-let",
    "plugin",
    "use-plugin",
    "else",
];

const DEFINING: &[&str] = &[
    "def",
    "defmacro",
    "defrecord",
    "deferror",
    "defonce",
    "defcommand",
    "defrule",
    "defrules",
];

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
}

fn number(word: &str) -> bool {
    word.trim_start_matches(['-', '+'])
        .starts_with(|character: char| character.is_ascii_digit())
}

fn word_spans(word: &str, start: usize, after_paren: bool, defining: bool) -> Spans {
    let end = start + word.chars().count();
    let offset = |index: usize| start + word[..index].chars().count();
    if word.starts_with(':') || word.ends_with(':') || number(word) || CONSTANTS.contains(&word) {
        return vec![(start..end, Highlight::Constant)];
    }
    if defining {
        return vec![(start..end, Highlight::Function)];
    }
    if after_paren && SPECIAL.contains(&word) {
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

const SUMMARIZED: &[&str] = &["defcommand", "defmode"];

fn word_end(chars: &[char], from: usize) -> usize {
    (from..chars.len())
        .find(|index| delimits(chars[*index]))
        .unwrap_or(chars.len())
}

fn reader_token(chars: &[char], at: usize) -> Option<(usize, Highlight)> {
    let next = chars.get(at + 1).copied();
    match chars[at] {
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
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut at = 0;
    let mut defining = false;
    let mut summary = false;
    let mut head: Option<String> = None;
    let mut after_paren = false;
    let mut first = true;
    let next = State {
        in_string: false,
        after_def: text.trim_start().starts_with("(def"),
    };
    if state.in_string {
        let (end, closed) = string_end(&chars, 0);
        string_spans(&chars, 0, end, Highlight::String, &mut spans);
        if !closed {
            return (
                spans,
                State {
                    in_string: true,
                    after_def: false,
                },
            );
        }
        at = end;
        first = false;
    }
    while at < chars.len() {
        let character = chars[at];
        let start = at;
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
                return (
                    spans,
                    State {
                        in_string: true,
                        after_def: false,
                    },
                );
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
            spans.extend(word_spans(&word, start, after_paren, defining));
            summary = defining
                && head
                    .as_deref()
                    .is_some_and(|head| SUMMARIZED.contains(&head));
            defining = after_paren && DEFINING.contains(&word.as_str());
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

fn top_level(line: &str) -> bool {
    ["(def", "(module", "(plugin", "(use-plugin"]
        .iter()
        .any(|head| line.starts_with(head))
}

fn highlight(editor: &Editor, id: BufferId, line: usize) -> Spans {
    let Some(buffer) = editor.buffer_by_id(id) else {
        return Vec::new();
    };
    let floor = line.saturating_sub(HIGHLIGHT_LOOK_BACK);
    let first = (floor..line)
        .rev()
        .find(|candidate| top_level(&buffer.line_text(*candidate)))
        .unwrap_or(floor);
    let mut state = State::default();
    for earlier in first..line {
        state = highlight_with(&buffer.line_text(earlier), state).1;
    }
    highlight_with(&buffer.line_text(line), state).0
}
const LOOK_BACK: usize = 2000;

const OX_FMT: Formatter = Formatter {
    name: "ox fmt",
    format: |_, request| {
        ox::format::format(request.text).map_err(|problem| format!("ox fmt: {problem}"))
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
    ox::format::indentation(&before)
}

pub fn install(editor: &mut Editor, scope: ScopeId) {
    editor.add_language(language(), Some(scope));
    editor.set_highlighter(language(), highlight);
    editor.set_indenter(language(), indent);
    editor.add_formatter(language(), OX_FMT);
}

#[cfg(test)]
mod tests {
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
