use std::cmp::Reverse;
use std::iter;
use std::ops::Range;
use std::sync::LazyLock;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor, Tree};

use crate::display::runs;
use crate::syntax::{Grammar, Syntax};
use crate::text::Buffer;
use crate::theme::Highlight;

const RUST: &str = include_str!("../../queries/rust/highlights.scm");
const TOML: &str = include_str!("../../queries/toml/highlights.scm");
const MARKDOWN: &str = include_str!("../../queries/markdown/highlights.scm");
const MARKDOWN_INLINE: &str = include_str!("../../queries/markdown/inline.scm");
const R: &str = include_str!("../../queries/r/highlights.scm");

struct Highlighter {
    query: Query,
    faces: Vec<Option<Highlight>>,
}

static RUST_HIGHLIGHTER: LazyLock<Option<Highlighter>> =
    LazyLock::new(|| Highlighter::new(Grammar::Rust, RUST));
static TOML_HIGHLIGHTER: LazyLock<Option<Highlighter>> =
    LazyLock::new(|| Highlighter::new(Grammar::Toml, TOML));
static MARKDOWN_HIGHLIGHTER: LazyLock<Option<Highlighter>> =
    LazyLock::new(|| Highlighter::new(Grammar::Markdown, MARKDOWN));
static MARKDOWN_INLINE_HIGHLIGHTER: LazyLock<Option<Highlighter>> =
    LazyLock::new(|| Highlighter::new(Grammar::MarkdownInline, MARKDOWN_INLINE));
static R_HIGHLIGHTER: LazyLock<Option<Highlighter>> =
    LazyLock::new(|| Highlighter::new(Grammar::R, R));

impl Highlighter {
    fn new(grammar: Grammar, source: &str) -> Option<Self> {
        let query = Query::new(&grammar.language(), source).ok()?;
        let faces = query
            .capture_names()
            .iter()
            .map(|name| face(name))
            .collect();
        Some(Self { query, faces })
    }

    fn of(grammar: Grammar) -> Option<&'static Self> {
        match grammar {
            Grammar::Rust => RUST_HIGHLIGHTER.as_ref(),
            Grammar::Toml => TOML_HIGHLIGHTER.as_ref(),
            Grammar::Markdown => MARKDOWN_HIGHLIGHTER.as_ref(),
            Grammar::MarkdownInline => MARKDOWN_INLINE_HIGHLIGHTER.as_ref(),
            Grammar::R => R_HIGHLIGHTER.as_ref(),
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
        "heading" => Highlight::Heading,
        "link" => Highlight::Link,
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
        let Some(tree) = self.synced(buffer) else {
            return Vec::new();
        };
        let start = buffer.line_start(line);
        let end = buffer.line_end(line);
        let bytes = buffer.char_to_byte(start)..buffer.char_to_byte(end);
        let mut faces = vec![None; end - start];
        let span = Span {
            buffer,
            start,
            bytes: bytes.clone(),
        };
        span.paint(&mut faces, self.language.grammar(), tree);
        for layer in &self.layers {
            if layer.replaces {
                for range in &layer.ranges {
                    if let Some(chars) = span.chars(range) {
                        faces[chars].fill(None);
                    }
                }
            }
            span.paint(&mut faces, layer.grammar, &layer.tree);
        }
        runs(&faces)
    }
}

struct Span<'a> {
    buffer: &'a Buffer,
    start: usize,
    bytes: Range<usize>,
}

