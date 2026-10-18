use std::cmp::Reverse;
use std::iter;
use std::ops::Range;
use std::sync::LazyLock;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor};

use crate::syntax::{Language, Syntax};
use crate::text::Buffer;
use crate::theme::Highlight;

const RUST: &str = include_str!("../../queries/rust/highlights.scm");
const TOML: &str = include_str!("../../queries/toml/highlights.scm");

struct Highlighter {
    query: Query,
    faces: Vec<Option<Highlight>>,
}

static RUST_HIGHLIGHTER: LazyLock<Option<Highlighter>> =
    LazyLock::new(|| Highlighter::new(Language::Rust, RUST));
static TOML_HIGHLIGHTER: LazyLock<Option<Highlighter>> =
    LazyLock::new(|| Highlighter::new(Language::Toml, TOML));

impl Highlighter {
    fn new(language: Language, source: &str) -> Option<Self> {
        let query = Query::new(&language.grammar(), source).ok()?;
        let faces = query
            .capture_names()
            .iter()
            .map(|name| face(name))
            .collect();
        Some(Self { query, faces })
    }

    fn of(language: Language) -> Option<&'static Self> {
        match language {
            Language::Rust => RUST_HIGHLIGHTER.as_ref(),
            Language::Toml => TOML_HIGHLIGHTER.as_ref(),
        }
    }
}

fn face(capture: &str) -> Option<Highlight> {
    Some(match capture {
        "keyword" => Highlight::Keyword,
        "string" => Highlight::String,
        "comment" => Highlight::Comment,
        "comment.documentation" => Highlight::DocComment,
        "function" => Highlight::Function,
        "function.macro" => Highlight::Macro,
        "type" => Highlight::Type,
        "constant" => Highlight::Constant,
        "attribute" => Highlight::Attribute,
        "lifetime" => Highlight::Lifetime,
        _ => return None,
    })
}

fn contains_others(highlight: Highlight) -> bool {
    matches!(
        highlight,
        Highlight::Comment | Highlight::DocComment | Highlight::String | Highlight::Attribute
    )
}

impl Syntax {
    pub fn highlights(&self, buffer: &Buffer, line: usize) -> Vec<(Range<usize>, Highlight)> {
        let Some(highlighter) = Highlighter::of(self.language) else {
            return Vec::new();
        };
        let Some(tree) = self
            .tree
            .as_ref()
            .filter(|_| self.revision == Some(buffer.revision()))
        else {
            return Vec::new();
        };
        let start = buffer.line_start(line);
        let end = buffer.line_end(line);
        let bytes = buffer.char_to_byte(start)..buffer.char_to_byte(end);
        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(bytes.clone());
        let text = |node: Node<'_>| iter::once(buffer.text_in_bytes(node.byte_range()));
        let mut captures = cursor.captures(&highlighter.query, tree.root_node(), text);
        let mut found = Vec::new();
        while let Some((matched, index)) = captures.next() {
            let capture = matched.captures()[*index];
            let Some(Some(highlight)) = highlighter.faces.get(capture.index as usize) else {
                continue;
            };
            let range = capture.node.byte_range();
            let from = range.start.max(bytes.start);
            let to = range.end.min(bytes.end);
            if from < to {
                let chars = buffer.byte_to_char(from) - start..buffer.byte_to_char(to) - start;
                found.push((chars, range.len(), matched.pattern_index, *highlight));
            }
        }
        found.sort_by_key(|(_, length, pattern, highlight)| {
            (contains_others(*highlight), Reverse(*length), *pattern)
        });
        let mut faces = vec![None; end - start];
        for (chars, _, _, highlight) in found {
            faces[chars].fill(Some(highlight));
        }
        runs(&faces)
    }
}

