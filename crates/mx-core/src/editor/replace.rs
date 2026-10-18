use std::ops::Range;

use crate::editor::isearch::match_marks;
use crate::editor::minibuffer::Purpose;
use crate::editor::{Editor, is_keyboard_quit};
use crate::input::{Key, KeyCode};
use crate::search::{Haystack, Pattern};
use crate::theme::Highlight;

#[derive(Debug)]
pub struct Replace {
    pattern: Pattern,
    regex: bool,
    from: String,
    to: String,
    end_gap: usize,
    current: Range<usize>,
    count: usize,
    lazy: Vec<Range<usize>>,
    revision: u64,
}

impl Replace {
    pub fn prompt(&self) -> String {
        format!(
            "Query replacing {}{} with {}: (y, n, !, ., ^, q) ",
            regexp_word(self.regex),
            self.from,
            self.to
        )
    }
}

impl Editor {
    pub fn replacing(&self) -> Option<&Replace> {
        self.replace.as_ref()
    }

    pub(super) fn query_replace(&mut self, regex: bool) {
        let prompt = format!("Query replace {}", regexp_word(regex));
        self.open_minibuffer(
            &format!("{}: ", prompt.trim_end()),
            Purpose::ReplaceFrom(regex),
        );
    }

    pub(super) fn exit_replace_prompt(&mut self, purpose: Purpose) {
        let input = self.minibuffer_input();
        match purpose {
            Purpose::ReplaceFrom(regex) => {
                self.minibuffer = None;
                if !input.is_empty() {
                    self.ask_replacement(input, regex);
                }
            }
            Purpose::ReplaceTo(regex) => {
                let from = self
                    .minibuffer
                    .take()
                    .map(|minibuffer| minibuffer.from)
                    .unwrap_or_default();
                self.start_replace(from, input, regex);
            }
            _ => {}
        }
    }

    pub(super) fn ask_replacement(&mut self, from: String, regex: bool) {
        let prompt = format!("Query replace {}{from} with: ", regexp_word(regex));
        if self.open_minibuffer(&prompt, Purpose::ReplaceTo(regex))
            && let Some(minibuffer) = &mut self.minibuffer
        {
            minibuffer.from = from;
        }
    }

    fn start_replace(&mut self, from: String, to: String, regex: bool) {
        let Ok(pattern) = Pattern::new(&from, regex) else {
            return;
        };
        self.shift_selection = false;
        let buffer = self.buffers.current_mut();
        let origin = buffer.point();
        let scope = origin..buffer.len_chars();
        buffer.set_mark(origin);
        buffer.deactivate_mark();
        buffer.close_undo_step();
        self.replace = Some(Replace {
            pattern,
            regex,
            from,
            to,
            end_gap: buffer.len_chars() - scope.end,
            current: scope.start..scope.start,
            count: 0,
            lazy: Vec::new(),
            revision: buffer.revision(),
        });
        self.next_replacement(scope.start);
    }

    fn next_replacement(&mut self, from: usize) {
        let Some(replace) = &mut self.replace else {
            return;
        };
        let buffer = self.buffers.current_mut();
        let end = buffer.len_chars().saturating_sub(replace.end_gap);
        let haystack = Haystack::new(buffer);
        let found = replace
            .pattern
            .find_forward(&haystack, from)
            .filter(|found| from <= end && found.end <= end);
        let Some(found) = found else {
            self.finish_replace();
            return;
        };
        replace.lazy = replace.pattern.find_all(&haystack, found.start..end);
        replace.revision = buffer.revision();
        replace.current = found.clone();
        buffer.set_point(found.end);
    }

    pub(super) fn answer_replace(&mut self, key: Key) -> bool {
        if self.replace.is_none() {
            return false;
        }
        let plain = key.modifiers().is_empty();
        match key.code() {
            KeyCode::Char('y' | ' ') if plain => {
                let next = self.replace_current();
                self.next_replacement(next);
            }
            KeyCode::Char('n') | KeyCode::Backspace if plain => {
                let next = self
                    .replace
                    .as_ref()
                    .map_or(0, |replace| after(&replace.current));
                self.next_replacement(next);
            }
            KeyCode::Char('q') | KeyCode::Return if plain => self.finish_replace(),
            _ if is_keyboard_quit(key) => {
                self.finish_replace();
                self.show_message("Quit");
            }
            _ => {
                self.finish_replace();
                return false;
            }
        }
        self.reveal_point();
        true
    }

    fn replace_current(&mut self) -> usize {
        let Some(replace) = &mut self.replace else {
            return 0;
        };
        let buffer = self.buffers.current_mut();
        let current = replace.current.clone();
        let text = replace
            .pattern
            .expand(&Haystack::new(buffer), current.clone(), &replace.to);
        buffer.delete_range(current.clone());
        buffer.set_point(current.start);
        buffer.insert(&text);
        replace.count += 1;
        buffer.point() + usize::from(current.is_empty())
    }

    fn finish_replace(&mut self) {
        let Some(replace) = self.replace.take() else {
            return;
        };
        self.buffers.current_mut().close_undo_step();
        let plural = if replace.count == 1 { "" } else { "s" };
        self.show_message(format!("Replaced {} occurrence{plural}", replace.count));
    }

