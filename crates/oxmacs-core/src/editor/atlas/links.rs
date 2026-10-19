use std::fs;
use std::path::PathBuf;

use crate::atlas::note::{self, Style};
use crate::atlas::vault;
use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::atlas::vaults::note_template;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::text::Buffer;
use crate::theme::Highlight;

const BACKLINKS: &str = "*Backlinks*";
const HELP: &str = "RET visits · g refreshes · q quits";

#[derive(Debug)]
pub(in crate::editor) struct AtlasList {
    subject: String,
    rows: Vec<(usize, PathBuf, usize)>,
    styles: Vec<LineStyle>,
}

impl AtlasList {
    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }
}

impl Editor {
    fn link_at_point(&self) -> Option<String> {
        if !self.is_atlas(0) {
            return None;
        }
        let buffer = self.buffers.current();
        let line = buffer.line_of(buffer.point());
        let column = buffer.point() - buffer.line_start(line);
        let text = buffer.line_text(line).to_string();
        let parsed = note::parse(&text, (self.today)());
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
        let here = self.buffers.current().file().map(|file| file.path.clone());
        let vaults = self.vaults();
        here.and_then(|path| {
            vaults
                .iter()
                .find(|(_, directory)| path.starts_with(directory))
                .map(|(_, directory)| directory.clone())
        })
        .or_else(|| vaults.first().map(|(_, directory)| directory.clone()))
    }

    pub(in crate::editor) fn atlas_follow_link(&mut self) {
        let Some(target) = self.link_at_point() else {
            self.show_message("No link here");
            return;
        };
        let origin = self.current_location();
        let known = self
            .atlas_index()
            .find(&target)
            .map(|page| page.path.clone());
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
        self.note_atlas_saved(&path, &template);
        self.show_message(format!("Started {title}"));
        Some(path)
    }

    fn atlas_list_index(&self) -> Option<usize> {
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::AtlasList(_)))
    }

    pub(in crate::editor) fn atlas_backlinks(&mut self) {
        let title = self.buffers.current().file().and_then(|file| {
            let text = self.buffers.current().contents();
            self.is_atlas(0)
                .then(|| vault::title_of(&file.path, &note::parse(&text, (self.today)())))
        });
        let Some(title) = title else {
            self.show_message("Backlinks are for Atlas notes");
            return;
        };
        if self.atlas_list_index().is_none() {
            let mut buffer = Buffer::with_text(BACKLINKS, "");
            buffer.set_read_only(true);
            self.beside(|editor| {
                editor.add_buffer_with(
                    buffer,
                    Mode::AtlasList(Box::new(AtlasList {
                        subject: title.clone(),
                        rows: Vec::new(),
                        styles: Vec::new(),
                    })),
                );
            });
        } else if let Some(index) = self.atlas_list_index()
            && let Some(Mode::AtlasList(list)) = self.buffers.mode_mut(index)
        {
            list.subject.clone_from(&title);
        }
        self.render_backlinks();
        self.show_beside(Self::atlas_list_index);
    }

    pub(in crate::editor) fn render_backlinks(&mut self) {
        let Some(index) = self.atlas_list_index() else {
            return;
        };
        let Some(Mode::AtlasList(list)) = self.buffers.mode(index) else {
            return;
        };
        let subject = list.subject.clone();
        let linking: Vec<(String, PathBuf)> = self
            .atlas_index()
            .backlinks(&subject)
            .iter()
            .map(|page| (page.title.clone(), page.path.clone()))
            .collect();
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
                rows.push((page.styles.len(), path.clone(), number));
                let label = format!("  {:>4}  ", number + 1);
                page.line(&[(&label, Some(Highlight::Comment)), (line.trim(), None)]);
            }
        }
        if let Some((buffer, Mode::AtlasList(list))) = self.buffers.entry_mut(index) {
            page.fill(buffer);
            list.rows = rows;
            list.styles = page.styles;
        }
    }

    pub(in crate::editor) fn atlas_visit(&mut self) {
        let line = self.point_line();
        let target = match self.buffers.mode(0) {
            Some(Mode::AtlasList(list)) => list
                .rows
                .iter()
                .find(|(row, ..)| *row == line)
                .map(|(_, path, target)| (path.clone(), *target)),
            _ => None,
        };
        let Some((path, target)) = target else {
            self.show_message("Nothing to visit on this line");
            return;
        };
        let origin = self.current_location();
        if self.open_file(&path) {
            self.remember_location(origin);
            self.go_to_line(target + 1, 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use jiff::civil::date;

    use crate::config::Config;
    use crate::editor::Editor;
    use crate::editor::test_support::{lines, press};

    fn vault() -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Garden plan.atlas"),
            "title: Garden plan\n---\nsee [Seed catalog] and [Ana]\n",
        )
        .unwrap();
        fs::write(
            root.join("Seed catalog.atlas"),
            "= Seeds\nback to [garden plan]\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("journal")).unwrap();
        fs::write(
            root.join("journal").join("2026-10-19.atlas"),
            "title: 2026-10-19\n---\nmet about the [Garden plan]\nnothing here\n",
        )
        .unwrap();
        let text = format!(
            "[[atlas-vault]]\nname = \"personal\"\npath = \"{}\"\n",
            root.display().to_string().replace('\\', "/")
        );
        let mut editor = Editor::new();
        editor.apply_config(&Config::parse(&text).unwrap());
        editor.today = || date(2026, 10, 19);
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
