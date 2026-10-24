use std::mem;
use std::ops::Range;
use std::time::{Duration, Instant};

use crate::editor::Editor;
use crate::text::{Buffer, BufferId, Marker};
use crate::theme::Highlight;

#[derive(Debug)]
pub(in crate::editor) struct Flash {
    buffer: BufferId,
    start: Marker,
    end: Marker,
    face: Highlight,
    began: Instant,
    duration: Duration,
}

fn fade(elapsed: Duration, duration: Duration) -> f32 {
    if elapsed >= duration {
        return 0.0;
    }
    let progress = elapsed.as_secs_f32() / duration.as_secs_f32();
    1.0 - progress * progress
}

fn closeness(range: &Range<usize>, point: usize) -> (bool, usize) {
    if range.end <= point {
        (false, point - range.end)
    } else if range.start < point {
        (false, 0)
    } else {
        (true, range.start - point)
    }
}

fn located(text: &str, code: &str, point: usize) -> Vec<Range<usize>> {
    let wanted = code.trim();
    if wanted.is_empty() {
        return Vec::new();
    }
    let length = wanted.chars().count();
    let mut counted = (0, 0);
    let exact = text.match_indices(wanted).map(|(at, _)| {
        counted = (at, counted.1 + text[counted.0..at].chars().count());
        counted.1..counted.1 + length
    });
    match exact.min_by_key(|range| closeness(range, point)) {
        Some(found) => vec![found],
        None => by_lines(text, code, point),
    }
}

fn blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn trimmed((start, line): (usize, &str)) -> Range<usize> {
    let lead = line.chars().take_while(|c| c.is_whitespace()).count();
    start + lead..start + line.trim_end().chars().count()
}

fn by_lines(text: &str, code: &str, point: usize) -> Vec<Range<usize>> {
    let wanted: Vec<&str> = code
        .lines()
        .map(str::trim_end)
        .filter(|line| !blank(line))
        .collect();
    let Some((first, rest)) = wanted.split_first().filter(|(_, rest)| !rest.is_empty()) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    let mut start = 0;
    for line in text.split('\n') {
        lines.push((start, line));
        start += line.chars().count() + 1;
    }
    let same = |at: usize, line: &str| lines[at].1.trim_end() == line;
    let follow = |from: usize| {
        let mut found = vec![from];
        let mut at = from + 1;
        for line in rest {
            at += lines[at..]
                .iter()
                .position(|(_, text)| text.trim_end() == *line)?;
            found.push(at);
            at += 1;
        }
        Some(found)
    };
    let span = |found: &Vec<usize>| {
        let (Some(first), Some(last)) = (found.first(), found.last()) else {
            return 0..0;
        };
        trimmed(lines[*first]).start..trimmed(lines[*last]).end
    };
    (0..lines.len())
        .filter(|at| same(*at, first))
        .filter_map(follow)
        .min_by_key(|found| closeness(&span(found), point))
        .map(|found| runs(&lines, &found))
        .unwrap_or_default()
}

fn runs(lines: &[(usize, &str)], found: &[usize]) -> Vec<Range<usize>> {
    let mut runs: Vec<Range<usize>> = Vec::new();
    let mut previous: Option<usize> = None;
    for &at in found {
        let range = trimmed(lines[at]);
        let joined = previous
            .is_some_and(|before| lines[before + 1..at].iter().all(|(_, line)| blank(line)));
        match runs.last_mut() {
            Some(run) if joined => run.end = range.end,
            _ => runs.push(range),
        }
        previous = Some(at);
    }
    runs
}

impl Flash {
    fn strength(&self, now: Instant) -> Option<f32> {
        let elapsed = now.saturating_duration_since(self.began);
        (elapsed < self.duration).then(|| fade(elapsed, self.duration))
    }
}

impl Editor {
    pub fn flash(
        &mut self,
        buffer: BufferId,
        range: Range<usize>,
        face: Highlight,
        duration: Duration,
        now: Instant,
    ) {
        let Some(last) = self.buffer_by_id(buffer).map(Buffer::len_chars) else {
            return;
        };
        let range = range.start.min(last)..range.end.min(last);
        self.forget_flashes(|flash, shown| {
            flash.strength(now).is_none()
                || (flash.buffer == buffer && shown.as_ref() == Some(&range))
        });
        if range.is_empty() || duration.is_zero() {
            return;
        }
        let Some(target) = self.buffer_by_id_mut(buffer) else {
            return;
        };
        let (start, end) = (target.add_marker(range.start), target.add_marker(range.end));
        self.flashes.push(Flash {
            buffer,
            start,
            end,
            face,
            began: now,
            duration,
        });
    }

