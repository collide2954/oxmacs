mod highlight;
mod indent;
mod items;
mod layers;
pub(crate) mod motion;

use std::fmt;
use std::path::Path;

use tree_sitter::{InputEdit, Node, Parser, Point, Tree};

pub use items::Item;

use layers::{Grammar, Layer};

use crate::text::{Buffer, TextChange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Toml,
    Markdown,
    R,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Highlighting,
    Indentation,
    Comments,
    Pairs,
    Formatting,
    Lint,
    Motion,
    Items,
    Folding,
    Preview,
    Lsp,
}

impl Feature {
    pub const ALL: [Self; 11] = [
        Self::Highlighting,
        Self::Indentation,
        Self::Comments,
        Self::Pairs,
        Self::Formatting,
        Self::Lint,
        Self::Motion,
        Self::Items,
        Self::Folding,
        Self::Preview,
        Self::Lsp,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Highlighting => "Highlight",
            Self::Indentation => "Indent",
            Self::Comments => "Comment",
            Self::Pairs => "Pairs",
            Self::Formatting => "Format",
            Self::Lint => "Lint",
            Self::Motion => "Motion",
            Self::Items => "Items",
            Self::Folding => "Fold",
            Self::Preview => "Preview",
            Self::Lsp => "LSP",
        }
    }
}

impl Language {
    pub const ALL: [Self; 4] = [Self::Rust, Self::Toml, Self::Markdown, Self::R];

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["rs"],
            Self::Toml => &["toml"],
            Self::Markdown => &["md", "markdown"],
            Self::R => &["r"],
        }
    }

    pub fn line_comment(self) -> Option<&'static str> {
        match self {
            Self::Rust => Some("//"),
            Self::Toml | Self::R => Some("#"),
            Self::Markdown => None,
        }
    }

    pub fn supports(self, feature: Feature) -> bool {
        matches!(
            (self, feature),
            (
                Self::Rust,
                Feature::Highlighting
                    | Feature::Indentation
                    | Feature::Comments
                    | Feature::Pairs
                    | Feature::Formatting
                    | Feature::Lint
                    | Feature::Motion
                    | Feature::Items
            ) | (
                Self::Toml,
                Feature::Highlighting
                    | Feature::Indentation
                    | Feature::Comments
                    | Feature::Pairs
                    | Feature::Motion
                    | Feature::Items
            ) | (
                Self::Markdown,
                Feature::Highlighting
                    | Feature::Indentation
                    | Feature::Motion
                    | Feature::Items
                    | Feature::Preview
            ) | (Self::R, Feature::Highlighting)
        )
    }

    pub fn pairs(self) -> &'static [(char, char)] {
        match self {
            Self::Rust | Self::Toml => &[('(', ')'), ('[', ']'), ('{', '}'), ('"', '"')],
            Self::R => &[
                ('(', ')'),
                ('[', ']'),
                ('{', '}'),
                ('"', '"'),
                ('\'', '\''),
                ('`', '`'),
            ],
            Self::Markdown => &[],
        }
    }

    pub fn for_path(path: &Path) -> Option<Self> {
        if path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("Cargo.lock"))
        {
            return Some(Self::Toml);
        }
        if path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(".Rprofile"))
        {
            return Some(Self::R);
        }
        let extension = path.extension()?.to_str()?;
        Self::ALL.into_iter().find(|language| {
            language
                .extensions()
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Toml => "TOML",
            Self::Markdown => "Markdown",
            Self::R => "R",
        }
    }
}

pub struct Syntax {
    language: Language,
    parser: Parser,
    tree: Option<Tree>,
    revision: Option<u64>,
    layers: Vec<Layer>,
    injectors: Vec<(Grammar, Parser)>,
}

impl fmt::Debug for Syntax {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Syntax")
            .field("language", &self.language)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

impl Syntax {
    pub fn new(language: Language) -> Self {
        let mut parser = Parser::new();
        let _ = parser.set_language(&language.grammar().language());
        Self {
            language,
            parser,
            tree: None,
            revision: None,
            layers: Vec::new(),
            injectors: Vec::new(),
        }
    }

    pub fn language(&self) -> Language {
        self.language
    }

    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    fn synced(&self, buffer: &Buffer) -> Option<&Tree> {
        self.tree
            .as_ref()
            .filter(|_| self.revision == Some(buffer.revision()))
    }

    pub fn comment_start(&self, buffer: &Buffer, line: usize) -> Option<usize> {
        let tree = self.synced(buffer)?;
        let start = buffer.line_start(line);
        let end = buffer.line_end(line);
        let last = buffer.char_to_byte(end.checked_sub(1).filter(|last| *last >= start)?);
        let node = tree.root_node().descendant_for_byte_range(last, last)?;
        let comment = ancestors(node).find(|ancestor| {
            matches!(
                ancestor.kind(),
                "line_comment" | "block_comment" | "comment"
            )
        })?;
        let byte = comment.start_byte().max(buffer.char_to_byte(start));
        Some(buffer.byte_to_char(byte) - start)
    }

