use std::ops::Range;

use oxmacs_core::editor::Editor;
use oxmacs_core::syntax::{Feature, Indentation, Language, LanguageSpec, Outline};
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

const KEYWORDS: &[&str] = &[
    "as", "break", "catch", "const", "continue", "do", "else", "export", "fn", "for", "global",
    "if", "import", "in", "let", "loop", "private", "return", "switch", "this", "throw", "try",
    "until", "while",
];
const CONSTANTS: &[&str] = &["false", "true"];

static SPEC: LanguageSpec = LanguageSpec {
    name: "Rhai",
    extensions: &["rhai"],
    file_names: &[],
    aliases: &["rhai"],
    line_comment: Some("//"),
    pairs: &[],
    indentation: Indentation::KEEP,
    grammar: None,
    grammars: &[],
    outline: Outline::NONE,
    features: &[Feature::Highlighting, Feature::Comments],
};

pub(crate) const LANGUAGE: Language = Language::new(&SPEC);

pub(crate) fn highlight(
    editor: &Editor,
    id: BufferId,
    line: usize,
) -> Vec<(Range<usize>, Highlight)> {
    let Some(buffer) = editor.buffer_by_id(id) else {
        return Vec::new();
    };
    spans(&buffer.text_in(buffer.line_start(line)..buffer.line_end(line)))
}

fn spans(text: &str) -> Vec<(Range<usize>, Highlight)> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let start = at;
        match chars[at] {
            '/' if chars.get(at + 1) == Some(&'/') => {
                spans.push((start..chars.len(), Highlight::Comment));
                break;
            }
            '"' | '`' => {
                at = closing(&chars, at);
                spans.push((start..at, Highlight::String));
            }
            first if first.is_alphabetic() || first == '_' => {
                at = word_end(&chars, at);
                let word: String = chars[start..at].iter().collect();
                if let Some(highlight) = word_highlight(&word) {
                    spans.push((start..at, highlight));
                }
            }
            _ => at += 1,
        }
    }
    spans
}

fn closing(chars: &[char], open: usize) -> usize {
    let quote = chars[open];
    let mut at = open + 1;
    while at < chars.len() {
        match chars[at] {
            '\\' => at += 2,
            found if found == quote => return at + 1,
            _ => at += 1,
        }
    }
    chars.len()
}

fn word_end(chars: &[char], start: usize) -> usize {
    chars[start..]
        .iter()
        .position(|found| !found.is_alphanumeric() && *found != '_')
        .map_or(chars.len(), |length| start + length)
}

fn word_highlight(word: &str) -> Option<Highlight> {
    if KEYWORDS.contains(&word) {
        Some(Highlight::Keyword)
    } else if CONSTANTS.contains(&word) {
        Some(Highlight::Constant)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::theme::Highlight;

    use super::spans;

    #[test]
    fn keywords_strings_and_comments_are_highlighted() {
        assert_eq!(
            spans("let s = \"if // no\"; // note"),
            [
                (0..3, Highlight::Keyword),
                (8..18, Highlight::String),
                (20..27, Highlight::Comment)
            ]
        );
    }

    #[test]
    fn true_and_false_are_constants_and_other_names_are_plain() {
        assert_eq!(
            spans("if done == true { lettuce }"),
            [(0..2, Highlight::Keyword), (11..15, Highlight::Constant)]
        );
    }

    #[test]
    fn escaped_quotes_stay_inside_strings() {
        assert_eq!(spans("\"a\\\"b\" + x"), [(0..6, Highlight::String)]);
    }
}
