use tree_sitter::Node;

use crate::display;
use crate::syntax::{Indentation, Syntax, ancestors};
use crate::text::Buffer;

const OPENERS: &[&str] = &["{", "(", "[", "[["];
const CLOSERS: &[&str] = &["}", ")", "]", "]]"];
const LITERALS: &[&str] = &[
    "string_literal",
    "raw_string_literal",
    "block_comment",
    "string",
];

impl Syntax {
    pub fn indentation(&self, buffer: &Buffer, line: usize, tab_width: usize) -> Option<usize> {
        let tree = self.synced(buffer)?;
        let text = buffer.line_text(line);
        let leading = text.chars().take_while(|c| matches!(c, ' ' | '\t')).count();
        let rest: String = text.chars().skip(leading).collect();
        let byte = buffer.char_to_byte(buffer.line_start(line) + leading);
        let node = tree.root_node().descendant_for_byte_range(byte, byte)?;
        if ancestors(node)
            .any(|ancestor| LITERALS.contains(&ancestor.kind()) && row(ancestor) < line)
        {
            return None;
        }
        let container = ancestors(node).find_map(|ancestor| {
            open_bracket(ancestor, byte)
                .filter(|open| row(*open) < line)
                .map(|open| (ancestor, open))
        });
        let rules = self.language.indentation();
        let unit = rules.unit;
        let Some((container, open)) = container else {
            return Some(if rules.continues {
                continuation(rules, statement(node, tree.root_node()), line, &rest) * unit
            } else {
                previous_indentation(buffer, line, tab_width)
            });
        };
        let base = if rules.from_owner {
            indentation_of(buffer, row(owner(container, open)), tab_width)
        } else {
            indentation_of(buffer, row(open), tab_width)
        };
        let closes = !node.is_named()
            && CLOSERS.contains(&node.kind())
            && node.start_byte() == byte
            && node.parent() == Some(container);
        if closes {
            return Some(base);
        }
        let continues = if rules.continues {
            continuation(rules, statement(node, container), line, &rest)
        } else {
            0
        };
        let hanging = (rules.hanging && open.kind() != "{")
            .then(|| hanging_column(buffer, open, tab_width))
            .flatten();
        Some(hanging.unwrap_or(base + unit) + continues * unit)
    }
}
fn owner<'tree>(container: Node<'tree>, open: Node<'tree>) -> Node<'tree> {
    if container.start_byte() < open.start_byte() {
        return container;
    }
    container
        .parent()
        .filter(|parent| !matches!(parent.kind(), "program" | "braced_expression"))
        .unwrap_or(container)
}

fn hanging_column(buffer: &Buffer, open: Node<'_>, tab_width: usize) -> Option<usize> {
    let after = buffer.byte_to_char(open.end_byte());
    let line = buffer.line_of(after);
    let start = buffer.line_start(line);
    let text = buffer.line_text(line);
    let rest: String = text.chars().skip(after - start).collect();
    let skipped = rest.chars().take_while(|c| matches!(c, ' ' | '\t')).count();
    let first = rest.chars().nth(skipped)?;
    (first != '#').then(|| display::display_column(&text, after - start + skipped, tab_width))
}

fn row(node: Node<'_>) -> usize {
    node.start_position().row
}

pub(super) fn open_bracket(node: Node<'_>, byte: usize) -> Option<Node<'_>> {
    let mut open = Vec::new();
    scan_brackets(node, byte, &mut open);
    open.pop()
}

fn scan_brackets<'tree>(node: Node<'tree>, byte: usize, open: &mut Vec<Node<'tree>>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.start_byte() >= byte {
            break;
        }
        let token = !child.is_named();
        if child.is_error() {
            scan_brackets(child, byte, open);
        } else if token && OPENERS.contains(&child.kind()) {
            open.push(child);
        } else if token && CLOSERS.contains(&child.kind()) {
            open.pop();
        }
    }
}

fn statement<'tree>(node: Node<'tree>, container: Node<'tree>) -> Option<Node<'tree>> {
    ancestors(node).find(|ancestor| ancestor.parent() == Some(container))
}

fn continuation(rules: Indentation, statement: Option<Node<'_>>, line: usize, rest: &str) -> usize {
    let starts_fresh = rest.starts_with('{')
        || rules
            .fresh_words
            .iter()
            .any(|word| starts_with_word(rest, word));
    usize::from(statement.is_some_and(|statement| row(statement) < line) && !starts_fresh)
}

fn starts_with_word(text: &str, word: &str) -> bool {
    text.strip_prefix(word)
        .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
}

fn previous_indentation(buffer: &Buffer, line: usize, tab_width: usize) -> usize {
    (0..line)
        .rev()
        .find(|earlier| !buffer.line_text(*earlier).trim().is_empty())
        .map_or(0, |earlier| indentation_of(buffer, earlier, tab_width))
}

