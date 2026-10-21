use oxmacs_core::syntax::{Feature, Grammar, Indentation, Language, LanguageSpec, Outline};
use oxmacs_core::text::Buffer;
use tree_sitter::Node;

static GRAMMAR: Grammar = Grammar::new(
    "python",
    || tree_sitter_python::LANGUAGE.into(),
    include_str!("../queries/highlights.scm"),
    "",
);

static SPEC: LanguageSpec = LanguageSpec {
    name: "Python",
    extensions: &["py", "pyi"],
    file_names: &[],
    aliases: &["python", "py"],
    line_comment: Some("#"),
    pairs: &[('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\'')],
    indentation: Indentation {
        guess: Some(crate::indent::guess),
        ..Indentation::KEEP
    },
    grammar: Some(&GRAMMAR),
    grammars: &[],
    outline: Outline {
        item,
        context: is_definition,
        defun: |node| is_definition(node) || node.kind() == "decorated_definition",
        sections: false,
    },
    features: &[
        Feature::Highlighting,
        Feature::Indentation,
        Feature::Comments,
        Feature::Pairs,
        Feature::Formatting,
        Feature::Lint,
        Feature::Motion,
        Feature::Items,
        Feature::Repl,
    ],
};

pub const LANGUAGE: Language = Language::new(&SPEC);

fn is_definition(node: Node<'_>) -> bool {
    matches!(node.kind(), "function_definition" | "class_definition")
}

fn item(node: Node<'_>, buffer: &Buffer) -> Option<String> {
    let asynchronous = node.child(0).is_some_and(|first| first.kind() == "async");
    let keyword = match node.kind() {
        "class_definition" => "class",
        "function_definition" if asynchronous => "async def",
        "function_definition" => "def",
        _ => return None,
    };
    let name = node.child_by_field_name("name")?;
    Some(format!(
        "{keyword} {}",
        buffer.text_in_bytes(name.byte_range())
    ))
}

#[cfg(test)]
mod tests {
    use std::ops::Range;
    use std::path::Path;

    use oxmacs_core::syntax::Syntax;
    use oxmacs_core::text::Buffer;
    use oxmacs_core::theme::Highlight;

    use super::LANGUAGE;

    fn faces(text: &str, line: usize) -> Vec<(Range<usize>, Highlight)> {
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::new(LANGUAGE);
        syntax.sync(&mut buffer);
        syntax.highlights(&buffer, line)
    }

    const SAMPLE: &str = r#""""Module docs."""
import os

MAX = 10


@dataclass
class Point(Base):
    """A point."""

    def norm(self, scale: float = 1.0) -> float:
        # length
        return math.sqrt(self.x) * scale if True else None
"#;

    #[test]
    fn keywords_names_and_literals_are_highlighted() {
        assert_eq!(faces(SAMPLE, 1), [(0..6, Highlight::Keyword)]);
        assert_eq!(
            faces(SAMPLE, 3),
            [(0..3, Highlight::Constant), (6..8, Highlight::Constant)]
        );
        assert_eq!(
            faces(SAMPLE, 7),
            [
                (0..5, Highlight::Keyword),
                (6..11, Highlight::Type),
                (12..16, Highlight::Type)
            ]
        );
        assert_eq!(
            faces(SAMPLE, 10),
            [
                (4..7, Highlight::Keyword),
                (8..12, Highlight::Function),
                (26..31, Highlight::Type),
                (34..37, Highlight::Constant),
                (42..47, Highlight::Type)
            ]
        );
        assert_eq!(
            faces(SAMPLE, 12),
            [
                (8..14, Highlight::Keyword),
                (20..24, Highlight::Function),
                (41..43, Highlight::Keyword),
                (44..48, Highlight::Constant),
                (49..53, Highlight::Keyword),
                (54..58, Highlight::Constant)
            ]
        );
    }

    #[test]
    fn comments_decorators_strings_and_docstrings_are_highlighted() {
        assert_eq!(faces(SAMPLE, 0), [(0..18, Highlight::DocComment)]);
        assert_eq!(faces(SAMPLE, 6), [(0..10, Highlight::Attribute)]);
        assert_eq!(faces(SAMPLE, 8), [(4..18, Highlight::DocComment)]);
        assert_eq!(faces(SAMPLE, 11), [(8..16, Highlight::Comment)]);
        assert_eq!(
            faces("x = f\"a {b}\"  # c\n", 0),
            [(4..12, Highlight::String), (14..17, Highlight::Comment)]
        );
    }

    #[test]
    fn strings_after_the_first_statement_are_not_docstrings() {
        let text = "def f():\n    x = 1\n    \"\"\"Not docs.\"\"\"\n";

        assert_eq!(faces(text, 2), [(4..19, Highlight::String)]);
    }

    #[test]
    fn classes_and_defs_are_items_inside_their_class_or_def() {
        let text = "class Shape:\n    def area(self):\n        pass\n\n    @property\n    def name(self):\n        return \"s\"\n\n\nasync def main():\n    def inner():\n        pass\n";
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::new(LANGUAGE);
        syntax.sync(&mut buffer);

        let items: Vec<(String, Option<String>)> = syntax
            .items(&buffer)
            .into_iter()
            .map(|item| (item.label, item.context))
            .collect();

        let shape = Some("class Shape".to_owned());
        assert_eq!(
            items,
            [
                ("class Shape".to_owned(), None),
                ("def area".to_owned(), shape.clone()),
                ("def name".to_owned(), shape),
                ("async def main".to_owned(), None),
                ("def inner".to_owned(), Some("async def main".to_owned())),
            ]
        );
    }

    #[test]
    fn decorated_definitions_are_whole_defuns() {
        let text = "x = 1\n\n@cache\ndef f():\n    return 2\n";
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::new(LANGUAGE);
        syntax.sync(&mut buffer);

        let around = syntax
            .defun_around(&buffer, 28)
            .map(|range| buffer.text_in(range));

        assert_eq!(around.as_deref(), Some("@cache\ndef f():\n    return 2"));
        assert_eq!(syntax.defun_around(&buffer, 2), None);
    }

    #[test]
    fn python_files_and_stubs_are_python() {
        assert_eq!(LANGUAGE.extensions(), ["py", "pyi"]);
        assert_eq!(LANGUAGE.line_comment(), Some("#"));
        assert_eq!(LANGUAGE.indentation().unit, 4);
        let known = [LANGUAGE];
        assert_eq!(
            oxmacs_core::syntax::language_for(&known, Path::new("stubs.PYI")),
            Some(LANGUAGE)
        );
    }
}
