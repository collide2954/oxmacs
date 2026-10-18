use std::mem;
use std::ops::Range;

use crate::command::Command;
use crate::display::ScreenLine;
use crate::editor::{Editor, inserts_itself};
use crate::input::{Key, KeyCode, Lookup};
use crate::search::{Haystack, Pattern};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Direction {
    Forward,
    Backward,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Step {
    query: String,
    regex: bool,
    direction: Direction,
    found: Option<Range<usize>>,
    success: bool,
    wrapped: bool,
}

#[derive(Debug)]
pub struct Search {
    origin: usize,
    origin_top: ScreenLine,
    step: Step,
    previous: Vec<Step>,
    note: Option<&'static str>,
}

impl Search {
    pub fn prompt(&self) -> String {
        let step = &self.step;
        let mut words = Vec::new();
        if !step.success {
            words.push("failing");
        }
        if step.wrapped {
            words.push("wrapped");
        }
        if step.regex {
            words.push("regexp");
        }
        words.push("I-search");
        if step.direction == Direction::Backward {
            words.push("backward");
        }
        let mut prompt = capitalize(&words.join(" "));
        prompt.push_str(": ");
        prompt
    }

    pub fn query(&self) -> &str {
        &self.step.query
    }

    pub fn failing_from(&self) -> Option<usize> {
        if self.step.success {
            return None;
        }
        let matched = self
            .previous
            .iter()
            .rev()
            .find(|step| step.success)
            .map_or(0, |step| step.query.chars().count());
        Some(matched.min(self.step.query.chars().count()))
    }

    pub fn note(&self) -> Option<&str> {
        self.note
    }
}

impl Editor {
    pub fn search(&self) -> Option<&Search> {
        self.search.as_ref()
    }

    pub(super) fn search_key(&mut self, key: Key) -> bool {
        let Some(search) = self.search.as_mut().filter(|_| self.pending.is_empty()) else {
            return false;
        };
        search.note = None;
        match self.search_keymap.lookup(&[key]) {
            Lookup::Command(command) => self.dispatch(key, command, false),
            _ if inserts_itself(key) => self.dispatch(key, Command::IsearchPrintingChar, false),
            _ => {
                self.exit_search();
                return false;
            }
        }
        true
    }

    pub(super) fn isearch(&mut self, direction: Direction, regex: bool) {
        if self.minibuffer.is_some() {
            self.show_message("Command attempted to use minibuffer while in minibuffer");
            return;
        }
        self.search = Some(Search {
            origin: self.buffers.current().point(),
            origin_top: self.viewport.top(),
            step: Step {
                query: String::new(),
                regex,
                direction,
                found: None,
                success: true,
                wrapped: false,
            },
            previous: Vec::new(),
            note: None,
        });
    }

    pub(super) fn isearch_printing_char(&mut self) {
        if let Some(KeyCode::Char(c)) = self.last_key.map(Key::code) {
            self.extend_search(c.encode_utf8(&mut [0; 4]));
        }
    }

    fn extend_search(&mut self, text: &str) {
        let Some(search) = &self.search else {
            return;
        };
        let step = &search.step;
        let next = Step {
            query: format!("{}{text}", step.query),
            ..step.clone()
        };
        let next = if step.success || step.regex {
            match &step.found {
                Some(found) => self.locate(next, found.start, true),
                None => self.locate(next, search.origin, false),
            }
        } else {
            next
        };
        self.push_step(next);
    }

    pub(super) fn isearch_repeat(&mut self, direction: Direction) {
        let Some(search) = &self.search else {
            return;
        };
        let step = &search.step;
        let next = Step {
            direction,
            ..step.clone()
        };
        let next = if step.query.is_empty() {
            let Some(query) = self.last_searches[usize::from(step.regex)].clone() else {
                self.note_search("[No previous search string]");
                return;
            };
            self.locate(Step { query, ..next }, search.origin, false)
        } else if !step.success && direction == step.direction {
            let anchor = match direction {
                Direction::Forward => 0,
                Direction::Backward => self.buffers.current().len_chars(),
            };
            self.locate(
                Step {
                    wrapped: true,
                    ..next
                },
                anchor,
                false,
            )
        } else {
            match (&step.found, direction) {
                (Some(found), Direction::Forward) => {
                    self.locate(next, found.end + usize::from(found.is_empty()), false)
                }
                (Some(found), Direction::Backward) => self.locate(next, found.start, false),
                (None, _) => self.locate(next, search.origin, false),
            }
        };
        self.push_step(next);
    }

    pub(super) fn isearch_toggle_regexp(&mut self) {
        let Some(search) = &self.search else {
            return;
        };
        let next = Step {
            regex: !search.step.regex,
            wrapped: false,
            ..search.step.clone()
        };
        let next = if next.query.is_empty() {
            next
        } else {
            self.locate(next, search.origin, false)
        };
        self.push_step(next);
    }

    fn note_search(&mut self, note: &'static str) {
        if let Some(search) = &mut self.search {
            search.note = Some(note);
        }
    }

    fn remember_search(&mut self, step: &Step) {
        if !step.query.is_empty() {
            self.last_searches[usize::from(step.regex)] = Some(step.query.clone());
        }
    }

    fn locate(&self, mut step: Step, anchor: usize, inclusive: bool) -> Step {
        let buffer = self.buffers.current();
        let found = Pattern::new(&step.query, step.regex)
            .ok()
            .and_then(|pattern| {
                let haystack = Haystack::new(buffer);
                match step.direction {
                    Direction::Forward => pattern.find_forward(&haystack, anchor),
                    Direction::Backward => {
                        pattern.find_backward(&haystack, anchor + usize::from(inclusive))
                    }
                }
            });
        step.success = found.is_some();
        if found.is_some() {
            step.found = found;
        }
        step
    }

    fn push_step(&mut self, step: Step) {
        if let Some(search) = &mut self.search {
            search.previous.push(mem::replace(&mut search.step, step));
        }
        self.refresh_search();
    }

    fn refresh_search(&mut self) {
        let Some(search) = &mut self.search else {
            return;
        };
        if search.step.regex && Pattern::new(&search.step.query, true).is_err() {
            search.note = Some("[Invalid regexp]");
        }
        let point = match (&search.step.found, search.step.direction) {
            (Some(found), Direction::Forward) => found.end,
            (Some(found), Direction::Backward) => found.start,
            (None, _) => search.origin,
        };
        self.buffers.current_mut().set_point(point);
    }

    pub(super) fn exit_search(&mut self) {
        let Some(search) = self.search.take() else {
            return;
        };
        self.remember_search(&search.step);
        let buffer = self.buffers.current_mut();
        if buffer.point() != search.origin && !buffer.mark_active() {
            buffer.set_mark(search.origin);
            buffer.deactivate_mark();
            self.show_message("Mark saved where search started");
        }
    }

    pub(super) fn isearch_delete_char(&mut self) {
        if let Some(search) = &mut self.search
            && let Some(previous) = search.previous.pop()
        {
            search.step = previous;
        }
        self.refresh_search();
    }

    pub(super) fn isearch_abort(&mut self) {
        if let Some(search) = self.search.as_mut().filter(|search| !search.step.success) {
            while !search.step.success
                && let Some(previous) = search.previous.pop()
            {
                search.step = previous;
            }
            self.refresh_search();
            return;
        }
        let Some(search) = self.search.take() else {
            return;
        };
        self.remember_search(&search.step);
        self.buffers.current_mut().set_point(search.origin);
        self.viewport.set_top(search.origin_top);
        self.show_message("Quit");
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, press, type_text};

    fn prompt(editor: &Editor) -> String {
        editor.search().unwrap().prompt()
    }

    fn point(editor: &Editor) -> usize {
        editor.buffer().point()
    }

    #[test]
    fn typing_moves_point_to_the_end_of_the_match() {
        let mut editor = editor_with("foo bar baz", 0);

        press(&mut editor, "C-s b");
        assert_eq!(point(&editor), 5);

        type_text(&mut editor, "az");
        assert_eq!(point(&editor), 11);
        assert_eq!(editor.search().unwrap().query(), "baz");
        assert_eq!(prompt(&editor), "I-search: ");
    }

    #[test]
    fn backward_search_puts_point_at_the_start_of_the_match() {
        let mut editor = editor_with("foo bar foo", 11);

        press(&mut editor, "C-r");
        assert_eq!(prompt(&editor), "I-search backward: ");

        type_text(&mut editor, "fo");
        assert_eq!(point(&editor), 8);
    }

    #[test]
    fn a_failing_query_keeps_point_at_the_last_match() {
        let mut editor = editor_with("foo", 0);

        press(&mut editor, "C-s f o x");

        assert_eq!(point(&editor), 2);
        assert_eq!(prompt(&editor), "Failing I-search: ");
    }

    #[test]
    fn return_ends_the_search_and_saves_the_mark_where_it_started() {
        let mut editor = editor_with("abc foo", 1);

        press(&mut editor, "C-s f o o RET");

        assert!(editor.search().is_none());
        assert_eq!(point(&editor), 7);
        assert_eq!(editor.buffer().mark(), Some(1));
        assert_eq!(editor.buffer().selection(), None);
        assert_eq!(editor.message(), Some("Mark saved where search started"));
    }

    #[test]
    fn an_active_selection_extends_to_the_match() {
        let mut editor = editor_with("abc foo", 1);

        press(&mut editor, "C-SPC C-s f o o RET");

        assert_eq!(editor.buffer().selection(), Some(1..7));
    }

    #[test]
    fn keyboard_quit_returns_to_where_the_search_started() {
        let mut editor = editor_with("abc foo", 1);

        press(&mut editor, "C-s f o o C-g");

        assert!(editor.search().is_none());
        assert_eq!(point(&editor), 1);
        assert_eq!(editor.message(), Some("Quit"));
    }

    #[test]
    fn other_keys_end_the_search_and_run_as_usual() {
        let mut editor = editor_with("foo\nbar baz", 0);

        press(&mut editor, "C-s b a z C-a");

        assert!(editor.search().is_none());
        assert_eq!(point(&editor), 4);
    }

    #[test]
    fn repeating_moves_to_the_next_match() {
        let mut editor = editor_with("foo foo foo", 0);

        press(&mut editor, "C-s f o o C-s");
        assert_eq!(point(&editor), 7);

        press(&mut editor, "C-s");
        assert_eq!(point(&editor), 11);
    }

    #[test]
    fn repeating_backward_moves_to_the_previous_match() {
        let mut editor = editor_with("foo foo foo", 11);

        press(&mut editor, "C-r f o o C-r");

        assert_eq!(point(&editor), 4);
    }

    #[test]
    fn changing_direction_moves_to_the_previous_match_at_once() {
        let mut editor = editor_with("foo foo foo", 0);

        press(&mut editor, "C-s f o o C-s C-r");

        assert_eq!(point(&editor), 0);
        assert_eq!(prompt(&editor), "I-search backward: ");
    }

    #[test]
    fn repeating_past_the_last_match_fails_then_wraps() {
        let mut editor = editor_with("foo foo", 1);

        press(&mut editor, "C-s f o o C-s");
        assert_eq!(prompt(&editor), "Failing I-search: ");
        assert_eq!(point(&editor), 7);

        press(&mut editor, "C-s");
        assert_eq!(prompt(&editor), "Wrapped I-search: ");
        assert_eq!(point(&editor), 3);
    }

    #[test]
    fn backward_searches_wrap_to_the_end() {
        let mut editor = editor_with("foo foo", 2);

        press(&mut editor, "C-r f o o C-r");
        assert_eq!(prompt(&editor), "Failing I-search backward: ");

        press(&mut editor, "C-r");
        assert_eq!(point(&editor), 4);
        assert_eq!(prompt(&editor), "Wrapped I-search backward: ");
    }

    #[test]
    fn an_empty_query_repeats_the_last_search() {
        let mut editor = editor_with("foo foo", 0);

        press(&mut editor, "C-s f o o RET C-s C-s");

        assert_eq!(editor.search().unwrap().query(), "foo");
        assert_eq!(point(&editor), 7);
    }

    #[test]
    fn aborted_searches_can_be_repeated_too() {
        let mut editor = editor_with("foo foo", 0);

        press(&mut editor, "C-s f o o C-g C-r C-r");

        assert_eq!(editor.search().unwrap().query(), "foo");
    }

    #[test]
    fn without_a_previous_search_there_is_nothing_to_repeat() {
        let mut editor = editor_with("foo", 0);

        press(&mut editor, "C-s C-s");
        assert_eq!(
            editor.search().unwrap().note(),
            Some("[No previous search string]")
        );

        press(&mut editor, "f");
        assert_eq!(editor.search().unwrap().note(), None);
    }

    #[test]
    fn delete_undoes_the_last_search_step() {
        let mut editor = editor_with("foo foo", 0);

        press(&mut editor, "C-s f o o C-s DEL");
        assert_eq!(point(&editor), 3);
        assert_eq!(editor.search().unwrap().query(), "foo");

        press(&mut editor, "DEL");
        assert_eq!(point(&editor), 2);
        assert_eq!(editor.search().unwrap().query(), "fo");

        press(&mut editor, "DEL DEL DEL");
        assert_eq!(point(&editor), 0);
        assert_eq!(editor.search().unwrap().query(), "");
    }

    #[test]
    fn the_failing_part_of_the_query_is_known() {
        let mut editor = editor_with("foo", 0);

        press(&mut editor, "C-s f o");
        assert_eq!(editor.search().unwrap().failing_from(), None);

        press(&mut editor, "x x");
        assert_eq!(editor.search().unwrap().failing_from(), Some(2));
    }

    #[test]
    fn keyboard_quit_while_failing_rubs_out_the_failing_part() {
        let mut editor = editor_with("foo", 0);

        press(&mut editor, "C-s f o x x C-g");
        assert_eq!(editor.search().unwrap().query(), "fo");
        assert_eq!(prompt(&editor), "I-search: ");
        assert_eq!(point(&editor), 2);

        press(&mut editor, "C-g");
        assert!(editor.search().is_none());
        assert_eq!(point(&editor), 0);
    }

    #[test]
    fn regexp_searches_match_patterns() {
        let mut editor = editor_with("a1 b22", 0);

        press(&mut editor, "C-M-s");
        assert_eq!(prompt(&editor), "Regexp I-search: ");

        type_text(&mut editor, "b2+");
        assert_eq!(point(&editor), 6);
    }

    #[test]
    fn regexp_searches_go_backward_too() {
        let mut editor = editor_with("ab ab", 5);

        press(&mut editor, "C-M-r a .");

        assert_eq!(prompt(&editor), "Regexp I-search backward: ");
        assert_eq!(point(&editor), 3);
    }

    #[test]
    fn invalid_regexps_fail_with_a_note_until_completed() {
        let mut editor = editor_with("(a)", 0);

        press(&mut editor, "C-M-s (");
        assert_eq!(prompt(&editor), "Failing regexp I-search: ");
        assert_eq!(editor.search().unwrap().note(), Some("[Invalid regexp]"));

        press(&mut editor, "a )");
        assert_eq!(point(&editor), 2);
        assert_eq!(editor.search().unwrap().note(), None);
    }

    #[test]
    fn meta_r_toggles_regexp_matching() {
        let mut editor = editor_with("abc a.c", 0);
        press(&mut editor, "C-s a .");
        assert_eq!(point(&editor), 6);

        press(&mut editor, "M-r");
        assert_eq!(prompt(&editor), "Regexp I-search: ");
        assert_eq!(point(&editor), 2);

        press(&mut editor, "M-r");
        assert_eq!(prompt(&editor), "I-search: ");
        assert_eq!(point(&editor), 6);
    }

    #[test]
    fn search_is_refused_in_the_minibuffer() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x C-s");

        assert!(editor.search().is_none());
        assert_eq!(
            editor.message(),
            Some("Command attempted to use minibuffer while in minibuffer")
        );
    }
}