    fn flash_range(&self, flash: &Flash) -> Option<Range<usize>> {
        let buffer = self.buffer_by_id(flash.buffer)?;
        Some(buffer.marker(flash.start)?..buffer.marker(flash.end)?)
    }

    fn forget_flashes(&mut self, gone: impl Fn(&Flash, Option<Range<usize>>) -> bool) {
        let (dropped, kept): (Vec<Flash>, Vec<Flash>) = mem::take(&mut self.flashes)
            .into_iter()
            .partition(|flash| gone(flash, self.flash_range(flash)));
        for flash in dropped {
            if let Some(buffer) = self.buffer_by_id_mut(flash.buffer) {
                buffer.remove_marker(flash.start);
                buffer.remove_marker(flash.end);
            }
        }
        self.flashes = kept;
    }

    pub fn flashing(&self, now: Instant) -> bool {
        self.flashes
            .iter()
            .any(|flash| flash.strength(now).is_some())
    }

    pub(in crate::editor) fn flashing_in(&self, index: usize, now: Instant) -> bool {
        self.buffers.get(index).is_some_and(|buffer| {
            self.flashes
                .iter()
                .any(|flash| flash.buffer == buffer.id() && flash.strength(now).is_some())
        })
    }

    pub fn sent_ranges(&self, buffer: BufferId, code: &str) -> Vec<Range<usize>> {
        self.buffer_by_id(buffer).map_or_else(Vec::new, |buffer| {
            located(&buffer.contents(), code, buffer.point())
        })
    }

