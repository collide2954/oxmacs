use std::ops::Range;

use tree_sitter::{Node, Parser, Tree};

use crate::syntax::{Language, Syntax, preorder};
use crate::text::Buffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Grammar {
    Rust,
    Toml,
    Markdown,
    MarkdownInline,
}

impl Grammar {
    pub(super) fn language(self) -> tree_sitter::Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
            Self::Markdown => tree_sitter_md::LANGUAGE.into(),
            Self::MarkdownInline => tree_sitter_md::INLINE_LANGUAGE.into(),
        }
    }

    fn for_fence(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "rust" | "rs" => Some(Self::Rust),
            "toml" => Some(Self::Toml),
            _ => None,
        }
    }
}

impl Language {
    pub(super) fn grammar(self) -> Grammar {
        match self {
            Self::Rust => Grammar::Rust,
            Self::Toml => Grammar::Toml,
            Self::Markdown => Grammar::Markdown,
        }
    }
}

pub(super) struct Layer {
    pub(super) grammar: Grammar,
    pub(super) tree: Tree,
    pub(super) ranges: Vec<Range<usize>>,
    pub(super) replaces: bool,
}

impl Syntax {
    pub(super) fn inject(&mut self, buffer: &Buffer) {
        self.layers.clear();
        let Some(tree) = self
            .tree
            .as_ref()
            .filter(|_| self.language == Language::Markdown)
        else {
            return;
        };
        let mut inline = Vec::new();
        let mut code = Vec::new();
        preorder(tree, |node| match node.kind() {
            "inline" => inline.push(node.range()),
            "fenced_code_block" => {
                if let Some(injection) = fenced_code(node, buffer) {
                    code.push(injection);
                }
            }
            _ => {}
        });
        if !inline.is_empty() {
            self.add_layer(Grammar::MarkdownInline, &inline, false, buffer);
        }
        for (grammar, range) in code {
            self.add_layer(grammar, &[range], true, buffer);
        }
    }

    fn injector(&mut self, grammar: Grammar) -> Option<&mut Parser> {
        if !self.injectors.iter().any(|(known, _)| *known == grammar) {
            let mut parser = Parser::new();
            parser.set_language(&grammar.language()).ok()?;
            self.injectors.push((grammar, parser));
        }
        self.injectors
            .iter_mut()
            .find(|(known, _)| *known == grammar)
            .map(|(_, parser)| parser)
    }

    fn add_layer(
        &mut self,
        grammar: Grammar,
        ranges: &[tree_sitter::Range],
        replaces: bool,
        buffer: &Buffer,
    ) {
        let Some(parser) = self.injector(grammar) else {
            return;
        };
        if parser.set_included_ranges(ranges).is_err() {
            return;
        }
        let tree = parser.parse_with_options(
            &mut |byte, _| buffer.chunk_at_byte(byte).as_bytes(),
            None,
            None,
        );
        if let Some(tree) = tree {
            self.layers.push(Layer {
                grammar,
                tree,
                ranges: ranges
                    .iter()
                    .map(|range| range.start_byte..range.end_byte)
                    .collect(),
                replaces,
            });
        }
    }
}

fn fenced_code(node: Node<'_>, buffer: &Buffer) -> Option<(Grammar, tree_sitter::Range)> {
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
    let info = children
        .iter()
        .find(|child| child.kind() == "info_string")?;
    let mut info_cursor = info.walk();
    let language = info
        .named_children(&mut info_cursor)
        .find(|child| child.kind() == "language")?;
    let grammar = Grammar::for_fence(&buffer.text_in_bytes(language.byte_range()))?;
    let content = children
        .iter()
        .find(|child| child.kind() == "code_fence_content")?;
    Some((grammar, content.range()))
}
