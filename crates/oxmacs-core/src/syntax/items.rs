use tree_sitter::Node;

use crate::syntax::{Language, Syntax, ancestors, is_function_assignment, preorder};
use crate::text::Buffer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub context: Option<String>,
    pub position: usize,
}

const CONTAINERS: &[&str] = &["impl_item", "trait_item", "mod_item"];

impl Syntax {
    pub fn items(&self, buffer: &Buffer) -> Vec<Item> {
        let Some(tree) = self.synced(buffer) else {
            return Vec::new();
        };
        let sections = self.language == Language::R;
        let mut items = Vec::new();
        preorder(tree, |node| {
            let label = if sections && node.kind() == "comment" {
                section_title(&buffer.text_in_bytes(node.byte_range()))
            } else {
                item_label(node, buffer)
            };
            let Some(label) = label else {
                return;
            };
            let context = node
                .parent()
                .and_then(|parent| {
                    ancestors(parent).find(|ancestor| {
                        CONTAINERS.contains(&ancestor.kind()) || is_function_assignment(*ancestor)
                    })
                })
                .and_then(|container| item_label(container, buffer));
            items.push(Item {
                label,
                context,
                position: buffer.byte_to_char(node.start_byte()),
            });
        });
        items
    }
}

fn section_title(comment: &str) -> Option<String> {
    let text = comment.trim_end();
    let marker = text
        .chars()
        .last()
        .filter(|c| matches!(c, '-' | '=' | '#'))?;
    let run = text.chars().rev().take_while(|c| *c == marker).count();
    let title = text[..text.len() - run].trim_end();
    (run >= 4 && title.chars().any(char::is_alphanumeric)).then(|| title.to_owned())
}

fn item_label(node: Node<'_>, buffer: &Buffer) -> Option<String> {
    let field = |name: &str| {
        node.child_by_field_name(name)
            .map(|child| buffer.text_in_bytes(child.byte_range()))
    };
    if is_function_assignment(node) {
        return field("lhs")
            .map(|name| format!("function {}", name.trim_matches(['"', '\'', '`'])));
    }
    let keyword = match node.kind() {
        "function_item" | "function_signature_item" => "fn",
        "struct_item" => "struct",
        "enum_item" => "enum",
        "union_item" => "union",
        "trait_item" => "trait",
        "mod_item" => "mod",
        "const_item" => "const",
        "static_item" => "static",
        "type_item" => "type",
        "macro_definition" => return field("name").map(|name| format!("macro {name}!")),
        "atx_heading" | "setext_heading" => {
            let line = buffer.line_of(buffer.byte_to_char(node.start_byte()));
            return Some(buffer.line_text(line).trim().to_owned());
        }
        "table" | "table_array_element" => {
            let mut cursor = node.walk();
            let key = node
                .named_children(&mut cursor)
                .find(|child| matches!(child.kind(), "bare_key" | "dotted_key" | "quoted_key"))?;
            let key = buffer.text_in_bytes(key.byte_range());
            return Some(if node.kind() == "table" {
                format!("[{key}]")
            } else {
                format!("[[{key}]]")
            });
        }
        "impl_item" => {
            let target = field("type")?;
            return Some(match field("trait") {
                Some(name) => format!("impl {name} for {target}"),
                None => format!("impl {target}"),
            });
        }
        _ => return None,
    };
    field("name").map(|name| format!("{keyword} {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_headings_are_items() {
        let text = "# Guide\n\nIntro.\n\n## Install\n\nSteps\n=====\n";
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::new(Language::Markdown);
        syntax.sync(&mut buffer);

        let labels: Vec<String> = syntax
            .items(&buffer)
            .into_iter()
            .map(|item| item.label)
            .collect();

        assert_eq!(labels, ["# Guide", "## Install", "Steps"]);
    }

    #[test]
    fn toml_tables_are_items() {
        let text = "[package]\nname = \"oxmacs\"\n\n[[bin]]\nname = \"oxmacs\"\n";
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::new(Language::Toml);
        syntax.sync(&mut buffer);

        let labels: Vec<String> = syntax
            .items(&buffer)
            .into_iter()
            .map(|item| item.label)
            .collect();

        assert_eq!(labels, ["[package]", "[[bin]]"]);
    }

    #[test]
    fn r_functions_and_rstudio_sections_are_items() {
        let text = "# Load data ----\nread <- function(path) readRDS(path)\n\n## Model ====\nfit <- function(data) {\n  inner <- function() 1\n}\nx <- 1\n# just a comment\n";
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::new(Language::R);
        syntax.sync(&mut buffer);

        let items: Vec<(String, Option<String>)> = syntax
            .items(&buffer)
            .into_iter()
            .map(|item| (item.label, item.context))
            .collect();

        assert_eq!(
            items,
            [
                ("# Load data".to_owned(), None),
                ("function read".to_owned(), None),
                ("## Model".to_owned(), None),
                ("function fit".to_owned(), None),
                ("function inner".to_owned(), Some("function fit".to_owned())),
            ]
        );
    }

    #[test]
    fn items_are_listed_in_order_with_their_context() {
        let text = "struct Point;\nimpl Point {\n    fn new() {}\n}\nimpl Display for Point {}\nmod tests {\n    fn works() {}\n}\nmacro_rules! m { () => {} }\n";
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::new(Language::Rust);
        syntax.sync(&mut buffer);

        let items: Vec<(String, Option<String>, usize)> = syntax
            .items(&buffer)
            .into_iter()
            .map(|item| (item.label, item.context, item.position))
            .collect();

        let impl_point = Some("impl Point".to_owned());
        let tests = Some("mod tests".to_owned());
        assert_eq!(
            items,
            [
                ("struct Point".to_owned(), None, 0),
                ("impl Point".to_owned(), None, 14),
                ("fn new".to_owned(), impl_point, 31),
                ("impl Display for Point".to_owned(), None, 45),
                ("mod tests".to_owned(), None, 71),
                ("fn works".to_owned(), tests, 87),
                ("macro m!".to_owned(), None, 103),
            ]
        );
    }
}
