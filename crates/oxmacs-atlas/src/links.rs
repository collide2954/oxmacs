use std::fs;
use std::path::PathBuf;

use oxmacs_core::editor::{Page, clip};
use oxmacs_core::theme::Highlight;

use crate::lists::ListKind;
use crate::model::note::{self, Style};
use crate::model::vault;
use crate::notes::{Notebook, is_note};
use crate::vaults::note_template;

const HELP: &str = "RET visits · g refreshes · q quits";
const ROOM: usize = 80;
const LABEL: usize = 8;

fn excerpt(line: &str, wanted: &str, room: usize) -> String {
    let length = line.chars().count();
    let lower = line.to_lowercase();
    let at = lower
        .find(wanted)
        .map_or(0, |byte| lower[..byte].chars().count());
    let end = at + wanted.chars().count();
    if length <= room || end < room {
        return clip(line, room);
    }
    let start = (end + 2).saturating_sub(room).min(at);
    let rest: String = line.chars().skip(start).collect();
    format!("…{}", clip(rest.trim_start(), room.saturating_sub(1)))
}

impl Notebook<'_> {
    fn link_at_point(&self) -> Option<String> {
        let buffer = self.buffer();
        if !is_note(self, buffer.id()) {
            return None;
        }
        let line = buffer.line_of(buffer.point());
        let column = buffer.point() - buffer.line_start(line);
        let text = buffer.line_text(line).to_string();
        let parsed = note::parse(&text, self.today());
        let span = parsed.spans.first()?.iter().find(|span| {
            span.style == Style::Link && span.columns.start <= column && column < span.columns.end
        })?;
        let inside: String = text
            .chars()
            .skip(span.columns.start + 1)
            .take(span.columns.len() - 2)
            .collect();
        Some(inside.trim().to_owned())
    }

    fn current_vault_directory(&self) -> Option<PathBuf> {
        let here = self.buffer().file().map(|file| file.path.clone());
        let vaults = self.vaults();
        here.and_then(|path| {
            vaults
                .iter()
                .find(|(_, directory)| path.starts_with(directory))
                .map(|(_, directory)| directory.clone())
        })
        .or_else(|| vaults.first().map(|(_, directory)| directory.clone()))
    }

    pub(crate) fn follow_link(&mut self) {
        let Some(target) = self.link_at_point() else {
            self.show_message("No link here");
            return;
        };
        let origin = self.current_location();
        let known = self.index().find(&target).map(|page| page.path.clone());
        let Some(path) = known.or_else(|| self.start_note(&target)) else {
            return;
        };
        if self.open_file(&path) {
            self.remember_location(origin);
        }
    }

    fn start_note(&mut self, title: &str) -> Option<PathBuf> {
        let Some(directory) = self.current_vault_directory() else {
            self.show_message("No Atlas vault to keep the new note in");
            return None;
        };
        let path = vault::page_path(&directory, title);
        let template = note_template(title);
        if let Err(error) = fs::write(&path, &template) {
            self.show_message(format!("{}: {error}", path.display()));
            return None;
        }
        self.note_saved(&path, &template);
        self.show_message(format!("Started {title}"));
        Some(path)
    }

    pub(crate) fn backlinks(&mut self) {
        let buffer = self.buffer();
        let title = buffer.file().and_then(|file| {
            is_note(self, buffer.id()).then(|| {
                vault::title_of(&file.path, &note::parse(&buffer.contents(), self.today()))
            })
        });
        let Some(title) = title else {
            self.show_message("Backlinks are for Atlas notes");
            return;
        };
        self.show_list(&ListKind::Backlinks(title));
    }

    pub(crate) fn render_backlinks(&mut self) {
        let Some(id) = self.list_id(&ListKind::Backlinks(String::new())) else {
            return;
        };
        let Some(ListKind::Backlinks(subject)) = self.list_kind(id) else {
            return;
        };
        let linking: Vec<(String, PathBuf)> = self
            .index()
            .backlinks(&subject)
            .iter()
            .map(|page| (page.title.clone(), page.path.clone()))
            .collect();
        let room = self.buffer_width(id).unwrap_or(ROOM).saturating_sub(LABEL);
        let mut page = Page::default();
        page.line(&[
            ("Backlinks to ", Some(Highlight::Heading)),
            (&subject, Some(Highlight::Heading)),
        ]);
        page.line(&[(HELP, Some(Highlight::Comment))]);
        let mut rows = Vec::new();
        if linking.is_empty() {
            page.line(&[]);
            page.line(&[("Nothing links here yet", Some(Highlight::Comment))]);
        }
        let wanted = format!("[{}]", subject.to_lowercase());
        for (title, path) in linking {
            page.line(&[]);
            page.line(&[(&title, Some(Highlight::Link))]);
            let text = fs::read_to_string(&path).unwrap_or_default();
            for (number, line) in text.lines().enumerate() {
                if !line.to_lowercase().contains(&wanted) {
                    continue;
                }
                rows.push((page.line_count(), path.clone(), number));
                let label = format!("  {:>4}  ", number + 1);
                page.line(&[
                    (&label, Some(Highlight::Comment)),
                    (&excerpt(line.trim(), &wanted, room), None),
                ]);
            }
        }
        self.fill_list(id, page, rows);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::editor::Editor;

    use crate::testing::{lines, press, vault_config, write};

    fn vault() -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            &root.join("Garden plan.atlas"),
            "title: Garden plan\n---\nsee [Seed catalog] and [Ana]\n",
        );
        write(
            &root.join("Seed catalog.atlas"),
            "= Seeds\nback to [garden plan]\n",
        );
        write(
            &root.join("journal").join("2026-10-19.atlas"),
            "title: 2026-10-19\n---\nmet about the [Garden plan]\nnothing here\n",
        );
        let mut editor = crate::testing::configured(&vault_config(&[("personal", root)]));
        editor.open_file(&root.join("Garden plan.atlas"));
        (dir, editor)
    }

    #[test]
    fn c_c_c_o_follows_a_link_and_m_comma_returns() {
        let (_dir, mut editor) = vault();
        press(&mut editor, "C-n C-n M-f M-f");

        press(&mut editor, "C-c C-o");
        assert_eq!(editor.buffer().name(), "Seed catalog.atlas");

        press(&mut editor, "M-,");
        assert_eq!(editor.buffer().name(), "Garden plan.atlas");
    }

    #[test]
    fn links_to_missing_notes_start_them_in_the_same_vault() {
        let (dir, mut editor) = vault();
        press(&mut editor, "C-n C-n C-e C-b C-b");

        press(&mut editor, "C-c C-o");

        assert_eq!(editor.buffer().name(), "Ana.atlas");
        assert_eq!(
            fs::read_to_string(dir.path().join("Ana.atlas")).unwrap(),
            "title: Ana\n---\n= Ana\n\n"
        );
        assert_eq!(editor.message(), Some("Started Ana"));
    }

    #[test]
    fn c_c_c_o_off_a_link_says_so() {
        let (_dir, mut editor) = vault();

        press(&mut editor, "C-c C-o");

        assert_eq!(editor.message(), Some("No link here"));
    }

    #[test]
    fn backlinks_list_the_lines_that_link_here() {
        let (_dir, mut editor) = vault();

        press(&mut editor, "C-c n b");

        assert_eq!(editor.buffer().name(), "*Backlinks*");
        let shown = lines(&editor);
        assert_eq!(shown[0], "Backlinks to Garden plan");
        assert!(
            shown.contains(&"     3  met about the [Garden plan]".to_owned()),
            "{shown:?}"
        );
        assert!(
            shown.contains(&"     2  back to [garden plan]".to_owned()),
            "{shown:?}"
        );
        assert!(!shown.iter().any(|line| line.contains("nothing here")));
    }

    #[test]
    fn backlinks_fit_the_window_and_keep_the_link_in_view() {
        let (dir, mut editor) = vault();
        write(
            &dir.path().join("Long.atlas"),
            "title: Long\n---\nshort start [Garden plan] then a great deal more text after it\nplenty of words written well before we finally mention [Garden plan]\naaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbb [Garden plan]\n",
        );
        press(&mut editor, "C-c n b");
        let window = editor.selected_window();

        editor.resize_pane(window, 30, 40);

        let shown = lines(&editor);
        assert!(
            shown.iter().all(|line| line.chars().count() <= 39),
            "{shown:?}"
        );
        assert!(
            shown.contains(&"     3  short start [Garden plan] then…".to_owned()),
            "{shown:?}"
        );
        assert!(
            shown.contains(&"     4  …finally mention [Garden plan]".to_owned()),
            "{shown:?}"
        );
        assert!(
            shown.contains(&"     5  …bbbbbbbbbbbbbb [Garden plan]".to_owned()),
            "{shown:?}"
        );
    }

    #[test]
    fn return_on_a_backlink_visits_its_line() {
        let (_dir, mut editor) = vault();
        press(&mut editor, "C-c n b");
        let row = lines(&editor)
            .iter()
            .position(|line| line.contains("met about"))
            .unwrap();
        press(&mut editor, "M-<");
        for _ in 0..row {
            press(&mut editor, "C-n");
        }

        press(&mut editor, "RET");

        assert_eq!(editor.buffer().name(), "2026-10-19.atlas");
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 2);
    }
}
