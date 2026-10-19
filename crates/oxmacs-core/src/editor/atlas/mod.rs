use std::ops::Range;

use jiff::civil::Date;

use crate::atlas::edit::{self, Cycled};
use crate::atlas::note::{self, Note, State, Style};
use crate::display::runs;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::prompts::minibuffer::Purpose;
use crate::text::{Buffer, BufferId};
use crate::theme::Highlight;

mod agenda;
mod calendar;
mod links;
pub(in crate::editor) mod list;
mod vaults;

pub(in crate::editor) const EXTENSION: &str = "atlas";

pub(in crate::editor) fn today() -> Date {
    jiff::Zoned::now().date()
}

pub(in crate::editor) fn time_now() -> jiff::civil::Time {
    jiff::Zoned::now().time()
}

fn face(style: Style) -> Highlight {
    match style {
        Style::Key | Style::Bullet => Highlight::Keyword,
        Style::Separator
        | Style::Finished
        | Style::Anchor
        | Style::Comment
        | Style::Box(State::Dropped) => Highlight::Comment,
        Style::Heading(_) => Highlight::Heading,
        Style::Box(State::Todo) => Highlight::Function,
        Style::Box(State::Doing) | Style::Loose => Highlight::Warning,
        Style::Box(State::Waiting) | Style::Field => Highlight::Constant,
        Style::Box(State::Done) => Highlight::Success,
        Style::Priority => Highlight::Error,
        Style::Tag => Highlight::Type,
        Style::Link => Highlight::Link,
        Style::When => Highlight::Attribute,
    }
}

pub(in crate::editor) fn cycled_message(cycled: &Cycled) -> Option<String> {
    match cycled.repeated {
        Some(Some(next)) => Some(format!("Next: {next}")),
        Some(None) => Some("Logged; the task has no date to move".to_owned()),
        None => None,
    }
}

impl Editor {
    fn replace_line(&mut self, index: usize, line: usize, text: &str) {
        let Some(buffer) = self.buffers.get_mut(index) else {
            return;
        };
        let start = buffer.line_start(line);
        let end = buffer.line_end(line);
        let point = buffer.point();
        buffer.delete_range(start..end);
        buffer.insert_at(start, text);
        if (start..=end).contains(&point) {
            let column = point - start;
            buffer.set_point(start + column.min(text.chars().count()));
        }
    }

    fn exact_dates_on(&mut self, index: usize, line: usize) {
        let today = (self.today)();
        let Some(buffer) = self.buffers.get(index) else {
            return;
        };
        if line >= buffer.len_lines() {
            return;
        }
        let text = buffer.line_text(line).to_string();
        if let Some(text) = edit::exact(&text, today) {
            self.replace_line(index, line, &text);
        }
    }

    pub(in crate::editor) fn exact_atlas_dates(&mut self, index: usize) {
        if !self.is_atlas(index) {
            return;
        }
        let lines = self.buffers.get(index).map_or(0, Buffer::len_lines);
        for line in 0..lines {
            self.exact_dates_on(index, line);
        }
    }

    pub(in crate::editor) fn after_atlas_command(&mut self) {
        let here = self.is_atlas(0).then(|| {
            let buffer = self.buffers.current();
            (buffer.id(), buffer.line_of(buffer.point()))
        });
        let left = std::mem::replace(&mut self.atlas_line, here);
        if let Some((id, line)) = left
            && left != here
            && let Some(index) = self.buffers.position(|buffer| buffer.id() == id)
            && self.is_atlas(index)
        {
            self.exact_dates_on(index, line);
        }
    }

    pub(in crate::editor) fn atlas_cycle_task(&mut self) {
        let buffer = self.buffers.current();
        let line = buffer.line_of(buffer.point());
        let text = buffer.line_text(line).to_string();
        let Some(cycled) = edit::cycle(&text, (self.today)()) else {
            self.show_message("No task on this line");
            return;
        };
        self.replace_line(0, line, &cycled.text);
        if let Some(message) = cycled_message(&cycled) {
            self.show_message(message);
        }
    }

    pub(in crate::editor) fn is_atlas(&self, index: usize) -> bool {
        matches!(self.buffers.mode(index), Some(Mode::Text(None)))
            && self
                .buffers
                .get(index)
                .and_then(|buffer| buffer.file())
                .and_then(|file| file.path.extension())
                .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION))
    }

    pub(in crate::editor) fn sync_atlas(&mut self, index: usize) {
        if !self.is_atlas(index) {
            return;
        }
        let Some(buffer) = self.buffers.get(index) else {
            return;
        };
        let (id, revision) = (buffer.id(), buffer.revision());
        let fresh = self
            .atlas_notes
            .iter()
            .any(|(known, seen, _)| *known == id && *seen == revision);
        if fresh {
            return;
        }
        let parsed = note::parse(&buffer.contents(), (self.today)());
        let buffers = &self.buffers;
        self.atlas_notes.retain(|(known, ..)| {
            *known != id && buffers.position(|buffer| buffer.id() == *known).is_some()
        });
        self.atlas_notes.push((id, revision, parsed));
    }

    pub(in crate::editor) fn atlas_note(&self, id: BufferId) -> Option<&Note> {
        self.atlas_notes
            .iter()
            .find(|(known, ..)| *known == id)
            .map(|(_, _, note)| note)
    }

    pub(in crate::editor) fn atlas_highlights(
        &self,
        index: usize,
        line: usize,
    ) -> Vec<(Range<usize>, Highlight)> {
        let Some(buffer) = self.buffers.get(index) else {
            return Vec::new();
        };
        let Some(spans) = self
            .atlas_note(buffer.id())
            .and_then(|note| note.spans.get(line))
        else {
            return Vec::new();
        };
        line_faces(spans, buffer.line_end(line) - buffer.line_start(line))
    }

    pub fn minibuffer_highlights(&self) -> Vec<(Range<usize>, Highlight)> {
        let Some(minibuffer) = &self.minibuffer else {
            return Vec::new();
        };
        if minibuffer.purpose != Purpose::AtlasCapture {
            return Vec::new();
        }
        let text = minibuffer.input().contents();
        let note = note::parse(&text, (self.today)());
        note.spans
            .first()
            .map_or_else(Vec::new, |spans| line_faces(spans, text.chars().count()))
    }
}

