use std::ops::{Deref, DerefMut, Range};
use std::path::PathBuf;

use jiff::civil::{Date, Time};
use oxmacs_core::command::ScopeId;
use oxmacs_core::display::runs;
use oxmacs_core::editor::Editor;
use oxmacs_core::text::{Buffer, BufferId};
use oxmacs_core::theme::Highlight;

use crate::LANGUAGE;
use crate::model::edit::{self, Cycled};
use crate::model::note::{self, Note, State, Style};
use crate::model::vault::{self, Index};
use crate::settings::{self, AtlasSettings, Vault};

#[derive(Debug)]
pub(crate) struct AtlasState {
    pub(crate) list_scope: ScopeId,
    pub(crate) calendar_scope: ScopeId,
    pub(crate) today: fn() -> Date,
    pub(crate) time_now: fn() -> Time,
    notes: Vec<(BufferId, u64, Note)>,
    line: Option<(BufferId, usize)>,
    pub(crate) index: Option<Index>,
    vaults: Vec<Vault>,
}

impl AtlasState {
    pub(crate) fn new(list_scope: ScopeId, calendar_scope: ScopeId) -> Self {
        Self {
            list_scope,
            calendar_scope,
            today: || jiff::Zoned::now().date(),
            time_now: || jiff::Zoned::now().time(),
            notes: Vec::new(),
            line: None,
            index: None,
            vaults: Vec::new(),
        }
    }
}

pub(crate) struct Notebook<'a> {
    editor: &'a mut Editor,
    pub(crate) atlas: AtlasState,
}

impl Deref for Notebook<'_> {
    type Target = Editor;

    fn deref(&self) -> &Editor {
        self.editor
    }
}

impl DerefMut for Notebook<'_> {
    fn deref_mut(&mut self) -> &mut Editor {
        self.editor
    }
}

pub(crate) fn notebook<'a, R>(
    editor: &'a mut Editor,
    work: impl FnOnce(&mut Notebook<'a>) -> R,
) -> Option<R> {
    let atlas = editor.take_applet_state::<AtlasState>()?;
    let mut book = Notebook { editor, atlas };
    let result = work(&mut book);
    book.editor.set_applet_state(book.atlas);
    Some(result)
}

pub(crate) fn settings(editor: &Editor) -> AtlasSettings {
    editor.applet_config(settings::TABLE)
}

pub(crate) fn vault_list(editor: &Editor) -> Vec<(String, PathBuf)> {
    settings(editor)
        .vault
        .iter()
        .map(|vault| (vault.name.clone(), vault.directory(editor.home())))
        .collect()
}

pub(crate) fn today_in(editor: &Editor) -> Date {
    editor
        .applet_state::<AtlasState>()
        .map_or_else(|| jiff::Zoned::now().date(), |state| (state.today)())
}

