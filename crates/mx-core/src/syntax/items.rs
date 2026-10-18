use tree_sitter::Node;

use crate::syntax::{Syntax, ancestors, preorder};
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
        let mut items = Vec::new();
        preorder(tree, |node| {
            let Some(label) = item_label(node, buffer) else {
                return;
            };
            let context = node
                .parent()
                .and_then(|parent| {
                    ancestors(parent).find(|ancestor| CONTAINERS.contains(&ancestor.kind()))
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

fn item_label(node: Node<'_>, buffer: &Buffer) -> Option<String> {
    let field = |name: &str| {
        node.child_by_field_name(name)
            .map(|child| buffer.text_in_bytes(child.byte_range()))
    };
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
    use crate::syntax::Language;

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
