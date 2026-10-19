use std::ops::Range;

use jiff::civil::Date;

use crate::atlas::note::{self, Note, State, Style};
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::text::BufferId;
use crate::theme::Highlight;

pub(in crate::editor) const EXTENSION: &str = "atlas";

pub(in crate::editor) fn today() -> Date {
    jiff::Zoned::now().date()
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

fn runs(faces: &[Option<Highlight>]) -> Vec<(Range<usize>, Highlight)> {
    let mut found: Vec<(Range<usize>, Highlight)> = Vec::new();
    for (column, face) in faces.iter().enumerate() {
        let Some(face) = face else {
            continue;
        };
        match found.last_mut() {
            Some((range, last)) if range.end == column && last == face => range.end += 1,
            _ => found.push((column..column + 1, *face)),
        }
    }
    found
}

impl Editor {
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
        let width = buffer.line_end(line) - buffer.line_start(line);
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
}

#[cfg(test)]
mod tests {
    use std::fs;

    use jiff::civil::date;

    use crate::editor::Editor;
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