impl Span<'_> {
    fn chars(&self, range: &Range<usize>) -> Option<Range<usize>> {
        let from = range.start.max(self.bytes.start);
        let to = range.end.min(self.bytes.end);
        (from < to).then(|| {
            self.buffer.byte_to_char(from) - self.start..self.buffer.byte_to_char(to) - self.start
        })
    }

    fn paint(&self, faces: &mut [Option<Highlight>], grammar: Grammar, tree: &Tree) {
        let Some(highlighter) = Highlighter::of(grammar) else {
            return;
        };
        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(self.bytes.clone());
        let text = |node: Node<'_>| iter::once(self.buffer.text_in_bytes(node.byte_range()));
        let mut captures = cursor.captures(&highlighter.query, tree.root_node(), text);
        let mut found = Vec::new();
        while let Some((matched, index)) = captures.next() {
            let capture = matched.captures()[*index];
            let Some(Some(highlight)) = highlighter.faces.get(capture.index as usize) else {
                continue;
            };
            let range = capture.node.byte_range();
            if let Some(chars) = self.chars(&range) {
                found.push((chars, range.len(), matched.pattern_index, *highlight));
            }
        }
        found.sort_by_key(|(_, length, pattern, highlight)| {
            (contains_others(*highlight), Reverse(*length), *pattern)
        });
        for (chars, _, _, highlight) in found {
            faces[chars].fill(Some(highlight));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::Language;

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
        let text = "[package]\nname = \"oxmacs\" # c\nversion = 1\n[[bin]]\n";
        let toml = |line| faces_in(Language::Toml, text, line);

        assert_eq!(toml(0), [(0..9, Highlight::Type)]);
        assert_eq!(
            toml(1),
            [
                (0..4, Highlight::Function),
                (7..15, Highlight::String),
                (16..19, Highlight::Comment)
            ]
        );
        assert_eq!(
            toml(2),
            [(0..7, Highlight::Function), (10..11, Highlight::Constant)]
        );
        assert_eq!(toml(3), [(0..7, Highlight::Type)]);
    }

    #[test]
    fn markdown_blocks_are_highlighted() {
        let text = "# Title\n\n- item\n\n```\ncode\n```\n\n> quote\n";
        let markdown = |line| faces_in(Language::Markdown, text, line);

        assert_eq!(markdown(0), [(0..7, Highlight::Heading)]);
        assert_eq!(markdown(2), [(0..2, Highlight::Keyword)]);
        assert_eq!(markdown(4), [(0..3, Highlight::Comment)]);
        assert_eq!(markdown(5), [(0..4, Highlight::String)]);
        assert_eq!(markdown(8), [(0..7, Highlight::DocComment)]);
    }

    #[test]
    fn markdown_inlines_are_highlighted() {
        let text = "Some *em* and **strong** with `code` and [link](url).\n# A *b*\n";
        let markdown = |line| faces_in(Language::Markdown, text, line);

        assert_eq!(
            markdown(0),
            [
                (5..9, Highlight::Constant),
                (14..24, Highlight::Keyword),
                (30..36, Highlight::String),
                (42..46, Highlight::Link),
                (48..51, Highlight::Comment)
            ]
        );
        assert_eq!(
            markdown(1),
            [(0..4, Highlight::Heading), (4..7, Highlight::Constant)]
        );
    }

    #[test]
    fn fenced_code_is_highlighted_in_its_language() {
        let text = "```rust\nfn a() {}\n```\n\n```py\nx = 1\n```\n";
        let markdown = |line| faces_in(Language::Markdown, text, line);

        assert_eq!(
            markdown(1),
            [(0..2, Highlight::Keyword), (3..4, Highlight::Function)]
        );
        assert_eq!(markdown(5), [(0..5, Highlight::String)]);
    }

    #[test]
    fn r_definitions_calls_and_literals_are_highlighted() {
        let text = "#' Adds one\nadd <- function(x = 1L) {\n  if (is.na(x)) return(NULL)\n  stats::median(x) + 2 # two\n}\n";
        let r = |line| faces_in(Language::R, text, line);

        assert_eq!(r(0), [(0..11, Highlight::DocComment)]);
        assert_eq!(
            r(1),
            [
                (0..3, Highlight::Function),
                (7..15, Highlight::Keyword),
                (20..22, Highlight::Constant)
            ]
        );
        assert_eq!(
            r(2),
            [
                (2..4, Highlight::Keyword),
                (6..11, Highlight::Function),
                (16..22, Highlight::Keyword),
                (23..27, Highlight::Constant)
            ]
        );
        assert_eq!(
            r(3),
            [
                (2..7, Highlight::Type),
                (9..15, Highlight::Function),
                (21..22, Highlight::Constant),
                (23..28, Highlight::Comment)
            ]
        );
    }

    #[test]
    fn r_strings_are_highlighted_whole() {
        let text = "x <- \"a # b\" # c\n";

        assert_eq!(
            faces_in(Language::R, text, 0),
            [(5..12, Highlight::String), (13..16, Highlight::Comment)]
        );
    }

    #[test]
    fn fenced_r_code_is_highlighted_as_r() {
        let text = "```r\nf <- function() NULL\n```\n";

        assert_eq!(
            faces_in(Language::Markdown, text, 1),
            [
                (0..1, Highlight::Function),
                (5..13, Highlight::Keyword),
                (16..20, Highlight::Constant)
            ]
        );
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
