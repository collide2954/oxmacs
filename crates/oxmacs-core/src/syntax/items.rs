use crate::syntax::{Syntax, ancestors, preorder};
use crate::text::Buffer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub context: Option<String>,
    pub position: usize,
}

impl Syntax {
    pub fn items(&self, buffer: &Buffer) -> Vec<Item> {
        let Some(tree) = self.synced(buffer) else {
            return Vec::new();
        };
        let outline = self.language.outline();
        let mut items = Vec::new();
        preorder(tree, |node| {
            let label = if outline.sections && node.kind() == "comment" {
                section_title(&buffer.text_in_bytes(node.byte_range()))
            } else {
                (outline.item)(node, buffer)
            };
            let Some(label) = label else {
                return;
            };
            let context = node
                .parent()
                .and_then(|parent| ancestors(parent).find(|ancestor| (outline.context)(*ancestor)))
                .and_then(|container| (outline.item)(container, buffer));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::Language;

    #[test]
    fn toml_tables_are_items() {
        let text = "[package]\nname = \"oxmacs\"\n\n[[bin]]\nname = \"oxmacs\"\n";
        let mut buffer = Buffer::with_text("t", text);
        let mut syntax = Syntax::new(Language::TOML);
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
        let mut syntax = Syntax::new(Language::RUST);
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
