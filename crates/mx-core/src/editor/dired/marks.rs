use std::path::PathBuf;

use crate::dired::{Listing, Mark};
use crate::editor::Editor;

impl Editor {
    pub(in crate::editor) fn dired_targets(
        &self,
        mark: Mark,
        at_point: bool,
    ) -> Vec<(PathBuf, String)> {
        let line = self.point_line();
        let Some(listing) = self.buffers.listing(0) else {
            return Vec::new();
        };
        let mut entries = listing.marked(mark);
        if entries.is_empty() && at_point {
            entries.extend(listing.entry_at(line));
        }
        entries
            .iter()
            .map(|entry| {
                (
                    listing.directory().join(&entry.name),
                    entry.display_name().into_owned(),
                )
            })
            .collect()
    }

    pub(in crate::editor) fn dired_mark(&mut self, mark: Option<Mark>) {
        self.mark_at_point(mark);
        self.dired_next_line(1);
    }

    pub(in crate::editor) fn dired_unmark_backward(&mut self) {
        self.dired_next_line(-1);
        self.mark_at_point(None);
    }

    pub(in crate::editor) fn dired_unmark_all(&mut self) {
        self.update_listing(0, Listing::clear_marks);
    }

    pub(in crate::editor) fn dired_toggle_marks(&mut self) {
        self.update_listing(0, Listing::toggle_marks);
    }

    fn mark_at_point(&mut self, mark: Option<Mark>) {
        let line = self.point_line();
        let Some(name) = self
            .buffers
            .listing(0)
            .and_then(|listing| listing.entry_at(line))
            .map(|entry| entry.name.clone())
        else {
            return;
        };
        self.update_listing(0, |listing| listing.set_mark(&name, mark));
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::dired::fixtures::{listed, mark_column, workspace};
    use crate::editor::test_support::{at_point, lines, press};
    use crate::files::paths::SEPARATOR as SEP;

    #[test]
    fn m_marks_and_moves_down() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "m");
        assert_eq!(mark_column(&editor), "*  ");
        assert_eq!(at_point(&editor), "a.txt");

        press(&mut editor, "m m m");
        assert_eq!(mark_column(&editor), "***");
        assert_eq!(at_point(&editor), "");
    }

    #[test]
    fn d_flags_files_for_deletion() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n d");

        assert_eq!(mark_column(&editor), " D ");
    }

    #[test]
    fn u_unmarks_forward_and_del_unmarks_backward() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);
        press(&mut editor, "m m m");

        press(&mut editor, "DEL");
        assert_eq!(mark_column(&editor), "** ");
        assert_eq!(at_point(&editor), "b.txt");

        press(&mut editor, "p u");
        assert_eq!(mark_column(&editor), "*  ");
        assert_eq!(at_point(&editor), "b.txt");
    }

    #[test]
    fn capital_u_removes_every_mark() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "m d m U");

        assert_eq!(mark_column(&editor), "   ");
    }

    #[test]
    fn t_toggles_marks_but_keeps_flags() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "m d t");

        assert_eq!(mark_column(&editor), " D*");
    }

    #[test]
    fn the_parent_line_cannot_be_marked() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "p m");

        assert!(lines(&editor)[1].starts_with(' '));
        assert_eq!(at_point(&editor), format!("sub{SEP}"));
    }

    #[test]
    fn marks_survive_a_refresh() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "m g");

        assert_eq!(mark_column(&editor), "*  ");
    }
}