fn line_faces(spans: &[note::Span], width: usize) -> Vec<(Range<usize>, Highlight)> {
    let mut faces = vec![None; width];
    let (finished, others): (Vec<_>, Vec<_>) =
        spans.iter().partition(|span| span.style == Style::Finished);
    for span in others.into_iter().chain(finished) {
        let end = span.columns.end.min(width);
        let start = span.columns.start.min(end);
        faces[start..end].fill(Some(face(span.style)));
    }
    runs(&faces)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use jiff::civil::date;

    use crate::editor::Editor;
    use crate::editor::test_support::press;
    use crate::theme::Highlight;

    fn note(text: &str) -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Garden plan.atlas");
        fs::write(&path, text).unwrap();
        let mut editor = Editor::new();
        editor.today = || date(2026, 10, 19);
        editor.open_file(&path);
        editor.prepare_display();
        (dir, editor)
    }

    #[test]
    fn atlas_notes_are_highlighted_from_their_parse() {
        let (_dir, editor) = note("title: Garden\n---\n= Plan\n[ ] Buy seeds  due:fri  #home\n");

        assert_eq!(editor.line_style(0).spans, [(0..6, Highlight::Keyword)]);
        assert_eq!(editor.line_style(2).spans, [(0..6, Highlight::Heading)]);
        assert_eq!(
            editor.line_style(3).spans,
            [
                (0..3, Highlight::Function),
                (15..22, Highlight::Warning),
                (24..29, Highlight::Type),
            ]
        );
    }

    #[test]
    fn finished_tasks_are_dimmed_after_their_box() {
        let (_dir, editor) = note("[x] Measure beds  done:2026-10-18\n");

        assert_eq!(
            editor.line_style(0).spans,
            [(0..3, Highlight::Success), (3..33, Highlight::Comment)]
        );
    }

    fn first_line(editor: &Editor) -> String {
        crate::editor::test_support::lines(editor)[0].clone()
    }

    #[test]
    fn c_c_c_c_cycles_a_task_and_stamps_when_it_was_done() {
        let (_dir, mut editor) = note("[ ] Buy seeds  #home\n");

        press(&mut editor, "C-c C-c");
        assert_eq!(first_line(&editor), "[/] Buy seeds  #home");
        press(&mut editor, "C-c C-c");
        assert_eq!(first_line(&editor), "[x] Buy seeds  #home  done:2026-10-19");
        press(&mut editor, "C-c C-c");
        assert_eq!(first_line(&editor), "[ ] Buy seeds  #home");
    }

    #[test]
    fn finishing_a_repeating_task_moves_it_on_and_logs_it() {
        let (_dir, mut editor) = note("  [/] Water plants  on:2026-10-21  every:3d\nnext\n");

        press(&mut editor, "C-c C-c");

        assert_eq!(
            crate::editor::test_support::lines(&editor)[..3],
            [
                "  [ ] Water plants  on:2026-10-24  every:3d",
                "      - done 2026-10-19",
                "next"
            ]
        );
        assert_eq!(editor.message(), Some("Next: 2026-10-24"));
    }

    #[test]
    fn c_c_c_c_off_a_task_says_so() {
        let (_dir, mut editor) = note("Just text\n");

        press(&mut editor, "C-c C-c");

        assert_eq!(editor.message(), Some("No task on this line"));
    }

    #[test]
    fn loose_dates_become_exact_when_point_leaves_the_line() {
        let (_dir, mut editor) = note("");
        crate::editor::test_support::type_text(&mut editor, "[ ] Call Ana  due:fri on:tomorrow");
        assert_eq!(first_line(&editor), "[ ] Call Ana  due:fri on:tomorrow");

        press(&mut editor, "RET");

        assert_eq!(
            first_line(&editor),
            "[ ] Call Ana  due:2026-10-23 on:2026-10-20"
        );
    }

    #[test]
    fn saving_makes_every_loose_date_exact() {
        let (dir, mut editor) = note("[ ] One  due:+1w\n[ ] Two  due:someday\n");
        press(&mut editor, "C-e SPC");

        press(&mut editor, "C-x C-s");

        assert_eq!(
            fs::read_to_string(dir.path().join("Garden plan.atlas")).unwrap(),
            "[ ] One  due:2026-10-26 \n[ ] Two  due:someday\n"
        );
    }

    #[test]
    fn the_mode_line_names_atlas_and_edits_reparse() {
        let (_dir, mut editor) = note("plain\n");
        assert!(
            editor.mode_line().contains("Atlas"),
            "{}",
            editor.mode_line()
        );

        crate::editor::test_support::type_text(&mut editor, "= ");
        editor.prepare_display();

        assert_eq!(editor.line_style(0).spans, [(0..7, Highlight::Heading)]);
    }
}