fn runs(faces: &[Option<Highlight>]) -> Vec<(Range<usize>, Highlight)> {
    let mut runs: Vec<(Range<usize>, Highlight)> = Vec::new();
    for (index, face) in faces.iter().enumerate() {
        let Some(face) = face else {
            continue;
        };
        match runs.last_mut() {
            Some((range, last)) if range.end == index && last == face => range.end = index + 1,
            _ => runs.push((index..index + 1, *face)),
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn faces(text: &str, line: usize) -> Vec<(Range<usize>, Highlight)> {
        faces_in(Language::Rust, text, line)
    }

    fn faces_in(language: Language, text: &str, line: usize) -> Vec<(Range<usize>, Highlight)> {
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::new(language);
        syntax.sync(&mut buffer);
        syntax.highlights(&buffer, line)
    }

    #[test]
    fn keywords_strings_and_comments_are_highlighted() {
        assert_eq!(
            faces("let s = \"hi\"; // note", 0),
            [
                (0..3, Highlight::Keyword),
                (8..12, Highlight::String),
                (14..21, Highlight::Comment)
            ]
        );
    }

    #[test]
    fn functions_and_types_are_highlighted() {
        assert_eq!(
            faces("fn add(a: u32) -> Option<u32> { helper(a) }", 0),
            [
                (0..2, Highlight::Keyword),
                (3..6, Highlight::Function),
                (10..13, Highlight::Type),
                (18..24, Highlight::Type),
                (25..28, Highlight::Type),
                (32..38, Highlight::Function)
            ]
        );
    }

    #[test]
    fn macros_attributes_lifetimes_and_constants_are_highlighted() {
        let text = "#[derive(Debug)]\nconst MAX: u8 = 1;\nfn f<'a>() { println!(\"{}\", MAX); }";

        assert_eq!(faces(text, 0), [(0..16, Highlight::Attribute)]);
        assert_eq!(
            faces(text, 1),
            [
                (0..5, Highlight::Keyword),
                (6..9, Highlight::Constant),
                (11..13, Highlight::Type),
                (16..17, Highlight::Constant)
            ]
        );
        assert_eq!(
            faces(text, 2),
            [
                (0..2, Highlight::Keyword),
                (3..4, Highlight::Function),
                (5..7, Highlight::Lifetime),
                (13..21, Highlight::Macro),
                (22..26, Highlight::String),
                (28..31, Highlight::Constant)
            ]
        );
    }

    #[test]
    fn capitalized_calls_are_types_and_definitions_win() {
        let text = "const Max: u8 = 0;\nfn f() { Some(1); }";

        assert_eq!(faces(text, 0)[1], (6..9, Highlight::Constant));
        assert_eq!(faces(text, 1)[2], (9..13, Highlight::Type));
    }

    #[test]
    fn doc_comments_have_their_own_face() {
        let text = "/// Docs\n// plain\n";

        assert_eq!(faces(text, 0), [(0..8, Highlight::DocComment)]);
        assert_eq!(faces(text, 1), [(0..8, Highlight::Comment)]);
    }

    #[test]
    fn constructs_spanning_lines_are_highlighted_on_each() {
        let text = "/* a\nb */ x\nlet s = \"é\nü\";";

        assert_eq!(faces(text, 1), [(0..4, Highlight::Comment)]);
        assert_eq!(faces(text, 3), [(0..2, Highlight::String)]);
    }

    #[test]
    fn toml_tables_keys_and_values_are_highlighted() {
        let text = "[package]\nname = \"mx\" # c\nversion = 1\n[[bin]]\n";
        let toml = |line| faces_in(Language::Toml, text, line);

        assert_eq!(toml(0), [(0..9, Highlight::Type)]);
        assert_eq!(
            toml(1),
            [
                (0..4, Highlight::Function),
                (7..11, Highlight::String),
                (12..15, Highlight::Comment)
            ]
        );
        assert_eq!(
            toml(2),
            [(0..7, Highlight::Function), (10..11, Highlight::Constant)]
        );
        assert_eq!(toml(3), [(0..7, Highlight::Type)]);
    }

    #[test]
    fn stale_trees_give_no_highlights() {
        let mut buffer = Buffer::with_text("t", "fn a() {}");
        let mut syntax = Syntax::new(Language::Rust);
        syntax.sync(&mut buffer);

        buffer.insert("x");

        assert_eq!(syntax.highlights(&buffer, 0), []);
    }
}