    pub(super) fn replace_marks(&self, line: usize) -> Vec<(Range<usize>, Highlight)> {
        let Some(replace) = &self.replace else {
            return Vec::new();
        };
        let buffer = self.buffers.current();
        let lazy = if replace.revision == buffer.revision() {
            &replace.lazy[..]
        } else {
            &[]
        };
        match_marks(buffer, line, lazy, Some(&replace.current))
    }
}

fn after(found: &Range<usize>) -> usize {
    found.end + usize::from(found.is_empty())
}

fn regexp_word(regex: bool) -> &'static str {
    if regex { "regexp " } else { "" }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, press, type_text};
    use crate::theme::Highlight;

    fn prompt(editor: &Editor) -> String {
        editor.minibuffer().unwrap().prompt().to_owned()
    }

    fn replacing(text: &str, point: usize, from: &str, to: &str) -> Editor {
        let mut editor = editor_with(text, point);
        press(&mut editor, "M-%");
        type_text(&mut editor, from);
        press(&mut editor, "RET");
        type_text(&mut editor, to);
        press(&mut editor, "RET");
        editor
    }

    #[test]
    fn m_percent_asks_for_the_text_then_its_replacement() {
        let mut editor = editor_with("a foo b foo", 0);

        press(&mut editor, "M-%");
        assert_eq!(prompt(&editor), "Query replace: ");

        type_text(&mut editor, "foo");
        press(&mut editor, "RET");
        assert_eq!(prompt(&editor), "Query replace foo with: ");

        type_text(&mut editor, "bar");
        press(&mut editor, "RET");
        assert!(editor.minibuffer().is_none());
        assert_eq!(
            editor.replacing().unwrap().prompt(),
            "Query replacing foo with bar: (y, n, !, ., ^, q) "
        );
        assert_eq!(editor.buffer().point(), 5);
    }

    #[test]
    fn y_replaces_and_n_skips() {
        let mut editor = replacing("a foo b foo c foo", 0, "foo", "bar");

        press(&mut editor, "y n y");

        assert_eq!(editor.buffer().contents(), "a bar b foo c bar");
        assert!(editor.replacing().is_none());
        assert_eq!(editor.message(), Some("Replaced 2 occurrences"));
    }

    #[test]
    fn space_and_delete_answer_like_y_and_n() {
        let mut editor = replacing("foo foo", 0, "foo", "bar");

        press(&mut editor, "SPC DEL");

        assert_eq!(editor.buffer().contents(), "bar foo");
        assert_eq!(editor.message(), Some("Replaced 1 occurrence"));
    }

    #[test]
    fn q_and_return_stop_replacing() {
        for key in ["q", "RET"] {
            let mut editor = replacing("foo foo", 0, "foo", "bar");

            press(&mut editor, &format!("y {key}"));

            assert_eq!(editor.buffer().contents(), "bar foo", "{key}");
            assert!(editor.replacing().is_none(), "{key}");
            assert_eq!(editor.buffer().point(), 7, "{key}");
        }
    }

    #[test]
    fn keyboard_quit_stops_and_keeps_what_was_replaced() {
        let mut editor = replacing("foo foo", 0, "foo", "bar");

        press(&mut editor, "y C-g");

        assert_eq!(editor.buffer().contents(), "bar foo");
        assert_eq!(editor.message(), Some("Quit"));
    }

    #[test]
    fn other_keys_stop_and_run_as_usual() {
        let mut editor = replacing("foo foo\nx", 0, "foo", "bar");

        press(&mut editor, "y C-e");

        assert!(editor.replacing().is_none());
        assert_eq!(editor.buffer().point(), 7);
    }

    #[test]
    fn replacing_covers_point_to_the_end() {
        let mut editor = replacing("foo x foo", 2, "foo", "bar");

        press(&mut editor, "y");

        assert_eq!(editor.buffer().contents(), "foo x bar");
        assert_eq!(editor.buffer().mark(), Some(2));
    }

    #[test]
    fn nothing_to_replace_finishes_at_once() {
        let editor = replacing("abc", 0, "foo", "bar");

        assert!(editor.replacing().is_none());
        assert_eq!(editor.message(), Some("Replaced 0 occurrences"));
    }

    #[test]
    fn a_whole_replace_undoes_in_one_step() {
        let mut editor = replacing("foo foo foo", 0, "foo", "bar");
        press(&mut editor, "y y y");
        assert_eq!(editor.buffer().contents(), "bar bar bar");

        press(&mut editor, "C-/");

        assert_eq!(editor.buffer().contents(), "foo foo foo");
    }

    #[test]
    fn read_only_buffers_refuse_replacing() {
        let mut editor = editor_with("foo", 0);
        editor.buffers.current_mut().set_read_only(true);

        press(&mut editor, "M-%");

        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.message(), Some("Buffer is read-only: t"));
    }

    #[test]
    fn the_match_in_question_and_the_others_are_marked() {
        let editor = replacing("foo foo", 0, "foo", "bar");

        assert_eq!(
            editor.line_marks(0),
            [
                (4..7, Highlight::SearchMatch),
                (0..3, Highlight::SearchCurrent)
            ]
        );
    }
}
