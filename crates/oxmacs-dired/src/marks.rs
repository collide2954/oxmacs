use std::path::PathBuf;

use oxmacs_core::editor::Editor;

use crate::browse::{current, next_line, point_line, update};
use crate::listing::{Listing, Mark};

pub(crate) fn targets(editor: &Editor, mark: Mark, at_point: bool) -> Vec<(PathBuf, String)> {
    let line = point_line(editor);
    let Some(listing) = current(editor) else {
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

pub(crate) fn mark(editor: &mut Editor, mark: Option<Mark>) {
    mark_at_point(editor, mark);
    next_line(editor, 1);
}

pub(crate) fn unmark_backward(editor: &mut Editor) {
    next_line(editor, -1);
    mark_at_point(editor, None);
}

pub(crate) fn unmark_all(editor: &mut Editor) {
    let id = editor.buffer().id();
    update(editor, id, Listing::clear_marks);
}

pub(crate) fn toggle(editor: &mut Editor) {
    let id = editor.buffer().id();
    update(editor, id, Listing::toggle_marks);
}

fn mark_at_point(editor: &mut Editor, mark: Option<Mark>) {
    let line = point_line(editor);
    let Some(name) = current(editor)
        .and_then(|listing| listing.entry_at(line))
        .map(|entry| entry.name.clone())
    else {
        return;
    };
    let id = editor.buffer().id();
    update(editor, id, |listing| listing.set_mark(&name, mark));
}
#[cfg(test)]
mod tests {
    use oxmacs_core::files::paths::SEPARATOR as SEP;

    use crate::fixtures::{listed, mark_column, workspace};
    use crate::testing::{at_point, lines, press};

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
