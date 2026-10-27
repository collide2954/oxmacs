use std::mem;
use std::ops::Range;
use std::time::{Duration, Instant};

use crate::editor::Editor;
use crate::external::diagnostics::Severity;
use crate::text::{Buffer, BufferId, Marker};
use crate::theme::Highlight;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Look {
    Shade(Highlight),
    Wave(Severity),
    Note(String, Highlight),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lasts {
    Edit,
    Fade(Instant, Duration),
    Cleared,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::editor) enum Owner {
    Note,
    Hint,
    Lint(usize),
    Script(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineDecorations {
    pub shades: Vec<(Range<usize>, Highlight, f32)>,
    pub waves: Vec<(Range<usize>, Severity)>,
    pub note: Option<(String, Highlight)>,
}

#[derive(Debug)]
pub(in crate::editor) struct Decoration {
    pub(in crate::editor) buffer: BufferId,
    start: Marker,
    end: Marker,
    look: Look,
    lasts: Lasts,
    revision: u64,
    pub(in crate::editor) owner: Owner,
}

fn fade(elapsed: Duration, duration: Duration) -> f32 {
    if elapsed >= duration {
        return 0.0;
    }
    let progress = elapsed.as_secs_f32() / duration.as_secs_f32();
    1.0 - progress * progress
}

impl Decoration {
    fn fading(&self, now: Instant) -> bool {
        matches!(self.lasts, Lasts::Fade(began, duration) if now.saturating_duration_since(began) < duration)
    }

    fn strength(&self, buffer: &Buffer, now: Instant) -> Option<f32> {
        match self.lasts {
            Lasts::Edit => (buffer.revision() == self.revision).then_some(1.0),
            Lasts::Fade(began, duration) => {
                let elapsed = now.saturating_duration_since(began);
                (elapsed < duration).then(|| fade(elapsed, duration))
            }
            Lasts::Cleared => Some(1.0),
        }
    }

    pub(in crate::editor) fn span(&self, buffer: &Buffer) -> Option<Range<usize>> {
        let start = buffer.marker(self.start)?;
        Some(start..buffer.marker(self.end)?.max(start))
    }
}

impl Editor {
    pub(in crate::editor) fn decorate(
        &mut self,
        buffer: BufferId,
        range: Range<usize>,
        look: Look,
        lasts: Lasts,
        owner: Owner,
    ) {
        let now = Instant::now();
        self.undecorate(|decoration, home| {
            home.is_none_or(|home| decoration.strength(home, now).is_none())
        });
        let Some(target) = self.buffer_by_id_mut(buffer) else {
            return;
        };
        let last = target.len_chars();
        let (start, end) = (range.start.min(last), range.end.min(last));
        let revision = target.revision();
        let (start, end) = (
            target.add_marker(start.min(end)),
            target.add_marker(start.max(end)),
        );
        self.decorations.push(Decoration {
            buffer,
            start,
            end,
            look,
            lasts,
            revision,
            owner,
        });
    }

    pub(in crate::editor) fn undecorate(
        &mut self,
        gone: impl Fn(&Decoration, Option<&Buffer>) -> bool,
    ) {
        let (dropped, kept): (Vec<Decoration>, Vec<Decoration>) = mem::take(&mut self.decorations)
            .into_iter()
            .partition(|decoration| gone(decoration, self.buffer_by_id(decoration.buffer)));
        for decoration in dropped {
            if let Some(buffer) = self.buffer_by_id_mut(decoration.buffer) {
                buffer.remove_marker(decoration.start);
                buffer.remove_marker(decoration.end);
            }
        }
        self.decorations = kept;
    }

    pub(in crate::editor) fn decorations_of<'a>(
        &'a self,
        buffer: &'a Buffer,
    ) -> impl Iterator<Item = (&'a Decoration, Range<usize>)> {
        self.decorations
            .iter()
            .filter(move |decoration| decoration.buffer == buffer.id())
            .filter_map(move |decoration| Some((decoration, decoration.span(buffer)?)))
    }

    pub fn flashing(&self, now: Instant) -> bool {
        self.decorations
            .iter()
            .any(|decoration| decoration.fading(now))
    }

    pub(in crate::editor) fn flashing_in(&self, index: usize, now: Instant) -> bool {
        self.buffers.get(index).is_some_and(|buffer| {
            self.decorations
                .iter()
                .any(|decoration| decoration.buffer == buffer.id() && decoration.fading(now))
        })
    }

    pub(in crate::editor) fn decorations_on_line(
        &self,
        index: usize,
        line: usize,
        now: Instant,
    ) -> LineDecorations {
        let mut found = LineDecorations::default();
        let Some(buffer) = self.buffers.get(index) else {
            return found;
        };
        let (first, last) = (buffer.line_start(line), buffer.line_end(line));
        let mut hint = None;
        for (decoration, span) in self.decorations_of(buffer) {
            let Some(strength) = decoration.strength(buffer, now) else {
                continue;
            };
            match &decoration.look {
                Look::Shade(face) => {
                    let (from, to) = (span.start.max(first), span.end.min(last));
                    if from < to {
                        found
                            .shades
                            .push((from - first..to - first, *face, strength));
                    }
                }
                Look::Wave(severity) => {
                    if span.start <= last && span.end.saturating_sub(1).max(span.start) >= first {
                        let from = span.start.max(first) - first;
                        let to = span.end.min(last) - first;
                        found.waves.push((from..to.max(from + 1), *severity));
                    }
                }
                Look::Note(text, face) => {
                    if (first..=last).contains(&span.start) {
                        let note = Some((text.clone(), *face));
                        if decoration.owner == Owner::Hint {
                            hint = hint.or(note);
                        } else if found.note.is_none() {
                            found.note = note;
                        }
                    }
                }
            }
        }
        found.note = found.note.or(hint);
        found
    }

    pub fn annotate(
        &mut self,
        buffer: BufferId,
        range: Range<usize>,
        look: Look,
        lasts: Lasts,
        key: &str,
    ) {
        let Some(last) = self.buffer_by_id(buffer).map(Buffer::len_chars) else {
            return;
        };
        let (start, end) = (range.start.min(last), range.end.min(last));
        let range = start.min(end)..start.max(end);
        self.undecorate(|decoration, home| {
            decoration.buffer == buffer
                && matches!(&decoration.owner, Owner::Script(known) if known == key)
                && mem::discriminant(&decoration.look) == mem::discriminant(&look)
                && home.and_then(|home| decoration.span(home)) == Some(range.clone())
        });
        self.decorate(buffer, range, look, lasts, Owner::Script(key.to_owned()));
    }

    pub fn clear_annotations(&mut self, buffer: BufferId, key: Option<&str>) {
        self.undecorate(|decoration, _| {
            decoration.buffer == buffer
                && matches!(&decoration.owner, Owner::Script(known) if key.is_none_or(|key| known == key))
        });
    }
    pub fn set_inline_note(
        &mut self,
        buffer: BufferId,
        position: usize,
        text: &str,
        face: Highlight,
    ) {
        let Some(target) = self.buffer_by_id(buffer) else {
            return;
        };
        let position = position.min(target.len_chars());
        let line = target.line_of(position);
        let span = target.line_start(line)..=target.line_end(line);
        self.undecorate(|decoration, home| {
            decoration.owner == Owner::Note
                && decoration.buffer == buffer
                && home
                    .and_then(|home| decoration.span(home))
                    .is_some_and(|at| span.contains(&at.start))
        });
        self.decorate(
            buffer,
            position..position,
            Look::Note(text.to_owned(), face),
            Lasts::Edit,
            Owner::Note,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Lasts, Look, fade};
    use crate::editor::{Editor, Severity};
    use crate::testing::editor_with;
    use crate::theme::Highlight;

    const HALF: Duration = Duration::from_millis(500);
    const MS: Duration = Duration::from_millis(1);

    fn flash(editor: &mut Editor, range: std::ops::Range<usize>, face: Highlight, now: Instant) {
        let id = editor.buffer().id();
        editor.annotate(id, range, Look::Shade(face), Lasts::Fade(now, HALF), "zap");
    }

    fn flashed(text: &str, range: std::ops::Range<usize>) -> (Editor, Instant) {
        let mut editor = editor_with(text, 0);
        let now = Instant::now();
        flash(&mut editor, range, Highlight::Region, now);
        (editor, now)
    }

    fn shades(
        editor: &Editor,
        line: usize,
        now: Instant,
    ) -> Vec<(std::ops::Range<usize>, Highlight, f32)> {
        editor.pane().line_decorations(line, now).shades
    }

    fn note(editor: &Editor, line: usize) -> Option<(String, Highlight)> {
        editor.pane().line_decorations(line, Instant::now()).note
    }

    #[test]
    fn a_flash_lights_its_characters_then_fades_out() {
        let (editor, now) = flashed("(+ 1 2)\n(* 3 4)\n", 0..7);

        assert_eq!(shades(&editor, 0, now), [(0..7, Highlight::Region, 1.0)]);
        assert_eq!(shades(&editor, 1, now), []);
        let [(_, _, halfway)] = shades(&editor, 0, now + MS * 250)[..] else {
            panic!("the flash should still show");
        };
        assert!(halfway > 0.0 && halfway < 1.0, "{halfway}");
        assert!(editor.flashing(now + MS * 499));
        assert_eq!(shades(&editor, 0, now + HALF), []);
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

        assert_eq!(shades(&editor, 0, now), [(3..6, Highlight::Region, 1.0)]);
        assert_eq!(shades(&editor, 1, now), []);
        assert_eq!(shades(&editor, 2, now), [(0..4, Highlight::Region, 1.0)]);
    }

    #[test]
    fn flashes_move_with_the_text_around_them() {
        let (mut editor, now) = flashed("abc def", 4..7);

        editor.buffer_mut().insert_at(0, "xy");

        assert_eq!(shades(&editor, 0, now), [(6..9, Highlight::Region, 1.0)]);
    }

    #[test]
    fn annotating_the_same_characters_again_replaces_the_annotation() {
        let (mut editor, now) = flashed("abc def", 4..7);

        flash(&mut editor, 4..7, Highlight::Removed, now + MS * 100);
        flash(&mut editor, 0..3, Highlight::Region, now + MS * 100);

        assert_eq!(
            shades(&editor, 0, now + MS * 100),
            [
                (4..7, Highlight::Removed, 1.0),
                (0..3, Highlight::Region, 1.0)
            ]
        );
    }

    #[test]
    fn shades_stay_inside_the_buffer_and_skip_empty_ranges() {
        let (mut editor, now) = flashed("abc", 1..99);
        flash(&mut editor, 2..2, Highlight::Removed, now);

        assert_eq!(shades(&editor, 0, now), [(1..3, Highlight::Region, 1.0)]);
    }

    #[test]
    fn flashes_show_in_every_window_of_their_buffer() {
        let (mut editor, now) = flashed("abc", 0..3);
        crate::testing::press(&mut editor, "C-x 2");
        let windows = editor.tiling().windows();

        for window in windows {
            assert_eq!(
                editor
                    .pane_of(window)
                    .unwrap()
                    .line_decorations(0, now)
                    .shades,
                [(0..3, Highlight::Region, 1.0)]
            );
        }
    }

    #[test]
    fn annotations_are_cleared_by_key_or_all_together() {
        let mut editor = editor_with("abc def\nghi\n", 0);
        let id = editor.buffer().id();
        editor.annotate(
            id,
            0..3,
            Look::Shade(Highlight::Region),
            Lasts::Cleared,
            "a",
        );
        editor.annotate(id, 4..7, Look::Wave(Severity::Error), Lasts::Cleared, "b");
        editor.annotate(
            id,
            11..11,
            Look::Note("=> 1".to_owned(), Highlight::Comment),
            Lasts::Cleared,
            "b",
        );
        let now = Instant::now();
        let line = |editor: &Editor, line| editor.pane().line_decorations(line, now);

        assert_eq!(line(&editor, 0).shades, [(0..3, Highlight::Region, 1.0)]);
        assert_eq!(line(&editor, 0).waves, [(4..7, Severity::Error)]);
        assert_eq!(
            line(&editor, 1).note,
            Some(("=> 1".to_owned(), Highlight::Comment))
        );

        editor.clear_annotations(id, Some("b"));
        assert_eq!(line(&editor, 0).waves, []);
        assert_eq!(line(&editor, 1).note, None);
        assert_eq!(line(&editor, 0).shades.len(), 1);

        editor.clear_annotations(id, None);
        assert_eq!(line(&editor, 0), super::LineDecorations::default());
    }

    #[test]
    fn annotations_last_until_the_next_edit_when_asked() {
        let mut editor = editor_with("abc", 0);
        let id = editor.buffer().id();
        editor.annotate(id, 0..3, Look::Shade(Highlight::Region), Lasts::Edit, "a");
        let now = Instant::now();

        assert_eq!(editor.pane().line_decorations(0, now).shades.len(), 1);
        editor.buffer_mut().insert_at(3, "d");
        assert_eq!(editor.pane().line_decorations(0, now).shades, []);
    }

    #[test]
    fn inline_notes_sit_after_their_line_until_the_buffer_changes() {
        let mut editor = Editor::new();
        editor.buffer_mut().insert_at(0, "(+ 1 2)\n(* 3 4)\n");
        let id = editor.buffer().id();
        let shown = |text: &str, face| Some((text.to_owned(), face));

        editor.set_inline_note(id, 7, "=> 3", Highlight::Comment);
        editor.set_inline_note(id, 15, "=> 12", Highlight::Comment);
        assert_eq!(note(&editor, 0), shown("=> 3", Highlight::Comment));
        assert_eq!(note(&editor, 1), shown("=> 12", Highlight::Comment));

        editor.set_inline_note(id, 3, "!! broke", Highlight::Error);
        assert_eq!(note(&editor, 0), shown("!! broke", Highlight::Error));
        assert_eq!(note(&editor, 1), shown("=> 12", Highlight::Comment));

        editor.buffer_mut().insert_at(0, " ");
        assert_eq!(note(&editor, 0), None);
        assert_eq!(note(&editor, 1), None);
    }

    #[test]
    fn killing_a_buffer_drops_its_decorations() {
        let (mut editor, _) = flashed("abc", 0..3);
        let id = editor.buffer().id();
        editor.set_inline_note(id, 0, "=> 1", Highlight::Comment);
        assert_eq!(editor.decorations.len(), 2);

        crate::testing::press(&mut editor, "C-x k RET");

        assert!(editor.buffer_by_id(id).is_none());
        assert!(editor.decorations.is_empty());
    }
}
