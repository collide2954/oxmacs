use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::dired::Mark;
use crate::editor::minibuffer::Purpose;
use crate::editor::{Editor, Effect};
use crate::files::{ops, paths};

impl Editor {
    pub(in crate::editor) fn dired_do_open(&mut self) {
        let targets = self.dired_targets(Mark::Marked, true);
        if targets.is_empty() {
            self.show_message("No file on this line");
            return;
        }
        self.effects.extend(
            targets
                .into_iter()
                .map(|(path, _)| Effect::OpenExternally(path)),
        );
    }

    pub(in crate::editor) fn dired_create_directory(&mut self) {
        if self.buffers.listing(0).is_some() {
            self.open_file_prompt("Create directory: ", Purpose::CreateDirectory);
        }
    }

    pub(in crate::editor) fn create_directory(&mut self, path: &Path) {
        let path = self.absolute(path);
        let shown = paths::abbreviate(&path, self.home.as_deref());
        if path.exists() {
            self.show_message(format!("{shown} already exists"));
            return;
        }
        if let Err(error) = fs::create_dir_all(&path) {
            self.show_message(format!("{shown}: {error}"));
            return;
        }
        self.refresh_listings();
        let created = self
            .buffers
            .listing(0)
            .and_then(|listing| path.strip_prefix(listing.directory()).ok())
            .and_then(|rest| rest.iter().next())
            .map(OsStr::to_owned);
        if let Some(name) = created {
            self.dired_focus(&name);
        }
    }

    pub(in crate::editor) fn dired_do_delete(&mut self, flagged: bool) {
        let targets = if flagged {
            self.dired_targets(Mark::Flagged, false)
        } else {
            self.dired_targets(Mark::Marked, true)
        };
        if targets.is_empty() {
            self.show_message(if flagged {
                "(No deletions requested)"
            } else {
                "No file on this line"
            });
            return;
        }
        let (paths, names): (Vec<PathBuf>, Vec<String>) = targets.into_iter().unzip();
        let verb = if self.delete_to_trash {
            "Trash"
        } else {
            "Delete"
        };
        let typed = !self.delete_to_trash;
        self.ask_to_delete(&format!("{verb} {}?", describe(&names)), paths, typed);
    }

    pub(in crate::editor) fn delete_paths(&mut self, paths: &[PathBuf]) {
        let result = if self.delete_to_trash {
            ops::trash(paths)
        } else {
            paths
                .iter()
                .try_for_each(|path| ops::remove(path))
                .map_err(|error| error.to_string())
        };
        self.refresh_listings();
        match result {
            Ok(()) => {
                let verb = if self.delete_to_trash {
                    "Trashed"
                } else {
                    "Deleted"
                };
                self.show_message(format!("{verb} {}", files(paths.len())));
            }
            Err(error) => self.show_message(error),
        }
    }
}

fn describe(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [first, second, third] => format!("{first}, {second} and {third}"),
        [first, second, rest @ ..] => format!("{first}, {second} and {} more", rest.len()),
    }
}

