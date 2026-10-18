use crate::editor::Editor;

const LIST_INDENT: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ListItem {
    indent: usize,
    marker: String,
    checkbox: bool,
}

impl ListItem {
    fn parse(line: &str) -> Option<Self> {
        let indent = line.chars().take_while(|c| *c == ' ').count();
        let rest = &line[indent..];
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        let marker_width = match rest[digits..].chars().next()? {
            '-' | '*' | '+' if digits == 0 => 1,
            '.' | ')' if digits > 0 => digits + 1,
            _ => return None,
        };
        let after = &rest[marker_width..];
        if !(after.is_empty() || after.starts_with(' ')) {
            return None;
        }
        let checkbox = ["[ ] ", "[x] ", "[X] "]
            .iter()
            .any(|box_| after.trim_start().starts_with(box_));
        Some(Self {
            indent,
            marker: rest[..marker_width].to_owned(),
            checkbox,
        })
    }

    fn next(&self) -> String {
        let digits = self.marker.trim_end_matches(['.', ')']);
        let marker = match digits.parse::<u64>() {
            Ok(number) => format!("{}{}", number + 1, &self.marker[digits.len()..]),
            Err(_) => self.marker.clone(),
        };
        let checkbox = if self.checkbox { "[ ] " } else { "" };
        format!("{}{marker} {checkbox}", " ".repeat(self.indent))
    }
}

impl Editor {
    pub(super) fn markdown_insert_list_item(&mut self) {
        let buffer = self.buffers.current_mut();
        let line = buffer.line_of(buffer.point());
        let text = buffer.line_text(line).into_owned();
        let prefix = match ListItem::parse(&text) {
            Some(item) => format!("\n{}", item.next()),
            None if text.trim().is_empty() => {
                buffer.set_point(buffer.line_start(line));
                "- ".to_owned()
            }
            None => "\n- ".to_owned(),
        };
        buffer.insert(&prefix);
    }

    pub(super) fn markdown_shift(&mut self, deeper: bool) {
        let buffer = self.buffers.current_mut();
        let line = buffer.line_of(buffer.point());
        let start = buffer.line_start(line);
        let text = buffer.line_text(line).into_owned();
        let hashes = text.chars().take_while(|c| *c == '#').count();
        let heading = hashes > 0 && text[hashes..].starts_with(' ');
        if heading {
            match (deeper, hashes) {
                (true, 6) => self.show_message("Cannot demote further"),
                (false, 1) => self.show_message("Cannot promote further"),
                (true, _) => buffer.insert_at(start, "#"),
                (false, _) => {
                    buffer.delete_range(start..start + 1);
                }
            }
            return;
        }
        let Some(item) = ListItem::parse(&text) else {
            self.show_message(if deeper {
                "Nothing to demote"
            } else {
                "Nothing to promote"
            });
            return;
        };
        if deeper {
            buffer.insert_at(start, &" ".repeat(LIST_INDENT));
        } else {
            buffer.delete_range(start..start + item.indent.min(LIST_INDENT));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, press};
    use crate::syntax::Language;

    fn markdown(text: &str, point: usize) -> Editor {
        let mut editor = editor_with(text, point);
        editor.set_major_mode(Some(Language::Markdown));
        editor
    }

    fn after(text: &str, point: usize, keys: &str) -> (String, usize) {
        let mut editor = markdown(text, point);
        press(&mut editor, keys);
        (editor.buffer().contents(), editor.buffer().point())
    }

    #[test]
    fn meta_return_continues_lists() {
        assert_eq!(after("- one", 5, "M-RET"), ("- one\n- ".to_owned(), 8));
        assert_eq!(after("9. one", 6, "M-RET"), ("9. one\n10. ".to_owned(), 11));
        assert_eq!(
            after("  * [x] done", 12, "M-RET"),
            ("  * [x] done\n  * [ ] ".to_owned(), 21)
        );
    }

    #[test]
    fn meta_return_splits_an_item_at_point() {
        assert_eq!(after("- ab", 3, "M-RET"), ("- a\n- b".to_owned(), 6));
    }

    #[test]
    fn meta_return_starts_a_list_outside_one() {
        assert_eq!(after("text", 4, "M-RET"), ("text\n- ".to_owned(), 7));
        assert_eq!(after("a\n\n", 2, "M-RET"), ("a\n- \n".to_owned(), 4));
    }

    #[test]
    fn headings_are_promoted_and_demoted() {
        assert_eq!(after("## Title", 3, "M-<left>").0, "# Title");
        assert_eq!(after("## Title", 3, "M-<right> C-c C-=").0, "#### Title");
        assert_eq!(after("## Title", 3, "C-c C--").0, "# Title");

        let mut editor = markdown("# Title", 0);
        press(&mut editor, "M-<left>");
        assert_eq!(editor.buffer().contents(), "# Title");
        assert_eq!(editor.message(), Some("Cannot promote further"));
    }

    #[test]
    fn list_items_are_indented_and_outdented() {
        assert_eq!(after("- a", 2, "M-<right>"), ("    - a".to_owned(), 6));
        assert_eq!(after("    - a", 6, "M-<left>"), ("- a".to_owned(), 2));
    }

    #[test]
    fn plain_lines_have_nothing_to_shift() {
        let mut editor = markdown("text", 0);

        press(&mut editor, "M-<right>");

        assert_eq!(editor.message(), Some("Nothing to demote"));
    }
}