fn indentation_of(buffer: &Buffer, line: usize, tab_width: usize) -> usize {
    let text = buffer.line_text(line);
    let leading = text.chars().take_while(|c| matches!(c, ' ' | '\t')).count();
    display::display_column(&text, leading, tab_width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::Language;

    const FORMATTED: &str = r#"use std::collections::{
    HashMap,
    HashSet,
};

/// Docs.
#[derive(Debug)]
pub struct Point {
    x: i32,
    // A comment.
    y: i32,
}

enum Shape {
    Circle { radius: f64 },
    Square(f64),
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    fn long<T>(
        first: T,
        second: T,
    ) -> Vec<T>
    where
        T: Clone,
    {
        let items = vec![
            first.clone(),
            second,
        ];
        let total = items
            .iter()
            .count();
        match total {
            0 => {
                println!("none");
            }
            1 =>
                println!("one"),
            _ => println!(
                "many: {}",
                total
            ),
        }
        if total > 1 {
            items.into_iter().map(|item| {
                item
            }).collect()
        } else {
            Vec::new()
        }
    }
}
"#;

    fn indentations(text: &str) -> (Buffer, Syntax) {
        parsed(Language::RUST, text)
    }

    fn parsed(language: Language, text: &str) -> (Buffer, Syntax) {
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::new(language);
        syntax.sync(&mut buffer);
        (buffer, syntax)
    }

    #[test]
    fn formatted_code_keeps_its_indentation() {
        let (buffer, syntax) = indentations(FORMATTED);

        for (line, text) in FORMATTED.lines().enumerate() {
            if text.trim().is_empty() {
                continue;
            }
            let expected = text.len() - text.trim_start().len();
            assert_eq!(
                syntax.indentation(&buffer, line, 8),
                Some(expected),
                "line {}: {text}",
                line + 1
            );
        }
    }

    #[test]
    fn blank_lines_take_the_level_of_their_block() {
        let (buffer, syntax) = indentations("fn a() {\n    let x = 1;\n\n}\n");

        assert_eq!(syntax.indentation(&buffer, 2, 8), Some(4));
    }

    #[test]
    fn unfinished_blocks_still_indent() {
        let (buffer, syntax) = indentations("fn a() {\n\n");

        assert_eq!(syntax.indentation(&buffer, 1, 8), Some(4));
    }

    #[test]
    fn misindented_lines_get_their_proper_level() {
        let (buffer, syntax) = indentations("fn a() {\nlet x = 1;\n        }\n");

        assert_eq!(syntax.indentation(&buffer, 1, 8), Some(4));
        assert_eq!(syntax.indentation(&buffer, 2, 8), Some(0));
    }

    #[test]
    fn toml_indents_arrays_and_keeps_other_lines_level() {
        let (buffer, syntax) = parsed(Language::TOML, "[a]\n  x = 1\n  y = [\n\"b\",\n]\n");
        let indent = |line| syntax.indentation(&buffer, line, 8);

        assert_eq!(indent(2), Some(2));
        assert_eq!(indent(3), Some(6));
        assert_eq!(indent(4), Some(2));
    }

    const TIDY_R: &str = r#"library(dplyr)

#' Summarize a data frame.
summarize_by <- function(data, group,
                         value = "x") {
  if (nrow(data) == 0) {
    return(NULL)
  } else if (is.null(group)) {
    stop("no group")
  }
  result <- data |>
    group_by({{ group }}) |>
    summarize(
      mean = mean(.data[[value]]),
      n = n()
    )
  for (i in seq_len(3)) {
    message(i)
  }
  result
}

plot <- ggplot(df, aes(x, y)) +
  geom_point()

if (interactive())
  print(plot)

values <- c(1, 2,
            3)
lookup <- list(
  a = 1,
  b = list(
    c = 2
  )
)
x[[
  "key"
]] <- 1
"#;

    #[test]
    fn tidyverse_r_keeps_its_indentation() {
        let (buffer, syntax) = parsed(Language::R, TIDY_R);

        for (line, text) in TIDY_R.lines().enumerate() {
            if text.trim().is_empty() {
                continue;
            }
            let expected = text.len() - text.trim_start().len();
            assert_eq!(
                syntax.indentation(&buffer, line, 8),
                Some(expected),
                "line {}: {text}",
                line + 1
            );
        }
    }

    #[test]
    fn r_indents_new_lines_two_spaces_into_blocks_and_pipes() {
        let (buffer, syntax) = parsed(Language::R, "f <- function(x) {\nx |>\nsum()\n}\n");
        let indent = |line| syntax.indentation(&buffer, line, 8);

        assert_eq!(indent(1), Some(2));
        assert_eq!(indent(2), Some(4));
        assert_eq!(indent(3), Some(0));
    }

    #[test]
    fn r_multiline_strings_are_left_alone() {
        let (buffer, syntax) = parsed(Language::R, "x <- \"a\n   b\"\n");

        assert_eq!(syntax.indentation(&buffer, 1, 8), None);
    }

    #[test]
    fn lines_inside_strings_and_comments_are_left_alone() {
        let (buffer, syntax) = indentations("let s = \"a\n   b\";\n/* x\n  y */\n");

        assert_eq!(syntax.indentation(&buffer, 1, 8), None);
        assert_eq!(syntax.indentation(&buffer, 3, 8), None);
    }
}
