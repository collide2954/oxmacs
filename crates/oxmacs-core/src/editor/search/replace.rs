use std::ops::Range;

use crate::editor::prompts::minibuffer::Purpose;
use crate::editor::search::isearch::match_marks;
use crate::editor::{Editor, is_keyboard_quit};
use crate::input::{Key, KeyCode};
use crate::search::{Haystack, Pattern};
use crate::theme::Highlight;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Visited {
    range: Range<usize>,
    replaced: bool,
}

#[derive(Debug)]
pub struct Replace {
    pattern: Pattern,
    regex: bool,
    from: String,
    to: String,
    end_gap: usize,
    current: Range<usize>,
    revisited: bool,
    visited: Vec<Visited>,
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

    pub(in crate::editor) fn query_replace(&mut self, regex: bool) {
        let default = self.last_replacements[usize::from(regex)]
            .as_ref()
            .map(|(from, to)| format!(" (default {from} → {to})"))
            .unwrap_or_default();
        let kind = if regex { " regexp" } else { "" };
        self.open_minibuffer(
            &format!("Query replace{kind}{default}: "),
            Purpose::ReplaceFrom(regex),
        );
    }

    pub(in crate::editor) fn isearch_query_replace(&mut self, regex: bool) {
        let Some(search) = self.search() else {
            return;
        };
        let query = search.query().to_owned();
        let regex = regex || search.is_regex();
        let start = search.found().map(|found| found.start);
        self.exit_search();
        if let Some(start) = start {
            self.buffers.current_mut().set_point(start);
        }
        if query.is_empty() {
            self.query_replace(regex);
        } else {
            self.ask_replacement(query, regex);
        }
    }

    pub(in crate::editor) fn exit_replace_prompt(&mut self, purpose: Purpose) {
        let input = self.prompts.input();
        match purpose {
            Purpose::ReplaceFrom(regex) if input.is_empty() => {
                self.prompts.done_reading();
                if let Some((from, to)) = self.last_replacements[usize::from(regex)].clone() {
                    self.start_replace(from, to, regex);
                }
            }
            Purpose::ReplaceFrom(regex) if Pattern::new(&input, regex).is_err() => {
                if let Some(minibuffer) = self.prompts.minibuffer_mut() {
                    minibuffer.note = Some("[Invalid regexp]".to_owned());
                }
            }
            Purpose::ReplaceFrom(regex) => {
                self.prompts.done_reading();
                self.ask_replacement(input, regex);
            }
            Purpose::ReplaceTo(regex) => {
                let from = self
                    .prompts
                    .done_reading()
                    .map(|minibuffer| minibuffer.from)
                    .unwrap_or_default();
                self.start_replace(from, input, regex);
            }
            _ => {}
        }
    }

    pub(in crate::editor) fn ask_replacement(&mut self, from: String, regex: bool) {
        let prompt = format!("Query replace {}{from} with: ", regexp_word(regex));
        if self.open_minibuffer(&prompt, Purpose::ReplaceTo(regex))
            && let Some(minibuffer) = self.prompts.minibuffer_mut()
        {
            minibuffer.from = from;
        }
    }