pub(crate) fn face(style: Style) -> Highlight {
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

pub(crate) fn cycled_message(cycled: &Cycled) -> Option<String> {
    match cycled.repeated {
        Some(Some(next)) => Some(format!("Next: {next}")),
        Some(None) => Some("Logged; the task has no date to move".to_owned()),
        None => None,
    }
}

pub(crate) fn is_note(editor: &Editor, id: BufferId) -> bool {
    editor.buffer_language(id) == Some(LANGUAGE)
}

pub(crate) fn highlight(
    editor: &Editor,
    id: BufferId,
    line: usize,
) -> Vec<(Range<usize>, Highlight)> {
    let Some(buffer) = editor.buffer_by_id(id) else {
        return Vec::new();
    };
    let Some(spans) = editor
        .applet_state::<AtlasState>()
        .and_then(|state| state.notes.iter().find(|(known, ..)| *known == id))
        .and_then(|(_, _, note)| note.spans.get(line))
    else {
        return Vec::new();
    };
    line_faces(spans, buffer.line_end(line) - buffer.line_start(line))
}

pub(crate) fn capture_style(editor: &Editor, text: &str) -> Vec<(Range<usize>, Highlight)> {
    let note = note::parse(text, today_in(editor));
    note.spans
        .first()
        .map_or_else(Vec::new, |spans| line_faces(spans, text.chars().count()))
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

impl Notebook<'_> {
    pub(crate) fn today(&self) -> Date {
        (self.atlas.today)()
    }

    pub(crate) fn vaults(&self) -> Vec<(String, PathBuf)> {
        vault_list(self)
    }

    pub(crate) fn index(&mut self) -> &Index {
        let today = self.today();
        let vaults = self.vaults();
        self.atlas
            .index
            .get_or_insert_with(|| Index::build(&vaults, today))
    }

    pub(crate) fn configured(&mut self) {
        let vaults = settings(self).vault;
        if vaults != self.atlas.vaults {
            self.atlas.index = None;
            self.atlas.vaults = vaults;
        }
    }

    pub(crate) fn note_saved(&mut self, path: &std::path::Path, text: &str) {
        let today = self.today();
        let Some(vault) = self
            .vaults()
            .into_iter()
            .find(|(_, directory)| path.starts_with(directory))
            .map(|(name, _)| name)
        else {
            return;
        };
        if let Some(index) = &mut self.atlas.index {
            index.update(vault::page(&vault, path, text, today));
        }
    }

    pub(crate) fn replace_line(&mut self, id: BufferId, line: usize, text: &str) {
        let Some(buffer) = self.buffer_by_id_mut(id) else {
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

    fn exact_dates_on(&mut self, id: BufferId, line: usize) {
        let today = self.today();
        let Some(buffer) = self.buffer_by_id(id) else {
            return;
        };
        if line >= buffer.len_lines() {
            return;
        }
        let text = buffer.line_text(line).to_string();
        if let Some(text) = edit::exact(&text, today) {
            self.replace_line(id, line, &text);
        }
    }

    pub(crate) fn exact_dates(&mut self, id: BufferId) {
        if !is_note(self, id) {
            return;
        }
        let lines = self.buffer_by_id(id).map_or(0, Buffer::len_lines);
        for line in 0..lines {
            self.exact_dates_on(id, line);
        }
    }

    pub(crate) fn save(&mut self, id: BufferId) -> bool {
        self.exact_dates(id);
        let saved = self.save_buffer(id);
        if saved {
            self.saved(id);
        }
        saved
    }

    pub(crate) fn saved(&mut self, id: BufferId) {
        if !is_note(self, id) {
            return;
        }
        let Some((path, text)) = self.buffer_by_id(id).and_then(|buffer| {
            buffer
                .file()
                .map(|file| (file.path.clone(), buffer.contents()))
        }) else {
            return;
        };
        self.note_saved(&path, &text);
    }

    pub(crate) fn after_command(&mut self) {
        let id = self.buffer().id();
        let here = is_note(self, id).then(|| {
            let buffer = self.buffer();
            (id, buffer.line_of(buffer.point()))
        });
        let left = std::mem::replace(&mut self.atlas.line, here);
        if let Some((id, line)) = left
            && left != here
            && is_note(self, id)
        {
            self.exact_dates_on(id, line);
        }
    }

    pub(crate) fn cycle_task(&mut self) {
        let buffer = self.buffer();
        let id = buffer.id();
        let line = buffer.line_of(buffer.point());
        let text = buffer.line_text(line).to_string();
        let Some(cycled) = edit::cycle(&text, self.today()) else {
            self.show_message("No task on this line");
            return;
        };
        self.replace_line(id, line, &cycled.text);
        if let Some(message) = cycled_message(&cycled) {
            self.show_message(message);
        }
    }

    pub(crate) fn sync_shown(&mut self) {
        self.refit_lists();
        let current = self.buffer().id();
        for id in self.buffer_ids() {
            if is_note(self, id) && (id == current || self.buffer_shown(id)) {
                self.sync(id);
            }
        }
    }

    fn sync(&mut self, id: BufferId) {
        let Some(buffer) = self.buffer_by_id(id) else {
            return;
        };
        let revision = buffer.revision();
        let fresh = self
            .atlas
            .notes
            .iter()
            .any(|(known, seen, _)| *known == id && *seen == revision);
        if fresh {
            return;
        }
        let parsed = note::parse(&buffer.contents(), self.today());
        let open = self.buffer_ids();
        self.atlas
            .notes
            .retain(|(known, ..)| *known != id && open.contains(known));
        self.atlas.notes.push((id, revision, parsed));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::editor::Editor;
    use oxmacs_core::theme::Highlight;

    use crate::testing::{lines, press, type_text};

    fn note(text: &str) -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Garden plan.atlas");
        fs::write(&path, text).unwrap();
        let mut editor = crate::testing::editor();
        editor.open_file(&path);
        editor.prepare_display();
        (dir, editor)
    }

    fn first_line(editor: &Editor) -> String {
        lines(editor)[0].clone()
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
            lines(&editor)[..3],
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
        type_text(&mut editor, "[ ] Call Ana  due:fri on:tomorrow");
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

        type_text(&mut editor, "= ");
        editor.prepare_display();

        assert_eq!(editor.line_style(0).spans, [(0..7, Highlight::Heading)]);
    }

    #[test]
    fn atlas_notes_stay_plain_text_without_the_applet() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Garden plan.atlas");
        fs::write(&path, "= Plan\n").unwrap();
        let mut editor = Editor::new();

        editor.open_file(&path);
        editor.prepare_display();

        assert_eq!(editor.language(), None);
        assert_eq!(editor.line_style(0).spans, []);
    }
}
