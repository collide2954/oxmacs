use std::borrow::Cow;
use std::ops::Range;

use ropey::Rope;

#[derive(Debug, Clone)]
pub struct Buffer {
    name: String,
    text: Rope,
    point: usize,
    mark: Option<usize>,
    mark_active: bool,
}

impl Buffer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text: Rope::new(),
            point: 0,
            mark: None,
            mark_active: false,
        }
    }

    pub fn with_text(name: impl Into<String>, text: &str) -> Self {
        Self {
            text: Rope::from_str(text),
            ..Self::new(name)
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn len_chars(&self) -> usize {
        self.text.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.text.len_lines()
    }

    pub fn line_of(&self, position: usize) -> usize {
        self.text.char_to_line(position.min(self.len_chars()))
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.text.line_to_char(self.clamp_line(line))
    }

    pub fn line_end(&self, line: usize) -> usize {
        let line = self.clamp_line(line);
        let slice = self.text.line(line);
        let len = slice.len_chars();
        let content = if len > 0 && slice.char(len - 1) == '\n' {
            len - 1
        } else {
            len
        };
        self.text.line_to_char(line) + content
    }

    pub fn line_text(&self, line: usize) -> Cow<'_, str> {
        self.text
            .slice(self.line_start(line)..self.line_end(line))
            .into()
    }

    pub fn chars_after(&self, position: usize) -> impl Iterator<Item = char> {
        self.text.chars_at(position.min(self.len_chars()))
    }

    pub fn chars_before(&self, position: usize) -> impl Iterator<Item = char> {
        self.text
            .chars_at(position.min(self.len_chars()))
            .reversed()
    }

    pub fn column(&self, position: usize) -> usize {
        position - self.line_start(self.line_of(position))
    }

    fn clamp_line(&self, line: usize) -> usize {
        line.min(self.len_lines() - 1)
    }

    pub fn point(&self) -> usize {
        self.point
    }

    pub fn set_point(&mut self, point: usize) {
        self.point = point.min(self.len_chars());
    }

    pub fn contents(&self) -> String {
        self.text.to_string()
    }

    pub fn mark(&self) -> Option<usize> {
        self.mark
    }

    pub fn mark_active(&self) -> bool {
        self.mark_active && self.mark.is_some()
    }

    pub fn set_mark(&mut self, mark: usize) {
        self.mark = Some(mark.min(self.len_chars()));
        self.mark_active = true;
    }

    pub fn deactivate_mark(&mut self) {
        self.mark_active = false;
    }

    pub fn selection(&self) -> Option<Range<usize>> {
        let mark = self.mark.filter(|_| self.mark_active)?;
        Some(mark.min(self.point)..mark.max(self.point))
    }

    pub fn exchange_point_and_mark(&mut self) -> bool {
        let Some(mark) = self.mark else {
            return false;
        };
        self.mark = Some(self.point);
        self.point = mark;
        self.mark_active = true;
        true
    }

    pub fn insert(&mut self, text: &str) {
        let at = self.point;
        let inserted = text.chars().count();
        self.text.insert(at, text);
        self.point += inserted;
        self.mark = self
            .mark
            .map(|mark| if mark > at { mark + inserted } else { mark });
    }

    pub fn delete_forward(&mut self, count: usize) {
        self.delete_range(self.point..self.point.saturating_add(count));
    }

    pub fn delete_backward(&mut self, count: usize) {
        self.delete_range(self.point.saturating_sub(count)..self.point);
    }

    pub fn text_in(&self, range: Range<usize>) -> String {
        let end = range.end.min(self.len_chars());
        let start = range.start.min(end);
        self.text.slice(start..end).to_string()
    }

    pub fn delete_range(&mut self, range: Range<usize>) -> String {
        let end = range.end.min(self.len_chars());
        let start = range.start.min(end);
        let removed = self.text_in(start..end);
        self.text.remove(start..end);
        self.point = position_after_removal(self.point, start, end);
        self.mark = self
            .mark
            .map(|mark| position_after_removal(mark, start, end));
        removed
    }
}

