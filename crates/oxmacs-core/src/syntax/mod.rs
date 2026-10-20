mod builtin;
mod highlight;
mod indent;
mod items;
mod language;
mod layers;
pub(crate) mod motion;
mod statements;

use std::fmt;

use tree_sitter::{InputEdit, Node, Parser, Point, Tree};

pub use items::Item;
pub use language::{Feature, Grammar, Indentation, Language, LanguageSpec, Outline, language_for};

use layers::Layer;

use crate::text::{Buffer, TextChange};
pub struct Syntax {
    language: Language,
    known: Vec<Language>,
    parser: Option<Parser>,
    tree: Option<Tree>,
    revision: Option<u64>,
    layers: Vec<Layer>,
    injectors: Vec<(&'static Grammar, Parser)>,
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
        Self::knowing(language, &Language::BUILT_IN)
    }

    pub fn knowing(language: Language, known: &[Language]) -> Self {
        let parser = language.grammar().and_then(|grammar| {
            let mut parser = Parser::new();
            parser.set_language(&grammar.language()).ok()?;
            Some(parser)
        });
        Self {
            language,
            known: known.to_vec(),
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

    pub fn parses(&self) -> bool {
        self.parser.is_some()
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
        if self.parser.is_none() {
            return;
        }
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
        self.tree = self.parser.as_mut().and_then(|parser| {
            parser.parse_with_options(
                &mut |byte, _| text.chunk_at_byte(byte).as_bytes(),
                old.as_ref(),
                None,
            )
        });
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
    use std::path::Path;

    use super::*;

    fn parsed(text: &str) -> (Buffer, Syntax) {
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::new(Language::RUST);
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
            language_for(&Language::BUILT_IN, Path::new("src/main.rs")),
            Some(Language::RUST)
        );
        assert_eq!(
            language_for(&Language::BUILT_IN, Path::new("LIB.RS")),
            Some(Language::RUST)
        );
        assert_eq!(
            language_for(&Language::BUILT_IN, Path::new("Cargo.toml")),
            Some(Language::TOML)
        );
        assert_eq!(
            language_for(&Language::BUILT_IN, Path::new("Cargo.lock")),
            Some(Language::TOML)
        );
        assert_eq!(
            language_for(&Language::BUILT_IN, Path::new("README.md")),
            None
        );
        assert_eq!(
            language_for(&Language::BUILT_IN, Path::new("analysis.R")),
            Some(Language::R)
        );
        assert_eq!(
            language_for(&Language::BUILT_IN, Path::new("plot.r")),
            Some(Language::R)
        );
        assert_eq!(
            language_for(&Language::BUILT_IN, Path::new(".Rprofile")),
            Some(Language::R)
        );
        assert_eq!(
            language_for(&Language::BUILT_IN, Path::new("notes.txt")),
            None
        );
        assert_eq!(
            language_for(&Language::BUILT_IN, Path::new("Makefile")),
            None
        );
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
