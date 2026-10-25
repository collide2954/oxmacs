use std::borrow::Cow;
use std::mem;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use ropey::Rope;

use crate::files::editorconfig::{self, Style};
use crate::files::{DiskState, Format};
use crate::text::history::{Edit, History};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub path: PathBuf,
    pub format: Format,
    pub disk: Option<DiskState>,
    pub changed_on_disk: bool,
    pub style: Style,
}

impl FileInfo {
    pub fn new(path: PathBuf, format: Format, disk: Option<DiskState>) -> Self {
        Self {
            path,
            format,
            disk,
            changed_on_disk: false,
            style: Style::default(),
        }
    }

    pub fn styled(mut self) -> Self {
        self.style = editorconfig::style_for(&self.path);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextChange {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub start: (usize, usize),
    pub old_end: (usize, usize),
    pub new_end: (usize, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Marker(usize);

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Splice {
    pub start: usize,
    pub removed: usize,
    pub inserted: usize,
}

#[derive(Debug, Clone)]
pub struct Buffer {
    id: BufferId,
    name: String,
    text: Rope,
    revision: u64,
    changes: Option<Vec<TextChange>>,
    splices: Option<Vec<Splice>>,
    point: usize,
    mark: Option<usize>,
    mark_active: bool,
    markers: Vec<Option<usize>>,
    history: History,
    saved: u64,
    file: Option<FileInfo>,
    read_only: bool,
}

impl Buffer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: BufferId(NEXT_ID.fetch_add(1, Ordering::Relaxed)),
            name: name.into(),
            text: Rope::new(),
            revision: 0,
            changes: None,
            splices: None,
            point: 0,
            mark: None,
            mark_active: false,
            markers: Vec::new(),
            history: History::default(),
            saved: 0,
            file: None,
            read_only: false,
        }
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub fn id(&self) -> BufferId {
        self.id
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    pub fn is_modified(&self) -> bool {
        self.history.current() != self.saved
    }

    pub fn mark_saved(&mut self) {
        self.history.close();
        self.saved = self.history.current();
    }

    pub fn file(&self) -> Option<&FileInfo> {
        self.file.as_ref()
    }

    pub fn file_mut(&mut self) -> Option<&mut FileInfo> {
        self.file.as_mut()
    }

    pub fn set_file(&mut self, file: FileInfo) {
        self.file = Some(file);
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

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn track_changes(&mut self) {
        self.changes.get_or_insert_with(Vec::new);
    }

    pub fn take_changes(&mut self) -> Vec<TextChange> {
        self.changes.as_mut().map(mem::take).unwrap_or_default()
    }

    pub fn text_in_bytes(&self, range: Range<usize>) -> String {
        let end = range.end.min(self.text.len_bytes());
        let start = range.start.min(end);
        self.text.byte_slice(start..end).to_string()
    }

    pub fn chunk_at_byte(&self, byte: usize) -> &str {
        let byte = byte.min(self.text.len_bytes());
        let (chunk, start, _, _) = self.text.chunk_at_byte(byte);
        &chunk[byte - start..]
    }

    fn byte_position(&self, byte: usize) -> (usize, usize) {
        let row = self.text.byte_to_line(byte);
        (row, byte - self.text.line_to_byte(row))
    }

    pub fn track_splices(&mut self) {
        self.splices.get_or_insert_with(Vec::new);
    }

    pub fn take_splices(&mut self) -> Vec<Splice> {
        self.splices.as_mut().map(mem::take).unwrap_or_default()
    }

    fn splice_made(&mut self, start: usize, removed: usize, inserted: usize) {
        if let Some(splices) = &mut self.splices {
            splices.push(Splice {
                start,
                removed,
                inserted,
            });
        }
    }

    fn record(&mut self, change: TextChange) {
        if let Some(changes) = &mut self.changes {
            changes.push(change);
        }
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

    pub fn char_to_byte(&self, position: usize) -> usize {
        self.text.char_to_byte(position.min(self.len_chars()))
    }

    pub fn byte_to_char(&self, byte: usize) -> usize {
        self.text.byte_to_char(byte.min(self.text.len_bytes()))
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

    pub fn add_marker(&mut self, position: usize) -> Marker {
        let position = Some(position.min(self.len_chars()));
        let slot = self
            .markers
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                self.markers.push(None);
                self.markers.len() - 1
            });
        self.markers[slot] = position;
        Marker(slot)
    }

    pub fn marker(&self, marker: Marker) -> Option<usize> {
        self.markers.get(marker.0).copied().flatten()
    }

    pub fn set_marker(&mut self, marker: Marker, position: usize) {
        let position = position.min(self.len_chars());
        if let Some(slot) = self.markers.get_mut(marker.0) {
            *slot = Some(position);
        }
    }

    pub fn remove_marker(&mut self, marker: Marker) {
        if let Some(slot) = self.markers.get_mut(marker.0) {
            *slot = None;
        }
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
        if text.is_empty() {
            return;
        }
        let at = self.point;
        self.apply_insert(at, text);
        self.point = at + text.chars().count();
        self.history.record(
            Edit::Insert {
                at,
                text: text.to_owned(),
            },
            at,
            self.point,
        );
    }

    pub fn insert_at(&mut self, at: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        let at = at.min(self.len_chars());
        let before = self.point;
        self.apply_insert(at, text);
        self.history.record(
            Edit::Insert {
                at,
                text: text.to_owned(),
            },
            before,
            self.point,
        );
    }

    pub fn insert_unrecorded(&mut self, at: usize, text: &str) {
        if !text.is_empty() {
            self.apply_insert(at.min(self.len_chars()), text);
            self.forget_history();
        }
    }

    pub fn delete_unrecorded(&mut self, range: Range<usize>) {
        let end = range.end.min(self.len_chars());
        let start = range.start.min(end);
        if start < end {
            self.apply_delete(start, end);
            self.forget_history();
        }
    }

    fn forget_history(&mut self) {
        self.history = History::default();
        self.saved = self.history.current();
    }

    pub fn close_undo_step(&mut self) {
        self.history.close();
    }

    pub fn undo(&mut self) -> bool {
        let Some(step) = self.history.take_undo() else {
            return false;
        };
        for edit in step.edits.iter().rev() {
            match edit {
                Edit::Insert { at, text } => {
                    self.apply_delete(*at, at + text.chars().count());
                }
                Edit::Delete { at, text } => self.apply_insert(*at, text),
            }
        }
        self.finish_history_move(step.point_before);
        self.history.push_undone(step);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(step) = self.history.take_redo() else {
            return false;
        };
        for edit in &step.edits {
            match edit {
                Edit::Insert { at, text } => self.apply_insert(*at, text),
                Edit::Delete { at, text } => {
                    self.apply_delete(*at, at + text.chars().count());
                }
            }
        }
        self.finish_history_move(step.point_after);
        self.history.push_done(step);
        true
    }

    fn finish_history_move(&mut self, point: usize) {
        self.point = point.min(self.len_chars());
        self.mark_active = false;
    }

    fn apply_insert(&mut self, at: usize, text: &str) {
        let inserted = text.chars().count();
        let start_byte = self.text.char_to_byte(at);
        let start = self.byte_position(start_byte);
        self.text.insert(at, text);
        self.revision += 1;
        self.splice_made(at, 0, inserted);
        let new_end_byte = start_byte + text.len();
        let new_end = self.byte_position(new_end_byte);
        self.record(TextChange {
            start_byte,
            old_end_byte: start_byte,
            new_end_byte,
            start,
            old_end: start,
            new_end,
        });
        self.point = position_after_insert(self.point, at, inserted);
        self.mark = self
            .mark
            .map(|mark| position_after_insert(mark, at, inserted));
        for marker in self.markers.iter_mut().flatten() {
            *marker = position_after_insert(*marker, at, inserted);
        }
    }

    fn apply_delete(&mut self, start: usize, end: usize) -> String {
        let removed = self.text_in(start..end);
        let start_byte = self.text.char_to_byte(start);
        let old_end_byte = self.text.char_to_byte(end);
        let from = self.byte_position(start_byte);
        let old_end = self.byte_position(old_end_byte);
        self.text.remove(start..end);
        self.revision += 1;
        self.splice_made(start, end - start, 0);
        self.record(TextChange {
            start_byte,
            old_end_byte,
            new_end_byte: start_byte,
            start: from,
            old_end,
            new_end: from,
        });
        self.point = position_after_removal(self.point, start, end);
        self.mark = self
            .mark
            .map(|mark| position_after_removal(mark, start, end));
        for marker in self.markers.iter_mut().flatten() {
            *marker = position_after_removal(*marker, start, end);
        }
        removed
    }

    pub fn splice(&mut self, range: Range<usize>, text: &str) {
        let end = range.end.min(self.len_chars());
        let start = range.start.min(end);
        if start < end {
            self.apply_delete(start, end);
        }
        if !text.is_empty() {
            self.apply_insert(start, text);
        }
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

    pub fn set_text(&mut self, text: &str) {
        let removed = self.len_chars();
        self.text = Rope::from_str(text);
        self.revision += 1;
        let inserted = self.len_chars();
        self.splice_made(0, removed, inserted);
        if let Some(changes) = &mut self.changes {
            changes.clear();
        }
        self.history = History::default();
        self.saved = self.history.current();
        let len = self.len_chars();
        self.point = self.point.min(len);
        self.mark = self.mark.map(|mark| mark.min(len));
        for marker in self.markers.iter_mut().flatten() {
            *marker = (*marker).min(len);
        }
    }

    pub fn replace_text(&mut self, text: &str) {
        let old = self.contents();
        let prefix = old
            .chars()
            .zip(text.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let old_len = self.len_chars();
        let new_len = text.chars().count();
        let suffix = old
            .chars()
            .rev()
            .zip(text.chars().rev())
            .take_while(|(a, b)| a == b)
            .count()
            .min(old_len.min(new_len) - prefix);
        self.history.close();
        self.delete_range(prefix..old_len - suffix);
        let middle: String = text
            .chars()
            .skip(prefix)
            .take(new_len - prefix - suffix)
            .collect();
        if !middle.is_empty() {
            let before = self.point;
            self.apply_insert(prefix, &middle);
            self.history.record(
                Edit::Insert {
                    at: prefix,
                    text: middle,
                },
                before,
                self.point,
            );
        }
        self.history.close();
    }

    pub fn delete_range(&mut self, range: Range<usize>) -> String {
        let end = range.end.min(self.len_chars());
        let start = range.start.min(end);
        if start == end {
            return String::new();
        }
        let before = self.point;
        let removed = self.apply_delete(start, end);
        self.history.record(
            Edit::Delete {
                at: start,
                text: removed.clone(),
            },
            before,
            self.point,
        );
        removed
    }
}

fn position_after_insert(position: usize, at: usize, inserted: usize) -> usize {
    if position > at {
        position + inserted
    } else {
        position
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
    fn unrecorded_edits_shift_markers_and_cannot_be_undone() {
        let mut buffer = Buffer::with_text("t", "log\n> ");
        let input = buffer.add_marker(6);
        buffer.set_point(6);
        buffer.insert("hi");

        buffer.insert_unrecorded(4, "more\n");
        buffer.delete_unrecorded(0..4);

        assert_eq!(buffer.contents(), "more\n> hi");
        assert_eq!(buffer.marker(input), Some(7));
        assert_eq!(buffer.point(), 9);
        assert!(!buffer.undo());
        assert!(!buffer.is_modified());
    }

    #[test]
    fn replacing_text_changes_only_the_differing_middle() {
        let mut buffer = Buffer::with_text("t", "one two three");
        buffer.set_point(12);

        buffer.replace_text("one 2 three");

        assert_eq!(buffer.contents(), "one 2 three");
        assert_eq!(buffer.point(), 10);
    }

    #[test]
    fn point_inside_the_replaced_part_moves_to_its_start() {
        let mut buffer = Buffer::with_text("t", "abcdef");
        buffer.set_point(3);

        buffer.replace_text("abXYZef");

        assert_eq!(buffer.point(), 2);
    }

    #[test]
    fn a_replacement_is_one_undo_step() {
        let mut buffer = Buffer::with_text("t", "abc");
        buffer.insert("x");

        buffer.replace_text("xyz");
        assert!(buffer.undo());

        assert_eq!(buffer.contents(), "xabc");
    }

    #[test]
    fn replacing_with_the_same_text_records_nothing() {
        let mut buffer = Buffer::with_text("t", "abc");

        buffer.replace_text("abc");

        assert!(!buffer.is_modified());
        assert!(!buffer.undo());
    }

    #[test]
    fn setting_text_replaces_it_without_history() {
        let mut buffer = Buffer::with_text("t", "hello world");
        buffer.insert("x");
        buffer.set_point(10);
        buffer.set_mark(11);

        buffer.set_text("short");

        assert_eq!(buffer.contents(), "short");
        assert_eq!(buffer.point(), 5);
        assert_eq!(buffer.mark(), Some(5));
        assert!(!buffer.is_modified());
        assert!(!buffer.undo());
    }

    #[test]
    fn every_buffer_has_its_own_id() {
        let first = Buffer::new("t");
        let second = Buffer::with_text("t", "x");

        assert_ne!(first.id(), second.id());
    }

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
    fn inserting_elsewhere_shifts_point_only_when_it_follows() {
        let mut buffer = Buffer::with_text("t", "abcd");
        buffer.set_point(2);

        buffer.insert_at(3, "x");
        assert_eq!(buffer.point(), 2);

        buffer.insert_at(0, "yy");
        assert_eq!(buffer.point(), 4);
        assert_eq!(buffer.contents(), "yyabcxd");

        buffer.undo();
        assert_eq!(buffer.contents(), "abcd");
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
    fn tracked_buffers_report_changes_in_bytes_and_rows() {
        let mut buffer = Buffer::with_text("t", "ab\né");
        buffer.insert("x");
        assert_eq!(buffer.take_changes(), []);

        buffer.track_changes();
        buffer.set_point(5);
        buffer.insert("y\nz");
        buffer.delete_range(0..2);

        assert_eq!(
            buffer.take_changes(),
            [
                TextChange {
                    start_byte: 6,
                    old_end_byte: 6,
                    new_end_byte: 9,
                    start: (1, 2),
                    old_end: (1, 2),
                    new_end: (2, 1),
                },
                TextChange {
                    start_byte: 0,
                    old_end_byte: 2,
                    new_end_byte: 0,
                    start: (0, 0),
                    old_end: (0, 2),
                    new_end: (0, 0),
                }
            ]
        );
        assert_eq!(buffer.take_changes(), []);
    }

    #[test]
    fn chunks_start_at_the_requested_byte() {
        let buffer = Buffer::with_text("t", "héllo");

        assert_eq!(buffer.chunk_at_byte(3), "llo");
        assert_eq!(buffer.chunk_at_byte(6), "");
        assert_eq!(buffer.chunk_at_byte(99), "");
    }

    #[test]
    fn every_text_change_bumps_the_revision() {
        let mut buffer = Buffer::with_text("t", "abc");
        let mut revisions = vec![buffer.revision()];

        buffer.insert("x");
        revisions.push(buffer.revision());
        buffer.delete_backward(1);
        revisions.push(buffer.revision());
        buffer.undo();
        revisions.push(buffer.revision());
        buffer.set_text("new");
        revisions.push(buffer.revision());
        buffer.set_point(1);
        buffer.set_mark(0);
        revisions.push(buffer.revision());

        assert!(revisions[..5].is_sorted_by(|a, b| a < b), "{revisions:?}");
        assert_eq!(revisions[4], revisions[5]);
    }

    #[test]
    fn positions_convert_between_chars_and_bytes() {
        let buffer = Buffer::with_text("t", "hé→x");

        assert_eq!(buffer.char_to_byte(2), 3);
        assert_eq!(buffer.char_to_byte(99), 7);
        assert_eq!(buffer.byte_to_char(6), 3);
        assert_eq!(buffer.byte_to_char(99), 4);
    }

    #[test]
    fn chars_iterate_away_from_position_in_both_directions() {
        let buffer = Buffer::with_text("t", "abcd");

        assert_eq!(buffer.chars_after(1).collect::<String>(), "bcd");
        assert_eq!(buffer.chars_before(3).collect::<String>(), "cba");
    }

    #[test]
    fn edits_mark_a_buffer_modified_until_saved() {
        let mut buffer = Buffer::with_text("t", "abc");
        assert!(!buffer.is_modified());

        buffer.insert("x");
        assert!(buffer.is_modified());

        buffer.mark_saved();
        assert!(!buffer.is_modified());
    }

    #[test]
    fn undoing_back_to_the_saved_state_is_unmodified() {
        let mut buffer = Buffer::new("t");
        buffer.insert("a");
        buffer.mark_saved();

        buffer.insert("b");
        assert!(buffer.is_modified());

        buffer.undo();
        assert_eq!(buffer.contents(), "a");
        assert!(!buffer.is_modified());

        buffer.undo();
        assert!(buffer.is_modified());

        buffer.redo();
        assert!(!buffer.is_modified());
    }

    #[test]
    fn buffers_remember_their_file() {
        let mut buffer = Buffer::new("a.txt");
        assert_eq!(buffer.file(), None);

        let info = FileInfo::new("dir/a.txt".into(), Format::default(), None);
        buffer.set_file(info.clone());

        assert_eq!(buffer.file(), Some(&info));
    }

    #[test]
    fn undo_reverts_an_insert_and_restores_point() {
        let mut buffer = Buffer::with_text("t", "ab");
        buffer.set_point(1);
        buffer.insert("X");
        buffer.close_undo_step();

        assert!(buffer.undo());

        assert_eq!(buffer.contents(), "ab");
        assert_eq!(buffer.point(), 1);
    }

    #[test]
    fn undo_reverts_a_deletion() {
        let mut buffer = Buffer::with_text("t", "abc");
        buffer.set_point(1);
        buffer.delete_forward(1);

        assert!(buffer.undo());

        assert_eq!(buffer.contents(), "abc");
        assert_eq!(buffer.point(), 1);
    }

    #[test]
    fn redo_reapplies_an_undone_step() {
        let mut buffer = Buffer::with_text("t", "ab");
        buffer.set_point(1);
        buffer.insert("X");
        buffer.undo();

        assert!(buffer.redo());

        assert_eq!(buffer.contents(), "aXb");
        assert_eq!(buffer.point(), 2);
    }

    #[test]
    fn new_edits_drop_the_redo_branch() {
        let mut buffer = Buffer::with_text("t", "");
        buffer.insert("a");
        buffer.undo();
        buffer.insert("b");

        assert!(!buffer.redo());
        assert_eq!(buffer.contents(), "b");
    }

    #[test]
    fn edits_without_a_step_boundary_undo_together() {
        let mut buffer = Buffer::new("t");
        buffer.insert("a");
        buffer.insert("b");

        buffer.undo();

        assert_eq!(buffer.contents(), "");
    }

    #[test]
    fn closed_steps_undo_one_at_a_time() {
        let mut buffer = Buffer::new("t");
        buffer.insert("a");
        buffer.close_undo_step();
        buffer.insert("b");

        buffer.undo();
        assert_eq!(buffer.contents(), "a");
        buffer.undo();
        assert_eq!(buffer.contents(), "");
        assert!(!buffer.undo());
    }

    #[test]
    fn undo_and_redo_are_inert_without_history() {
        let mut buffer = Buffer::with_text("t", "abc");

        assert!(!buffer.undo());
        assert!(!buffer.redo());
        assert_eq!(buffer.contents(), "abc");
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
    fn markers_follow_edits_around_them() {
        let mut buffer = Buffer::with_text("t", "abcdef");
        let before = buffer.add_marker(1);
        let after = buffer.add_marker(4);

        buffer.set_point(2);
        buffer.insert("xy");
        assert_eq!(buffer.marker(before), Some(1));
        assert_eq!(buffer.marker(after), Some(6));

        buffer.delete_range(0..3);
        assert_eq!(buffer.marker(before), Some(0));
        assert_eq!(buffer.marker(after), Some(3));

        buffer.set_text("z");
        assert_eq!(buffer.marker(after), Some(1));
    }

    #[test]
    fn removed_markers_free_their_slot() {
        let mut buffer = Buffer::with_text("t", "abc");
        let first = buffer.add_marker(1);
        buffer.remove_marker(first);

        assert_eq!(buffer.marker(first), None);
        let second = buffer.add_marker(2);
        assert_eq!(second, first);
        assert_eq!(buffer.marker(second), Some(2));
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
    #[test]
    fn splicing_replaces_text_outside_the_undo_history() {
        let mut buffer = Buffer::with_text("t", "abc|input");
        let marker = buffer.add_marker(4);
        buffer.set_point(9);

        buffer.splice(1..3, "XYZ");

        assert_eq!(buffer.contents(), "aXYZ|input");
        assert_eq!(buffer.marker(marker), Some(5));
        assert_eq!(buffer.point(), 10);
        assert!(!buffer.undo());
    }
}