fn position_after_removal(position: usize, start: usize, end: usize) -> usize {
    if position >= end {
        position - (end - start)
    } else {
        position.min(start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_is_empty_with_point_at_start() {
        let buffer = Buffer::new("*scratch*");

        assert_eq!(buffer.name(), "*scratch*");
        assert_eq!(buffer.len_chars(), 0);
        assert_eq!(buffer.point(), 0);
        assert_eq!(buffer.contents(), "");
    }

    #[test]
    fn insert_places_text_at_point_and_advances_point() {
        let mut buffer = Buffer::new("t");

        buffer.insert("hello");
        buffer.insert(", world");

        assert_eq!(buffer.contents(), "hello, world");
        assert_eq!(buffer.point(), 12);
    }

    #[test]
    fn insert_counts_point_in_chars_not_bytes() {
        let mut buffer = Buffer::new("t");

        buffer.insert("héllo→");

        assert_eq!(buffer.point(), 6);
    }

    #[test]
    fn with_text_starts_with_point_at_beginning() {
        let buffer = Buffer::with_text("t", "abc");

        assert_eq!(buffer.contents(), "abc");
        assert_eq!(buffer.point(), 0);
    }

    #[test]
    fn set_point_clamps_to_end_of_buffer() {
        let mut buffer = Buffer::with_text("t", "abc");

        buffer.set_point(2);
        assert_eq!(buffer.point(), 2);

        buffer.set_point(99);
        assert_eq!(buffer.point(), 3);
    }

    #[test]
    fn insert_in_middle_keeps_surrounding_text() {
        let mut buffer = Buffer::with_text("t", "ac");
        buffer.set_point(1);

        buffer.insert("b");

        assert_eq!(buffer.contents(), "abc");
        assert_eq!(buffer.point(), 2);
    }

    #[test]
    fn delete_forward_removes_chars_after_point() {
        let mut buffer = Buffer::with_text("t", "abcd");
        buffer.set_point(1);

        buffer.delete_forward(2);

        assert_eq!(buffer.contents(), "ad");
        assert_eq!(buffer.point(), 1);
    }

    #[test]
    fn delete_forward_stops_at_end_of_buffer() {
        let mut buffer = Buffer::with_text("t", "ab");
        buffer.set_point(1);

        buffer.delete_forward(5);

        assert_eq!(buffer.contents(), "a");
    }

    #[test]
    fn delete_backward_removes_chars_before_point() {
        let mut buffer = Buffer::with_text("t", "abcd");
        buffer.set_point(3);

        buffer.delete_backward(2);

        assert_eq!(buffer.contents(), "ad");
        assert_eq!(buffer.point(), 1);
    }

    #[test]
    fn delete_backward_stops_at_start_of_buffer() {
        let mut buffer = Buffer::with_text("t", "ab");
        buffer.set_point(1);

        buffer.delete_backward(5);

        assert_eq!(buffer.contents(), "b");
        assert_eq!(buffer.point(), 0);
    }

    #[test]
    fn delete_range_returns_removed_text_and_shifts_point() {
        let mut buffer = Buffer::with_text("t", "hello world");
        buffer.set_point(11);

        let removed = buffer.delete_range(0..6);

        assert_eq!(removed, "hello ");
        assert_eq!(buffer.contents(), "world");
        assert_eq!(buffer.point(), 5);
    }

    #[test]
    fn delete_range_moves_point_inside_range_to_its_start() {
        let mut buffer = Buffer::with_text("t", "hello world");
        buffer.set_point(3);

        buffer.delete_range(1..8);

        assert_eq!(buffer.contents(), "hrld");
        assert_eq!(buffer.point(), 1);
    }

    #[test]
    fn len_lines_counts_line_after_trailing_newline() {
        assert_eq!(Buffer::new("t").len_lines(), 1);
        assert_eq!(Buffer::with_text("t", "ab").len_lines(), 1);
        assert_eq!(Buffer::with_text("t", "ab\n").len_lines(), 2);
        assert_eq!(Buffer::with_text("t", "ab\ncd").len_lines(), 2);
    }

    #[test]
    fn line_queries_locate_line_boundaries() {
        let buffer = Buffer::with_text("t", "ab\ncde\n");

        assert_eq!(buffer.line_of(0), 0);
        assert_eq!(buffer.line_of(2), 0);
        assert_eq!(buffer.line_of(3), 1);
        assert_eq!(buffer.line_of(7), 2);
        assert_eq!(buffer.line_start(1), 3);
        assert_eq!(buffer.line_end(0), 2);
        assert_eq!(buffer.line_end(1), 6);
        assert_eq!(buffer.line_end(2), 7);
    }

    #[test]
    fn line_text_excludes_line_break() {
        let buffer = Buffer::with_text("t", "ab\ncde\n");

        assert_eq!(buffer.line_text(0), "ab");
        assert_eq!(buffer.line_text(1), "cde");
        assert_eq!(buffer.line_text(2), "");
    }

    #[test]
    fn column_is_char_offset_from_line_start() {
        let buffer = Buffer::with_text("t", "ab\ncde");

        assert_eq!(buffer.column(1), 1);
        assert_eq!(buffer.column(3), 0);
        assert_eq!(buffer.column(5), 2);
    }

    #[test]
    fn chars_iterate_away_from_position_in_both_directions() {
        let buffer = Buffer::with_text("t", "abcd");

        assert_eq!(buffer.chars_after(1).collect::<String>(), "bcd");
        assert_eq!(buffer.chars_before(3).collect::<String>(), "cba");
    }

    #[test]
    fn text_in_copies_a_range_without_changing_the_buffer() {
        let buffer = Buffer::with_text("t", "hello world");

        assert_eq!(buffer.text_in(6..11), "world");
        assert_eq!(buffer.text_in(6..99), "world");
        assert_eq!(buffer.contents(), "hello world");
    }

    #[test]
    fn new_buffers_have_no_mark_or_selection() {
        let buffer = Buffer::with_text("t", "abc");

        assert_eq!(buffer.mark(), None);
        assert!(!buffer.mark_active());
        assert_eq!(buffer.selection(), None);
    }

    #[test]
    fn active_mark_selects_between_mark_and_point() {
        let mut buffer = Buffer::with_text("t", "hello world");
        buffer.set_point(6);

        buffer.set_mark(2);
        assert_eq!(buffer.selection(), Some(2..6));

        buffer.set_point(0);
        assert_eq!(buffer.selection(), Some(0..2));
    }

    #[test]
    fn deactivated_mark_is_kept_but_selects_nothing() {
        let mut buffer = Buffer::with_text("t", "hello");
        buffer.set_mark(2);

        buffer.deactivate_mark();

        assert_eq!(buffer.mark(), Some(2));
        assert!(!buffer.mark_active());
        assert_eq!(buffer.selection(), None);
    }

    #[test]
    fn mark_is_clamped_to_the_buffer() {
        let mut buffer = Buffer::with_text("t", "abc");

        buffer.set_mark(10);

        assert_eq!(buffer.mark(), Some(3));
    }

    #[test]
    fn mark_follows_insertions_before_it_but_not_at_it() {
        let mut buffer = Buffer::with_text("t", "abcd");
        buffer.set_mark(2);

        buffer.insert("xy");
        assert_eq!(buffer.mark(), Some(4));

        buffer.set_point(4);
        buffer.insert("z");
        assert_eq!(buffer.mark(), Some(4));
    }

    #[test]
    fn mark_follows_deletions() {
        let mut buffer = Buffer::with_text("t", "abcdef");
        buffer.set_mark(4);

        buffer.delete_range(0..2);
        assert_eq!(buffer.mark(), Some(2));

        buffer.delete_range(1..3);
        assert_eq!(buffer.mark(), Some(1));
    }

    #[test]
    fn exchange_point_and_mark_swaps_and_activates() {
        let mut buffer = Buffer::with_text("t", "abcd");
        buffer.set_mark(1);
        buffer.deactivate_mark();
        buffer.set_point(3);

        assert!(buffer.exchange_point_and_mark());

        assert_eq!(buffer.point(), 1);
        assert_eq!(buffer.mark(), Some(3));
        assert!(buffer.mark_active());
    }

    #[test]
    fn exchange_point_and_mark_needs_a_mark() {
        let mut buffer = Buffer::with_text("t", "abcd");
        buffer.set_point(2);

        assert!(!buffer.exchange_point_and_mark());
        assert_eq!(buffer.point(), 2);
    }
}