    pub(in crate::editor) fn flashes_on_line(
        &self,
        index: usize,
        line: usize,
        now: Instant,
    ) -> Vec<(Range<usize>, Highlight, f32)> {
        let Some(buffer) = self.buffers.get(index) else {
            return Vec::new();
        };
        let (first, last) = (buffer.line_start(line), buffer.line_end(line));
        self.flashes
            .iter()
            .filter(|flash| flash.buffer == buffer.id())
            .filter_map(|flash| {
                let strength = flash.strength(now)?;
                let start = buffer.marker(flash.start)?.max(first);
                let end = buffer.marker(flash.end)?.min(last);
                (start < end).then(|| (start - first..end - first, flash.face, strength))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::fade;
    use crate::editor::Editor;
    use crate::editor::test_support::editor_with;
    use crate::theme::Highlight;

    const HALF: Duration = Duration::from_millis(500);
    const MS: Duration = Duration::from_millis(1);

    fn flashed(text: &str, range: std::ops::Range<usize>) -> (Editor, Instant) {
        let mut editor = editor_with(text, 0);
        let now = Instant::now();
        let id = editor.buffer().id();
        editor.flash(id, range, Highlight::Region, HALF, now);
        (editor, now)
    }

    #[test]
    fn a_flash_lights_its_characters_then_fades_out() {
        let (editor, now) = flashed("(+ 1 2)\n(* 3 4)\n", 0..7);

        assert_eq!(
            editor.pane().line_flashes(0, now),
            [(0..7, Highlight::Region, 1.0)]
        );
        assert_eq!(editor.pane().line_flashes(1, now), []);
        let [(_, _, halfway)] = editor.pane().line_flashes(0, now + MS * 250)[..] else {
            panic!("the flash should still show");
        };
        assert!(halfway > 0.0 && halfway < 1.0, "{halfway}");
        assert!(editor.flashing(now + MS * 499));
        assert_eq!(editor.pane().line_flashes(0, now + HALF), []);
        assert!(!editor.flashing(now + HALF));
    }

    #[test]
    fn fading_starts_slowly_and_ends_at_nothing() {
        assert!((fade(Duration::ZERO, HALF) - 1.0).abs() < f32::EPSILON);
        assert!((fade(MS * 250, HALF) - 0.75).abs() < 1e-6);
        assert!(fade(MS * 400, HALF) < fade(MS * 100, HALF));
        assert!(fade(HALF, HALF).abs() < f32::EPSILON);
        assert!(fade(HALF * 2, HALF).abs() < f32::EPSILON);
        assert!(fade(Duration::ZERO, Duration::ZERO).abs() < f32::EPSILON);
    }

    #[test]
    fn flashes_over_several_lines_light_only_their_characters() {
        let (editor, now) = flashed("abcdef\n\nghijkl\n", 3..12);

        let pane = editor.pane();
        assert_eq!(pane.line_flashes(0, now), [(3..6, Highlight::Region, 1.0)]);
        assert_eq!(pane.line_flashes(1, now), []);
        assert_eq!(pane.line_flashes(2, now), [(0..4, Highlight::Region, 1.0)]);
    }

    #[test]
    fn flashes_move_with_the_text_around_them() {
        let (mut editor, now) = flashed("abc def", 4..7);

        editor.buffer_mut().insert_at(0, "xy");

        assert_eq!(
            editor.pane().line_flashes(0, now),
            [(6..9, Highlight::Region, 1.0)]
        );
    }

    #[test]
    fn flashing_the_same_characters_again_replaces_the_flash() {
        let (mut editor, now) = flashed("abc def", 4..7);
        let id = editor.buffer().id();

        editor.flash(id, 4..7, Highlight::Removed, HALF, now + MS * 100);
        editor.flash(id, 0..3, Highlight::Region, HALF, now + MS * 100);

        assert_eq!(
            editor.pane().line_flashes(0, now + MS * 100),
            [
                (4..7, Highlight::Removed, 1.0),
                (0..3, Highlight::Region, 1.0)
            ]
        );
    }

    #[test]
    fn flashes_stay_inside_the_buffer_and_skip_empty_ranges() {
        let (mut editor, now) = flashed("abc", 1..99);
        let id = editor.buffer().id();
        editor.flash(id, 2..2, Highlight::Removed, HALF, now);

        assert_eq!(
            editor.pane().line_flashes(0, now),
            [(1..3, Highlight::Region, 1.0)]
        );
    }

    fn sent(text: &str, point: usize, code: &str) -> Vec<(usize, usize)> {
        let editor = editor_with(text, point);
        editor
            .sent_ranges(editor.buffer().id(), code)
            .into_iter()
            .map(|range| (range.start, range.end))
            .collect()
    }

    #[test]
    fn sent_code_is_found_where_it_sits() {
        assert_eq!(sent("x <- 1\ny <- 2\n", 7, "x <- 1"), [(0, 6)]);
        assert_eq!(sent("x <- 1\ny <- 2\n", 13, "y <- 2\n"), [(7, 13)]);
        assert_eq!(sent("é <- 1\nf(é)\n", 0, "  f(é)\n\n"), [(7, 11)]);
        assert_eq!(sent("x <- 1\n", 0, "x <- 2"), []);
        assert_eq!(sent("x <- 1\n", 0, " \n"), []);
    }

    #[test]
    fn of_several_copies_the_one_just_before_or_around_point_was_sent() {
        let text = "x <- 1\nx <- 1\nx <- 1\n";

        assert_eq!(sent(text, 7, "x <- 1"), [(0, 6)]);
        assert_eq!(sent(text, 13, "x <- 1"), [(7, 13)]);
        assert_eq!(sent(text, 9, "x <- 1"), [(7, 13)]);
        assert_eq!(sent("a\nx <- 1\nb\nx <- 1\n", 0, "x <- 1"), [(2, 8)]);
    }

    #[test]
    fn code_gathered_from_several_places_lights_each_of_them() {
        let text =
            "```{r}\na <- 1\n\nb <- 2\n```\n\nSome text\n\n```{r}\n#| echo: false\nc <- 3\n```\n";

        assert_eq!(
            sent(text, 0, "a <- 1\n\nb <- 2\nc <- 3\n"),
            [(7, 21), (60, 66)]
        );
        assert_eq!(sent(text, 0, "a <- 1\nd <- 4\n"), []);
    }

    #[test]
    fn flashes_show_in_every_window_of_their_buffer() {
        let (mut editor, now) = flashed("abc", 0..3);
        crate::editor::test_support::press(&mut editor, "C-x 2");
        let windows = editor.tiling().windows();

        for window in windows {
            assert_eq!(
                editor.pane_of(window).unwrap().line_flashes(0, now),
                [(0..3, Highlight::Region, 1.0)]
            );
        }
    }
}