    pub fn bracket_partner(&self, buffer: &Buffer, position: usize) -> Option<usize> {
        let tree = self.synced(buffer)?;
        let byte = buffer.char_to_byte(position);
        let node = tree.root_node().descendant_for_byte_range(byte, byte + 1)?;
        if node.is_named() || node.start_byte() != byte {
            return None;
        }
        let kind = node.kind();
        let (partner, forward) = match kind {
            "(" => (")", true),
            "[" => ("]", true),
            "{" => ("}", true),
            ")" => ("(", false),
            "]" => ("[", false),
            "}" => ("{", false),
            _ => return None,
        };
        let parent = node.parent()?;
        let mut cursor = parent.walk();
        let siblings: Vec<Node<'_>> = parent.children(&mut cursor).collect();
        let index = siblings
            .iter()
            .position(|sibling| sibling.id() == node.id())?;
        let mut depth = 0_usize;
        let mut matches = |sibling: &&Node<'_>| {
            if sibling.is_named() || sibling.is_missing() {
                return false;
            }
            if sibling.kind() == kind {
                depth += 1;
            } else if sibling.kind() == partner {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            }
            false
        };
        let found = if forward {
            siblings[index + 1..].iter().find(&mut matches)
        } else {
            siblings[..index].iter().rev().find(&mut matches)
        }?;
        Some(buffer.byte_to_char(found.start_byte()))
    }

    pub fn sync(&mut self, buffer: &mut Buffer) {
        buffer.track_changes();
        let changes = buffer.take_changes();
        let revision = buffer.revision();
        if self.revision == Some(revision) && self.tree.is_some() {
            return;
        }
        let incremental = self.revision.is_some_and(|synced| {
            u64::try_from(changes.len()).is_ok_and(|count| synced + count == revision)
        });
        let mut old = self.tree.take().filter(|_| incremental);
        if let Some(tree) = &mut old {
            for change in &changes {
                tree.edit(&input_edit(change));
            }
        }
        let text: &Buffer = buffer;
        self.tree = self.parser.parse_with_options(
            &mut |byte, _| text.chunk_at_byte(byte).as_bytes(),
            old.as_ref(),
            None,
        );
        self.revision = Some(revision);
        self.inject(buffer);
    }
}

fn preorder<'tree>(tree: &'tree Tree, mut visit: impl FnMut(Node<'tree>)) {
    let mut cursor = tree.walk();
    loop {
        visit(cursor.node());
        if cursor.goto_first_child() || cursor.goto_next_sibling() {
            continue;
        }
        loop {
            if !cursor.goto_parent() {
                return;
            }
            if cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn ancestors(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    std::iter::successors(Some(node), Node::parent)
}

fn input_edit(change: &TextChange) -> InputEdit {
    InputEdit {
        start_byte: change.start_byte,
        old_end_byte: change.old_end_byte,
        new_end_byte: change.new_end_byte,
        start_position: point(change.start),
        old_end_position: point(change.old_end),
        new_end_position: point(change.new_end),
    }
}

fn point((row, column): (usize, usize)) -> Point {
    Point { row, column }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> (Buffer, Syntax) {
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::new(Language::Rust);
        syntax.sync(&mut buffer);
        (buffer, syntax)
    }

    fn sexp(syntax: &Syntax) -> String {
        syntax.tree().unwrap().root_node().to_sexp()
    }

    fn fresh(buffer: &Buffer) -> String {
        sexp(&parsed(&buffer.contents()).1)
    }

    #[test]
    fn languages_follow_file_extensions() {
        assert_eq!(
            Language::for_path(Path::new("src/main.rs")),
            Some(Language::Rust)
        );
        assert_eq!(
            Language::for_path(Path::new("LIB.RS")),
            Some(Language::Rust)
        );
        assert_eq!(
            Language::for_path(Path::new("Cargo.toml")),
            Some(Language::Toml)
        );
        assert_eq!(
            Language::for_path(Path::new("Cargo.lock")),
            Some(Language::Toml)
        );
        assert_eq!(
            Language::for_path(Path::new("README.md")),
            Some(Language::Markdown)
        );
        assert_eq!(
            Language::for_path(Path::new("analysis.R")),
            Some(Language::R)
        );
        assert_eq!(
            Language::for_path(Path::new(".Rprofile")),
            Some(Language::R)
        );
        assert_eq!(Language::for_path(Path::new("notes.txt")), None);
        assert_eq!(Language::for_path(Path::new("Makefile")), None);
    }

    #[test]
    fn rust_parses_into_a_syntax_tree() {
        let (_, syntax) = parsed("fn main() {}\n");

        assert_eq!(
            sexp(&syntax),
            "(source_file (function_item name: (identifier) parameters: (parameters) body: (block)))"
        );
    }

    #[test]
    fn edits_keep_the_tree_in_step_with_the_text() {
        let (mut buffer, mut syntax) = parsed("fn a() {}\n");

        buffer.set_point(buffer.len_chars());
        buffer.insert("fn é() { let x = \"ü\"; }\n");
        buffer.delete_range(3..4);
        buffer.set_point(3);
        buffer.insert("bb");
        syntax.sync(&mut buffer);

        assert_eq!(sexp(&syntax), fresh(&buffer));
        assert!(sexp(&syntax).contains("(function_item name: (identifier)"));
    }

    #[test]
    fn replaced_text_is_parsed_again() {
        let (mut buffer, mut syntax) = parsed("fn a() {}\n");

        buffer.set_text("struct S;\n");
        buffer.insert("// c\n");
        syntax.sync(&mut buffer);

        assert_eq!(sexp(&syntax), fresh(&buffer));
    }

    #[test]
    fn comments_are_found_where_they_start_on_a_line() {
        let (buffer, syntax) = parsed("x(); // note\nlet s = \"// no\";\n/* a\nb */\n");

        assert_eq!(syntax.comment_start(&buffer, 0), Some(5));
        assert_eq!(syntax.comment_start(&buffer, 1), None);
        assert_eq!(syntax.comment_start(&buffer, 3), Some(0));
    }

    #[test]
    fn syncing_unchanged_text_keeps_the_tree() {
        let (mut buffer, mut syntax) = parsed("fn a() {}\n");
        let root = syntax.tree().unwrap().root_node().id();

        syntax.sync(&mut buffer);

        assert_eq!(syntax.tree().unwrap().root_node().id(), root);
    }
}
