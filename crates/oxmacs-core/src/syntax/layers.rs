use std::iter;
use std::ops::Range;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Parser, Query, QueryCursor, QueryMatch, Tree};

use crate::syntax::{Grammar, Syntax};
use crate::text::Buffer;

pub(super) struct Layer {
    pub(super) grammar: &'static Grammar,
    pub(super) tree: Tree,
    pub(super) ranges: Vec<Range<usize>>,
    pub(super) replaces: bool,
}

struct Injection {
    grammar: &'static Grammar,
    range: tree_sitter::Range,
    combined: bool,
}

impl Syntax {
    pub(super) fn inject(&mut self, buffer: &Buffer) {
        self.layers.clear();
        let Some(query) = self
            .language
            .grammar()
            .and_then(|grammar| grammar.injections())
        else {
            return;
        };
        let Some(tree) = &self.tree else {
            return;
        };
        let mut found = Vec::new();
        let mut cursor = QueryCursor::new();
        let text = |node: Node<'_>| iter::once(buffer.text_in_bytes(node.byte_range()));
        let mut matches = cursor.matches(query, tree.root_node(), text);
        while let Some(matched) = matches.next() {
            if let Some(injection) = self.injection(query, matched, buffer) {
                found.push(injection);
            }
        }
        let mut combined: Vec<(&'static Grammar, Vec<tree_sitter::Range>)> = Vec::new();
        let mut separate = Vec::new();
        for injection in found {
            if injection.combined {
                match combined
                    .iter_mut()
                    .find(|(grammar, _)| std::ptr::eq(*grammar, injection.grammar))
                {
                    Some((_, ranges)) => ranges.push(injection.range),
                    None => combined.push((injection.grammar, vec![injection.range])),
                }
            } else {
                separate.push(injection);
            }
        }
        for (grammar, ranges) in combined {
            self.add_layer(grammar, &ranges, false, buffer);
        }
        for injection in separate {
            self.add_layer(injection.grammar, &[injection.range], true, buffer);
        }
    }

    fn injection(
        &self,
        query: &Query,
        matched: &QueryMatch<'_, '_>,
        buffer: &Buffer,
    ) -> Option<Injection> {
        let names = query.capture_names();
        let captured = |wanted: &str| {
            matched
                .captures()
                .iter()
                .find(|capture| names.get(capture.index as usize) == Some(&wanted))
                .map(|capture| capture.node)
        };
        let content = captured("injection.content")?;
        let properties = query.property_settings(matched.pattern_index);
        let property = |key: &str| properties.iter().find(|property| &*property.key == key);
        let name = match captured("injection.language") {
            Some(node) => buffer.text_in_bytes(node.byte_range()),
            None => property("injection.language")?.value.as_deref()?.to_owned(),
        };
        Some(Injection {
            grammar: self.language.injected(name.trim(), &self.known)?,
            range: content.range(),
            combined: property("injection.combined").is_some(),
        })
    }

    fn injector(&mut self, grammar: &'static Grammar) -> Option<&mut Parser> {
        if !self
            .injectors
            .iter()
            .any(|(known, _)| std::ptr::eq(*known, grammar))
        {
            let mut parser = Parser::new();
            parser.set_language(&grammar.language()).ok()?;
            self.injectors.push((grammar, parser));
        }
        self.injectors
            .iter_mut()
            .find(|(known, _)| std::ptr::eq(*known, grammar))
            .map(|(_, parser)| parser)
    }

    fn add_layer(
        &mut self,
        grammar: &'static Grammar,
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