    fn start_replace(&mut self, from: String, to: String, regex: bool) {
        let Ok(pattern) = Pattern::new(&from, regex) else {
            return;
        };
        self.shift_selection = false;
        self.last_replacements[usize::from(regex)] = Some((from.clone(), to.clone()));
        let buffer = self.buffers.current_mut();
        let origin = buffer.point();
        let scope = buffer.selection().unwrap_or(origin..buffer.len_chars());
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
            revisited: false,
            visited: Vec::new(),
            count: 0,
            lazy: Vec::new(),
            revision: buffer.revision(),
        });
        self.next_replacement(scope.start);
    }

    fn next_replacement(&mut self, from: usize) {
        let Some(replace) = &self.replace else {
            return;
        };
        let buffer = self.buffers.current();
        let end = buffer.len_chars().saturating_sub(replace.end_gap);
        let found = replace
            .pattern
            .find_forward(&Haystack::new(buffer), from)
            .filter(|found| from <= end && found.end <= end);
        match found {
            Some(found) => self.show_replacement(found, false),
            None => self.finish_replace(),
        }
    }

    fn show_replacement(&mut self, found: Range<usize>, revisited: bool) {
        let Some(replace) = &mut self.replace else {
            return;
        };
        let buffer = self.buffers.current_mut();
        let end = buffer.len_chars().saturating_sub(replace.end_gap);
        replace.lazy = replace
            .pattern
            .find_all(&Haystack::new(buffer), found.start..end);
        replace.revision = buffer.revision();
        buffer.set_point(found.end);
        replace.current = found;
        replace.revisited = revisited;
    }

    pub(in crate::editor) fn answer_replace(&mut self, key: Key) -> bool {
        if self.replace.is_none() {
            return false;
        }
        let plain = key.modifiers().is_empty();
        match key.code() {
            KeyCode::Char('y' | ' ') if plain => {
                let next = self.act_on_replacement();
                self.next_replacement(next);
            }
            KeyCode::Char('n') | KeyCode::Backspace if plain => {
                let next = self.skip_replacement();
                self.next_replacement(next);
            }
            KeyCode::Char('!') if plain => {
                let next = self.act_on_replacement();
                self.replace_rest(next);
                self.finish_replace();
            }
            KeyCode::Char('.') if plain => {
                self.act_on_replacement();
                self.finish_replace();
            }
            KeyCode::Char('^') if plain => self.previous_replacement(),
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

    fn act_on_replacement(&mut self) -> usize {
        let Some(replace) = &mut self.replace else {
            return 0;
        };
        let current = replace.current.clone();
        let range = if replace.revisited {
            current.clone()
        } else {
            let buffer = self.buffers.current_mut();
            let text = replace
                .pattern
                .expand(&Haystack::new(buffer), current.clone(), &replace.to);
            buffer.delete_range(current.clone());
            buffer.set_point(current.start);
            buffer.insert(&text);
            replace.count += 1;
            current.start..buffer.point()
        };
        let next = range.end + usize::from(current.is_empty());
        replace.visited.push(Visited {
            range,
            replaced: true,
        });
        next
    }

    fn skip_replacement(&mut self) -> usize {
        let Some(replace) = &mut self.replace else {
            return 0;
        };
        replace.visited.push(Visited {
            range: replace.current.clone(),
            replaced: replace.revisited,
        });
        after(&replace.current)
    }

    fn previous_replacement(&mut self) {
        match self
            .replace
            .as_mut()
            .and_then(|replace| replace.visited.pop())
        {
            Some(visited) => self.show_replacement(visited.range, visited.replaced),
            None => self.show_message("No previous match"),
        }
    }

    fn replace_rest(&mut self, from: usize) {
        let Some(replace) = &mut self.replace else {
            return;
        };
        let buffer = self.buffers.current_mut();
        let end = buffer.len_chars().saturating_sub(replace.end_gap);
        if from > end {
            return;
        }
        let edits: Vec<(Range<usize>, String)> = {
            let haystack = Haystack::new(buffer);
            replace
                .pattern
                .find_all(&haystack, from..end)
                .into_iter()
                .map(|found| {
                    let text = replace
                        .pattern
                        .expand(&haystack, found.clone(), &replace.to);
                    (found, text)
                })
                .collect()
        };
        let (mut grown, mut shrunk) = (0, 0);
        for (found, text) in edits {
            let start = found.start + grown - shrunk;
            buffer.delete_range(start..start + found.len());
            buffer.set_point(start);
            buffer.insert(&text);
            grown += text.chars().count();
            shrunk += found.len();
            replace.count += 1;
        }
    }

    fn finish_replace(&mut self) {
        let Some(replace) = self.replace.take() else {
            return;
        };
        self.buffers.current_mut().close_undo_step();
        let plural = if replace.count == 1 { "" } else { "s" };
        self.show_message(format!("Replaced {} occurrence{plural}", replace.count));
    }

    pub(in crate::editor) fn replace_marks(&self, line: usize) -> Vec<(Range<usize>, Highlight)> {
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
    use crate::testing::{editor_with, press, type_text};
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
    fn bang_replaces_all_the_rest() {
        let mut editor = replacing("foo foo foo x", 0, "foo", "bar");

        press(&mut editor, "n !");

        assert_eq!(editor.buffer().contents(), "foo bar bar x");
        assert_eq!(editor.message(), Some("Replaced 2 occurrences"));
        assert_eq!(editor.buffer().point(), 11);
    }

    #[test]
    fn dot_replaces_this_one_and_stops() {
        let mut editor = replacing("foo foo", 0, "foo", "bar");

        press(&mut editor, ".");

        assert_eq!(editor.buffer().contents(), "bar foo");
        assert!(editor.replacing().is_none());
    }

    #[test]
    fn caret_goes_back_to_the_previous_match() {
        let mut editor = replacing("foo foo foo", 0, "foo", "bar");

        press(&mut editor, "n ^");
        assert_eq!(editor.buffer().point(), 3);

        press(&mut editor, "y");
        assert_eq!(editor.buffer().contents(), "bar foo foo");
        assert_eq!(editor.buffer().point(), 7);
    }

    #[test]
    fn revisited_replacements_are_not_replaced_twice() {
        let mut editor = replacing("foo foo", 0, "o", "oo");

        press(&mut editor, "y ^ y");

        assert_eq!(editor.buffer().contents(), "fooo foo");
        assert_eq!(editor.buffer().point(), 4);
    }

    #[test]
    fn caret_at_the_first_match_says_so() {
        let mut editor = replacing("foo", 0, "foo", "bar");

        press(&mut editor, "^");

        assert_eq!(editor.message(), Some("No previous match"));
        assert!(editor.replacing().is_some());
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
    fn an_active_selection_limits_the_replacing() {
        let mut editor = editor_with("foo foo foo", 0);
        press(&mut editor, "C-SPC M-f M-f M-%");
        type_text(&mut editor, "foo");
        press(&mut editor, "RET");
        type_text(&mut editor, "bar");

        press(&mut editor, "RET !");

        assert_eq!(editor.buffer().contents(), "bar bar foo");
        assert_eq!(editor.buffer().selection(), None);
    }

    #[test]
    fn regexp_replacements_expand_groups() {
        let mut editor = editor_with("a1 b2", 0);
        press(&mut editor, "C-M-%");
        assert_eq!(prompt(&editor), "Query replace regexp: ");
        type_text(&mut editor, r"(\w)(\d)");
        press(&mut editor, "RET");
        assert_eq!(prompt(&editor), r"Query replace regexp (\w)(\d) with: ");
        type_text(&mut editor, r"\2\1");

        press(&mut editor, "RET !");

        assert_eq!(editor.buffer().contents(), "1a 2b");
    }

    #[test]
    fn invalid_regexps_are_refused_with_a_note() {
        let mut editor = editor_with("(", 0);

        press(&mut editor, "C-M-% ( RET");

        let minibuffer = editor.minibuffer().unwrap();
        assert_eq!(minibuffer.prompt(), "Query replace regexp: ");
        assert_eq!(minibuffer.note(), Some("[Invalid regexp]"));
    }

    #[test]
    fn the_last_pair_is_the_default() {
        let mut editor = replacing("foo foo", 0, "foo", "bar");
        press(&mut editor, "q M-<");

        press(&mut editor, "M-%");
        assert_eq!(prompt(&editor), "Query replace (default foo → bar): ");

        press(&mut editor, "RET");
        assert_eq!(
            editor.replacing().unwrap().prompt(),
            "Query replacing foo with bar: (y, n, !, ., ^, q) "
        );
    }

    #[test]
    fn m_percent_in_a_search_replaces_its_query_from_the_match() {
        let mut editor = editor_with("foo x foo foo", 0);

        press(&mut editor, "C-s f o o C-s M-%");
        assert!(editor.search().is_none());
        assert_eq!(prompt(&editor), "Query replace foo with: ");

        type_text(&mut editor, "bar");
        press(&mut editor, "RET !");
        assert_eq!(editor.buffer().contents(), "foo x bar bar");
    }

    #[test]
    fn regexp_searches_hand_over_a_regexp() {
        for keys in ["C-M-s a . C-M-%", "C-M-s a . M-%", "C-s a . C-M-%"] {
            let mut editor = editor_with("a1 a2", 0);

            press(&mut editor, keys);

            assert_eq!(prompt(&editor), "Query replace regexp a. with: ", "{keys}");
        }
    }

    #[test]
    fn lowercase_queries_keep_the_case_of_what_they_replace() {
        let mut editor = replacing("Foo FOO foo", 0, "foo", "bar");

        press(&mut editor, "!");

        assert_eq!(editor.buffer().contents(), "Bar BAR bar");
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
