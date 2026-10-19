use crate::editor::prompts::minibuffer::Purpose;
use crate::editor::{Choice, Completion, Editor};

impl Editor {
    pub(in crate::editor) fn imenu(&mut self) {
        self.sync_syntax();
        let items = self
            .buffers
            .syntax(0)
            .map(|syntax| syntax.items(self.buffers.current()))
            .unwrap_or_default();
        if items.is_empty() {
            self.show_message("No items in this buffer");
            return;
        }
        if self.open_minibuffer("Index item: ", Purpose::Item)
            && let Some(minibuffer) = &mut self.minibuffer
        {
            minibuffer.items = items;
        }
    }

    pub(in crate::editor) fn item_completions(
        &self,
        matches: impl Fn(&str) -> bool,
    ) -> Vec<Completion> {
        let Some(minibuffer) = &self.minibuffer else {
            return Vec::new();
        };
        let buffer = self.buffers.current();
        minibuffer
            .items
            .iter()
            .filter(|item| matches(&item.label))
            .map(|item| {
                let line = buffer.line_of(item.position) + 1;
                Completion {
                    label: item.label.clone(),
                    annotation: Some(match &item.context {
                        Some(context) => format!("{context}  L{line}"),
                        None => format!("L{line}"),
                    }),
                    choice: Choice::Item(item.position),
                }
            })
            .collect()
    }

    pub(in crate::editor) fn jump_to_item(&mut self, position: usize) {
        let buffer = self.buffers.current_mut();
        buffer.set_mark(buffer.point());
        buffer.deactivate_mark();
        buffer.set_point(position);
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, labels, press, type_text};
    use crate::syntax::Language;

    fn rust(text: &str) -> Editor {
        let mut editor = editor_with(text, 0);
        editor.set_major_mode(Some(Language::Rust));
        editor
    }

    #[test]
    fn m_g_i_lists_items_with_their_context_and_line() {
        let mut editor = rust("struct S;\nimpl S {\n    fn new() {}\n}\n");

        press(&mut editor, "M-g i");

        assert_eq!(editor.minibuffer().unwrap().prompt(), "Index item: ");
        assert_eq!(labels(&editor), ["struct S", "impl S", "fn new"]);
        let annotations: Vec<Option<String>> = editor
            .completions()
            .into_iter()
            .map(|completion| completion.annotation)
            .collect();
        assert_eq!(
            annotations,
            [
                Some("L1".to_owned()),
                Some("L2".to_owned()),
                Some("impl S  L3".to_owned())
            ]
        );
    }

    #[test]
    fn choosing_an_item_jumps_there_and_saves_the_mark() {
        let mut editor = rust("struct S;\nimpl S {\n    fn new() {}\n}\n");

        press(&mut editor, "M-g i");
        type_text(&mut editor, "new");
        press(&mut editor, "RET");

        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.buffer().point(), 23);
        assert_eq!(editor.buffer().mark(), Some(0));
    }

    #[test]
    fn buffers_without_items_say_so() {
        let mut editor = editor_with("plain", 0);

        press(&mut editor, "M-g i");

        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.message(), Some("No items in this buffer"));
    }
}
