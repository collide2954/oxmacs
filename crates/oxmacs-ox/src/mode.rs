use std::ops::Range;

use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::Editor;
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

fn word_face(word: &str, after_paren: bool, defining: bool) -> Option<Highlight> {
    let number = word
        .trim_start_matches(['-', '+'])
        .starts_with(|character: char| character.is_ascii_digit());
    if word.starts_with(':') || word.ends_with(':') || number || CONSTANTS.contains(&word) {
        Some(Highlight::Constant)
    } else if defining {
        Some(Highlight::Function)
    } else if after_paren && SPECIAL.contains(&word) {
        Some(Highlight::Keyword)
    } else {
        None
    }
}

pub fn highlight_line(text: &str) -> Vec<(Range<usize>, Highlight)> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut at = 0;
    let mut defining = false;
    let mut after_paren = false;
    while at < chars.len() {
        let character = chars[at];
        let start = at;
        if character == ';' {
            spans.push((start..chars.len(), Highlight::Comment));
            break;
        }
        if character == '"' {
            at += 1;
            while at < chars.len() && chars[at] != '"' {
                at += if chars[at] == '\\' { 2 } else { 1 };
            }
            at = (at + 1).min(chars.len());
            spans.push((start..at, Highlight::String));
            after_paren = false;
            continue;
        }
        if character == '\\' && at + 1 < chars.len() {
            at += 2;
            while at < chars.len() && !delimits(chars[at]) {
                at += 1;
            }
            spans.push((start..at, Highlight::Constant));
            after_paren = false;
            continue;
        }
        if starts_symbol(character) {
            while at < chars.len() && !delimits(chars[at]) {
                at += 1;
            }
            let word: String = chars[start..at].iter().collect();
            if let Some(face) = word_face(&word, after_paren, defining) {
                spans.push((start..at, face));
            }
            defining = after_paren && DEFINING.contains(&word.as_str());
            after_paren = false;
            continue;
        }
        after_paren = character == '(';
        at += 1;
    }
    spans
}

fn highlight(editor: &Editor, id: BufferId, line: usize) -> Vec<(Range<usize>, Highlight)> {
    editor
        .buffer_by_id(id)
        .map(|buffer| highlight_line(&buffer.line_text(line)))
        .unwrap_or_default()
}

pub fn install(editor: &mut Editor, scope: ScopeId) {
    editor.add_language(language(), Some(scope));
    editor.set_highlighter(language(), highlight);
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::syntax::Language;
    use oxmacs_core::theme::Highlight;

    use super::highlight_line;
    use crate::Ox;

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

    #[test]
    fn lines_are_highlighted_by_what_they_hold() {
        let spans = highlight_line(r#"(def (greet name) (str "Hi {name}" :loud 42 nil)) ; say hi"#);

        assert_eq!(
            spans,
            [
                (1..4, Highlight::Keyword),
                (6..11, Highlight::Function),
                (23..34, Highlight::String),
                (35..40, Highlight::Constant),
                (41..43, Highlight::Constant),
                (44..47, Highlight::Constant),
                (50..58, Highlight::Comment),
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
