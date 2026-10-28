use std::ops::Range;

use tree_sitter::{Node, Tree};

use crate::syntax::motion::{COMMENTS, chars, leaf_at};
use crate::syntax::{Syntax, ancestors};
use crate::text::Buffer;

fn code_from(tree: &Tree, buffer: &Buffer, first: usize) -> Option<usize> {
    (first..buffer.len_lines()).find_map(|line| {
        let start = buffer.line_start(line);
        let indent = buffer
            .chars_after(start)
            .take_while(|c| matches!(c, ' ' | '\t'))
            .count();
        let anchor = start + indent;
        if anchor >= buffer.line_end(line) {
            return None;
        }
        let leaf = leaf_at(tree, buffer, anchor)?;
        let comment = ancestors(leaf).any(|node| COMMENTS.contains(&node.kind()));
        (!comment).then_some(anchor)
    })
}

fn is_statement(node: Node<'_>, containers: &[&str]) -> bool {
    node.is_named()
        && !COMMENTS.contains(&node.kind())
        && node
            .parent()
            .is_some_and(|parent| containers.contains(&parent.kind()))
}

impl Syntax {
    pub fn statement_at(&self, buffer: &Buffer, point: usize) -> Option<Range<usize>> {
        let tree = self.synced(buffer)?;
        let containers = self.language.outline().statements;
        let anchor = code_from(tree, buffer, buffer.line_of(point))?;
        let statement = ancestors(leaf_at(tree, buffer, anchor)?)
            .find(|node| is_statement(*node, containers))?;
        let mut range = chars(buffer, statement.byte_range());
        let last = buffer.line_of(range.end);
        let mut next = statement.next_named_sibling();
        while let Some(sibling) = next.filter(|sibling| is_statement(*sibling, containers)) {
            let span = chars(buffer, sibling.byte_range());
            if buffer.line_of(span.start) != last {
                break;
            }
            range.end = span.end;
            next = sibling.next_named_sibling();
        }
        Some(range)
    }

    pub fn statement_after(&self, buffer: &Buffer, position: usize) -> Option<usize> {
        let tree = self.synced(buffer)?;
        code_from(tree, buffer, buffer.line_of(position) + 1)
    }
}

#[cfg(test)]
mod tests {
    use crate::syntax::{Language, Syntax};
    use crate::text::Buffer;

    const SCRIPT: &str = "library(dplyr)\n\n# tidy the data\ntidy <- df |>\n  filter(x > 1) |>\n  mutate(y = 2)\n\nf <- function(x) {\n  y <- x + 1\n  y * 2\n}\na <- 1; b <- 2\n\n";

    fn parsed() -> (Buffer, Syntax) {
        let mut buffer = Buffer::with_text("t", SCRIPT);
        let mut syntax = Syntax::new(Language::r());
        syntax.sync(&mut buffer);
        (buffer, syntax)
    }

    fn statement(line: usize) -> Option<String> {
        let (buffer, syntax) = parsed();
        let range = syntax.statement_at(&buffer, buffer.line_start(line) + 1)?;
        Some(buffer.text_in(range))
    }

    #[test]
    fn a_line_is_a_statement_of_its_own() {
        assert_eq!(statement(0).as_deref(), Some("library(dplyr)"));
    }

    #[test]
    fn statements_span_every_line_they_cover() {
        let pipe = "tidy <- df |>\n  filter(x > 1) |>\n  mutate(y = 2)";

        assert_eq!(statement(3).as_deref(), Some(pipe));
        assert_eq!(statement(4).as_deref(), Some(pipe));
        assert_eq!(statement(5).as_deref(), Some(pipe));
    }

    #[test]
    fn blank_and_comment_lines_lead_to_the_next_statement() {
        let pipe = "tidy <- df |>\n  filter(x > 1) |>\n  mutate(y = 2)";

        assert_eq!(statement(1).as_deref(), Some(pipe));
        assert_eq!(statement(2).as_deref(), Some(pipe));
        assert_eq!(statement(12), None);
    }

    #[test]
    fn lines_inside_a_body_are_their_own_statements() {
        let function = "f <- function(x) {\n  y <- x + 1\n  y * 2\n}";

        assert_eq!(statement(7).as_deref(), Some(function));
        assert_eq!(statement(8).as_deref(), Some("y <- x + 1"));
        assert_eq!(statement(10).as_deref(), Some(function));
    }

    #[test]
    fn statements_sharing_a_line_go_together() {
        assert_eq!(statement(11).as_deref(), Some("a <- 1; b <- 2"));
    }

    #[test]
    fn the_next_statement_starts_on_a_later_line() {
        let (buffer, syntax) = parsed();
        let after = |line: usize| {
            let end = syntax.statement_at(&buffer, buffer.line_start(line))?.end;
            syntax.statement_after(&buffer, end)
        };

        assert_eq!(after(3), Some(buffer.line_start(7)));
        assert_eq!(after(8), Some(buffer.line_start(9) + 2));
        assert_eq!(after(11), None);
    }
}