pub(in crate::editor) fn files(count: usize) -> String {
    if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::editor::Effect;
    use crate::editor::dired::fixtures::{listed, mark_column, permanent, trashed, workspace};
    use crate::editor::test_support::{at_point, lines, press, question, type_text};
    use crate::files::paths::SEPARATOR as SEP;

    #[test]
    fn x_trashes_flagged_files_after_asking() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);
        press(&mut editor, "n d d x");

        assert_eq!(
            question(&editor),
            Some("Trash a.txt and b.txt? (y or n) ".to_owned())
        );
        press(&mut editor, "y");

        assert!(!dir.path().join("a.txt").exists());
        assert_eq!(
            trashed(),
            [dir.path().join("a.txt"), dir.path().join("b.txt")]
        );
        assert_eq!(lines(&editor).len(), 3);
        assert_eq!(editor.message(), Some("Trashed 2 files"));
    }

    #[test]
    fn declining_keeps_the_files_flagged() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n d x n");

        assert!(dir.path().join("a.txt").exists());
        assert_eq!(mark_column(&editor), " D ");
    }

    #[test]
    fn x_without_flags_reports_it() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "x");

        assert_eq!(question(&editor), None);
        assert_eq!(editor.message(), Some("(No deletions requested)"));
    }

    #[test]
    fn capital_d_deletes_marked_files_or_the_one_at_point() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n D");
        assert_eq!(question(&editor), Some("Trash a.txt? (y or n) ".to_owned()));
        press(&mut editor, "y");
        assert!(!dir.path().join("a.txt").exists());
        assert_eq!(editor.message(), Some("Trashed 1 file"));

        press(&mut editor, "M-< n n m m D");
        assert_eq!(
            question(&editor),
            Some("Trash sub and b.txt? (y or n) ".to_owned())
        );
    }

    #[test]
    fn the_question_names_at_most_three_files() {
        let (dir, mut editor) = workspace();
        for name in ["c.txt", "d.txt"] {
            fs::write(dir.path().join(name), "").unwrap();
        }
        listed(&mut editor, &dir);

        press(&mut editor, "n d d d x");
        assert_eq!(
            question(&editor),
            Some("Trash a.txt, b.txt and c.txt? (y or n) ".to_owned())
        );
        press(&mut editor, "n d x");
        assert_eq!(
            question(&editor),
            Some("Trash a.txt, b.txt and 2 more? (y or n) ".to_owned())
        );
    }

    #[test]
    fn permanent_deletion_needs_a_typed_yes() {
        let (dir, mut editor) = workspace();
        permanent(&mut editor, &dir);

        press(&mut editor, "n D");
        assert_eq!(
            question(&editor),
            Some("Delete a.txt? (yes or no) ".to_owned())
        );
        press(&mut editor, "y RET");
        assert_eq!(editor.message(), Some("Please answer yes or no"));
        assert!(dir.path().join("a.txt").exists());

        type_text(&mut editor, "yez");
        press(&mut editor, "DEL");
        assert_eq!(editor.question().unwrap().answer(), "ye");
        press(&mut editor, "s RET");

        assert!(!dir.path().join("a.txt").exists());
        assert_eq!(trashed(), Vec::<std::path::PathBuf>::new());
        assert_eq!(editor.message(), Some("Deleted 1 file"));
    }

    #[test]
    fn answering_no_keeps_the_file() {
        let (dir, mut editor) = workspace();
        permanent(&mut editor, &dir);

        press(&mut editor, "n D");
        type_text(&mut editor, "no");
        press(&mut editor, "RET");

        assert_eq!(question(&editor), None);
        assert!(dir.path().join("a.txt").exists());
    }

    #[test]
    fn plus_creates_a_directory_with_point_on_it() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "+");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Create directory: ");
        type_text(&mut editor, "new");
        press(&mut editor, "RET");

        assert!(dir.path().join("new").is_dir());
        assert_eq!(at_point(&editor), format!("new{SEP}"));
    }

    #[test]
    fn plus_creates_missing_parents() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "+");
        type_text(&mut editor, &format!("deep{SEP}er"));
        press(&mut editor, "M-RET");

        assert!(dir.path().join("deep").join("er").is_dir());
        assert_eq!(at_point(&editor), format!("deep{SEP}"));
    }

    #[test]
    fn creating_an_existing_directory_is_reported() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "+");
        type_text(&mut editor, "sub");
        press(&mut editor, "M-RET");

        assert!(editor.message().unwrap().ends_with("already exists"));
    }

    #[test]
    fn w_asks_the_system_to_open_files() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n W");
        assert_eq!(
            editor.take_effects(),
            [Effect::OpenExternally(dir.path().join("a.txt"))]
        );

        press(&mut editor, "m m W");
        assert_eq!(
            editor.take_effects(),
            [
                Effect::OpenExternally(dir.path().join("a.txt")),
                Effect::OpenExternally(dir.path().join("b.txt"))
            ]
        );
    }
}
