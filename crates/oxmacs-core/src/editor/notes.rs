use crate::editor::Editor;
use crate::text::{BufferId, Marker};
use crate::theme::Highlight;

#[derive(Debug, Clone)]
pub(in crate::editor) struct Note {
    buffer: BufferId,
    at: Marker,
    revision: u64,
    text: String,
    face: Highlight,
}

impl Editor {
    pub fn set_inline_note(
        &mut self,
        buffer: BufferId,
        position: usize,
        text: &str,
        face: Highlight,
    ) {
        let Some(index) = self.buffers.position(|found| found.id() == buffer) else {
            return;
        };
        let Some(target) = self.buffers.get_mut(index) else {
            return;
        };
        let position = position.min(target.len_chars());
        let line = target.line_of(position);
        let revision = target.revision();
        let (dropped, kept): (Vec<Note>, Vec<Note>) = std::mem::take(&mut self.notes)
            .into_iter()
            .partition(|note| {
                note.buffer == buffer
                    && (note.revision != revision
                        || target.marker(note.at).map(|at| target.line_of(at)) == Some(line))
            });
        for note in dropped {
            target.remove_marker(note.at);
        }
        self.notes = kept;
        let at = target.add_marker(position);
        self.notes.push(Note {
            buffer,
            at,
            revision,
            text: text.to_owned(),
            face,
        });
    }

    pub(in crate::editor) fn note_on_line(
        &self,
        index: usize,
        line: usize,
    ) -> Option<(String, Highlight)> {
        let buffer = self.buffers.get(index)?;
        self.notes
            .iter()
            .filter(|note| note.buffer == buffer.id() && note.revision == buffer.revision())
            .find(|note| {
                buffer
                    .marker(note.at)
                    .is_some_and(|at| buffer.line_of(at) == line)
            })
            .map(|note| (note.text.clone(), note.face))
            .or_else(|| {
                self.hint_on_line(buffer.id(), buffer.revision(), line)
                    .map(|hint| (hint, Highlight::Comment))
            })
    }
}
#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::theme::Highlight;

    #[test]
    fn inline_notes_sit_after_their_line_until_the_buffer_changes() {
        let mut editor = Editor::new();
        editor.buffer_mut().insert_at(0, "(+ 1 2)\n(* 3 4)\n");
        let id = editor.buffer().id();
        let note = |text: &str, face| Some((text.to_owned(), face));

        editor.set_inline_note(id, 7, "=> 3", Highlight::Comment);
        editor.set_inline_note(id, 15, "=> 12", Highlight::Comment);
        assert_eq!(editor.pane().line_note(0), note("=> 3", Highlight::Comment));
        assert_eq!(
            editor.pane().line_note(1),
            note("=> 12", Highlight::Comment)
        );

        editor.set_inline_note(id, 3, "!! broke", Highlight::Error);
        assert_eq!(
            editor.pane().line_note(0),
            note("!! broke", Highlight::Error)
        );
        assert_eq!(
            editor.pane().line_note(1),
            note("=> 12", Highlight::Comment)
        );

        editor.buffer_mut().insert_at(0, " ");
        assert_eq!(editor.pane().line_note(0), None);
        assert_eq!(editor.pane().line_note(1), None);
    }
}
