use std::ops::Range;

use tree_sitter::{Node, Tree};

use crate::syntax::indent::open_bracket;
use crate::syntax::{Syntax, ancestors, preorder};
use crate::text::Buffer;

pub const PREMATURE: &str = "Containing expression ends prematurely";

const DEFUNS: &[&str] = &[
    "function_item",
    "function_signature_item",
    "impl_item",
    "struct_item",
    "enum_item",
    "union_item",
    "trait_item",
    "mod_item",
    "macro_definition",
    "const_item",
    "static_item",
    "type_item",
];
const OPENERS: &[&str] = &["(", "[", "{"];
const CLOSERS: &[&str] = &[")", "]", "}"];
const COMMENTS: &[&str] = &["line_comment", "block_comment"];

impl Syntax {
    fn defuns(&self, buffer: &Buffer) -> Vec<Range<usize>> {
        let Some(tree) = self.synced(buffer) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        preorder(tree, |node| {
            if DEFUNS.contains(&node.kind()) {
                found.push(chars(buffer, node.byte_range()));
            }
        });
        found
    }

    pub fn defun_start_before(&self, buffer: &Buffer, point: usize) -> Option<usize> {
        self.defuns(buffer)
            .into_iter()
            .map(|defun| defun.start)
            .filter(|start| *start < point)
            .max()
    }

    pub fn defun_end_after(&self, buffer: &Buffer, point: usize) -> Option<usize> {
        let defuns = self.defuns(buffer);
        let inside = defuns
            .iter()
            .filter(|defun| defun.start < point && point < defun.end)
            .min_by_key(|defun| defun.len());
        inside.map(|defun| defun.end).or_else(|| {
            defuns
                .iter()
                .filter(|defun| defun.start >= point)
                .map(|defun| defun.end)
                .min()
        })
    }

    pub fn sexp_end(&self, buffer: &Buffer, point: usize) -> Result<usize, &'static str> {
        let tree = self.synced(buffer).ok_or(PREMATURE)?;
        let mut from = point;
        loop {
            from += buffer
                .chars_after(from)
                .take_while(|c| c.is_whitespace())
                .count();
            if from >= buffer.len_chars() {
                return Err(PREMATURE);
            }
            let leaf = leaf_at(tree, buffer, from).ok_or(PREMATURE)?;
            let node = ancestors(leaf)
                .find(|node| COMMENTS.contains(&node.kind()))
                .unwrap_or(leaf);
            if COMMENTS.contains(&node.kind()) {
                from = chars(buffer, node.byte_range()).end;
                continue;
            }
            if CLOSERS.contains(&node.kind()) && !node.is_named() {
                return Err(PREMATURE);
            }
            let whole = node
                .parent()
                .filter(|parent| !node.is_named() && parent.start_byte() == node.start_byte())
                .unwrap_or(node);
            return Ok(chars(buffer, whole.byte_range()).end);
        }
    }

    pub fn sexp_start(&self, buffer: &Buffer, point: usize) -> Result<usize, &'static str> {
        let tree = self.synced(buffer).ok_or(PREMATURE)?;
        let mut from = point;
        loop {
            from -= buffer
                .chars_before(from)
                .take_while(|c| c.is_whitespace())
                .count();
            if from == 0 {
                return Err(PREMATURE);
            }
            let leaf = leaf_at(tree, buffer, from - 1).ok_or(PREMATURE)?;
            let node = ancestors(leaf)
                .find(|node| COMMENTS.contains(&node.kind()))
                .unwrap_or(leaf);
            if COMMENTS.contains(&node.kind()) {
                from = chars(buffer, node.byte_range()).start;
                continue;
            }
            if OPENERS.contains(&node.kind()) && !node.is_named() {
                return Err(PREMATURE);
            }
            let whole = node
                .parent()
                .filter(|parent| !node.is_named() && parent.end_byte() == node.end_byte())
                .unwrap_or(node);
            return Ok(chars(buffer, whole.byte_range()).start);
        }
    }

    pub fn enclosing_opener(&self, buffer: &Buffer, point: usize) -> Option<usize> {
        let tree = self.synced(buffer)?;
        let byte = buffer.char_to_byte(point);
        let leaf = tree.root_node().descendant_for_byte_range(byte, byte)?;
        ancestors(leaf)
            .find_map(|ancestor| open_bracket(ancestor, byte))
            .map(|open| buffer.byte_to_char(open.start_byte()))
    }

    pub fn next_opener(&self, buffer: &Buffer, point: usize) -> Option<usize> {
        let tree = self.synced(buffer)?;
        for (position, c) in (point..).zip(buffer.chars_after(point)) {
            if matches!(c, '(' | '[' | '{' | ')' | ']' | '}')
                && let Some(leaf) = leaf_at(tree, buffer, position)
                && !leaf.is_named()
            {
                return OPENERS.contains(&leaf.kind()).then_some(position);
            }
        }
        None
    }
}

fn leaf_at<'tree>(tree: &'tree Tree, buffer: &Buffer, position: usize) -> Option<Node<'tree>> {
    let start = buffer.char_to_byte(position);
    let end = buffer.char_to_byte(position + 1);
    tree.root_node().descendant_for_byte_range(start, end)
}

fn chars(buffer: &Buffer, bytes: Range<usize>) -> Range<usize> {
    buffer.byte_to_char(bytes.start)..buffer.byte_to_char(bytes.end)
}
