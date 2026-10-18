use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use jiff::Zoned;

use crate::dired::{Listing, Mark};
use crate::editor::buffers::Mode;
use crate::editor::minibuffer::Purpose;
use crate::editor::{Editor, Effect};
use crate::files::{ops, paths, read_entries, sort};
use crate::text::Buffer;

impl Editor {
    pub(super) fn dired_prompt(&mut self) {
        self.open_file_prompt("Dired (directory): ", Purpose::Dired);
    }

    pub fn dired(&mut self, directory: &Path) -> bool {
        let directory = self.absolute(directory);
        if let Some(index) = self.listing_index(&directory) {
            self.select_buffer(index);
            return true;
        }
        let Some(listing) = self.read_listing(&directory) else {
            return false;
        };
        self.add_buffer_with(listing_buffer(listing.directory()), Mode::Dired(listing));
        self.redisplay_listing(0);
        true
    }

    pub(super) fn dired_jump(&mut self) {
        if self.buffers.listing(0).is_some() {
            self.dired_up_directory();
            return;
        }
        let file = self.buffers.current().file().map(|file| file.path.clone());
        let directory = file
            .as_deref()
            .and_then(Path::parent)
            .map_or_else(|| self.default_directory(), Path::to_owned);
        if self.dired(&directory)
            && let Some(name) = file.as_deref().and_then(Path::file_name)
        {
            self.dired_focus(name);
        }
    }

    pub(super) fn dired_find_file(&mut self) {
        let line = self.point_line();
        let Some(listing) = self.buffers.listing(0) else {
            return;
        };
        if Some(line) == listing.parent_line() {
            self.dired_up_directory();
            return;
        }
        let Some(entry) = listing.entry_at(line) else {
            self.show_message("No file on this line");
            return;
        };
        let path = listing.directory().join(&entry.name);
        if entry.is_dir() {
            self.dired_move_to(&path, None);
        } else {
            self.open_file(&path);
        }
    }

    pub(super) fn dired_up_directory(&mut self) {
        let Some(listing) = self.buffers.listing(0) else {
            return;
        };
        let Some(parent) = listing.directory().parent() else {
            return;
        };
        let parent = parent.to_owned();
        let from = listing.directory().file_name().map(OsStr::to_owned);
        self.dired_move_to(&parent, from.as_deref());
    }

    fn dired_move_to(&mut self, directory: &Path, focus: Option<&OsStr>) {
        let directory = self.absolute(directory);
        if let Some(index) = self.listing_index(&directory) {
            self.select_buffer(index);
            self.finish_kill(1);
        } else {
            let Some(current) = self.buffers.listing(0) else {
                return;
            };
            let (sort, details) = (current.sort(), current.details());
            let Some(mut listing) = self.read_listing(&directory) else {
                return;
            };
            listing.set_sort(sort);
            listing.set_details(details);
            self.buffers
                .replace_current(listing_buffer(listing.directory()), Mode::Dired(listing));
            self.name_file_buffers();
            self.after_buffer_switch();
            self.redisplay_listing(0);
        }
        if let Some(name) = focus {
            self.dired_focus(name);
        }
    }

    pub(super) fn refresh_listing(&mut self, index: usize) {
        let Some(listing) = self.buffers.listing(index) else {
            return;
        };
        let directory = listing.directory().to_owned();
        match read_entries(&directory) {
            Ok(entries) => self.update_listing(index, |listing| listing.set_entries(entries)),
            Err(error) => self.show_message(format!("{}: {error}", directory.display())),
        }
    }

    pub fn watched_directories(&self) -> Vec<PathBuf> {
        let mut directories: Vec<PathBuf> = (0..self.buffers.len())
            .filter_map(|index| self.buffers.listing(index))
            .map(|listing| listing.directory().to_owned())
            .collect();
        directories.sort();
        directories.dedup();
        directories
    }

    pub(super) fn refresh_changed_listings(&mut self) {
        for index in 0..self.buffers.len() {
            let Some(listing) = self.buffers.listing(index) else {
                continue;
            };
            let Ok(mut entries) = read_entries(listing.directory()) else {
                continue;
            };
            sort(&mut entries, listing.sort());
            if entries != listing.entries() {
                self.update_listing(index, |listing| listing.set_entries(entries));
            }
        }
    }

    pub(super) fn dired_targets(&self, mark: Mark, at_point: bool) -> Vec<(PathBuf, String)> {
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

    pub(super) fn dired_do_open(&mut self) {
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

    pub(super) fn dired_create_directory(&mut self) {
        if self.buffers.listing(0).is_some() {
            self.open_file_prompt("Create directory: ", Purpose::CreateDirectory);
        }
    }

    pub(super) fn create_directory(&mut self, path: &Path) {
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

    pub(super) fn dired_do_delete(&mut self, flagged: bool) {
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

    pub(super) fn delete_paths(&mut self, paths: &[PathBuf]) {
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

    pub(super) fn refresh_listings(&mut self) {
        for index in 0..self.buffers.len() {
            self.refresh_listing(index);
        }
    }

    pub(super) fn dired_mark(&mut self, mark: Option<Mark>) {
        self.mark_at_point(mark);
        self.dired_next_line(1);
    }

    pub(super) fn dired_unmark_backward(&mut self) {
        self.dired_next_line(-1);
        self.mark_at_point(None);
    }

    pub(super) fn dired_unmark_all(&mut self) {
        self.update_listing(0, Listing::clear_marks);
    }

    pub(super) fn dired_toggle_marks(&mut self) {
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

    pub(super) fn dired_cycle_sort(&mut self) {
        self.update_listing(0, |listing| listing.set_sort(listing.sort().next()));
    }

    pub(super) fn dired_hide_details(&mut self) {
        self.update_listing(0, |listing| listing.set_details(!listing.details()));
    }

    pub(super) fn dired_focus(&mut self, name: &OsStr) {
        if let Some((buffer, listing)) = self.buffers.dired_mut(0)
            && let Some(line) = listing.line_of(name)
        {
            place_point(buffer, listing, line);
            self.viewport.reveal(self.point_line());
        }
    }

    fn listing_index(&self, directory: &Path) -> Option<usize> {
        (0..self.buffers.len()).find(|index| {
            self.buffers
                .listing(*index)
                .is_some_and(|listing| listing.directory() == directory)
        })
    }

    fn read_listing(&mut self, directory: &Path) -> Option<Listing> {
        match Listing::read(directory.to_owned()) {
            Ok(listing) => Some(listing),
            Err(error) => {
                self.show_message(format!("{}: {error}", directory.display()));
                None
            }
        }
    }

    pub(super) fn redisplay_listing(&mut self, index: usize) {
        self.update_listing(index, |_| {});
    }

    fn update_listing(&mut self, index: usize, change: impl FnOnce(&mut Listing)) {
        let home = self.home.clone();
        let Some((buffer, listing)) = self.buffers.dired_mut(index) else {
            return;
        };
        let line = buffer.line_of(buffer.point());
        let focus = listing.entry_at(line).map(|entry| entry.name.clone());
        change(listing);
        buffer.set_text(&listing.render(&Zoned::now(), home.as_deref()));
        let line = focus
            .and_then(|name| listing.line_of(&name))
            .unwrap_or_else(|| {
                if Some(line) == listing.parent_line() {
                    line
                } else {
                    line.max(listing.first_entry_line())
                        .min(listing.last_entry_line())
                }
            });
        place_point(buffer, listing, line);
        if index == 0 {
            self.viewport.reveal(self.point_line());
        }
    }

    pub(super) fn dired_next_line(&mut self, lines: isize) {
        let Some((buffer, listing)) = self.buffers.dired_mut(0) else {
            return;
        };
        let last = buffer.len_lines() - 1;
        let line = buffer
            .line_of(buffer.point())
            .saturating_add_signed(lines)
            .min(last);
        place_point(buffer, listing, line);
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

pub(super) fn files(count: usize) -> String {
    if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}

fn listing_buffer(directory: &Path) -> Buffer {
    let mut buffer = Buffer::new(directory.display().to_string());
    buffer.set_read_only(true);
    buffer
}

fn place_point(buffer: &mut Buffer, listing: &Listing, line: usize) {
    let start = buffer.line_start(line);
    let column = if line == 0 { 0 } else { listing.name_column() };
    buffer.set_point((start + column).min(buffer.line_end(line)));
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::editor::test_support::{at_point, labels, lines, press, question, type_text};
    use crate::editor::{Editor, Effect};
    use crate::files::paths::SEPARATOR as SEP;

    fn workspace() -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::write(dir.path().join("b.txt"), "bee").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub").join("c.txt"), "sea").unwrap();
        let mut editor = Editor::new();
        editor.set_home_directory(None);
        editor.set_working_directory(dir.path().to_owned());
        (dir, editor)
    }

    fn dir_name(path: &Path) -> String {
        format!("{}{SEP}", path.file_name().unwrap().to_string_lossy())
    }

    #[test]
    fn dired_lists_a_directory_in_a_read_only_buffer() {
        let (dir, mut editor) = workspace();

        assert!(editor.dired(dir.path()));

        let lines = lines(&editor);
        assert_eq!(lines[0], format!("  {}:", dir.path().display()));
        assert!(lines[1].ends_with(" .."), "{}", lines[1]);
        assert!(lines[2].ends_with(&format!(" sub{SEP}")), "{}", lines[2]);
        assert!(lines[3].ends_with(" a.txt"), "{}", lines[3]);
        assert!(lines[4].ends_with(" b.txt"), "{}", lines[4]);
        assert_eq!(editor.buffer().name(), dir_name(dir.path()));
        assert!(editor.buffer().is_read_only());
        assert!(!editor.buffer().is_modified());
    }

    #[test]
    fn point_starts_on_the_first_entry_name() {
        let (dir, mut editor) = workspace();

        editor.dired(dir.path());

        assert_eq!(at_point(&editor), format!("sub{SEP}"));
    }

    #[test]
    fn a_directory_already_listed_reuses_its_buffer() {
        let (dir, mut editor) = workspace();
        editor.dired(dir.path());
        editor.open_file(&dir.path().join("a.txt"));

        editor.dired(dir.path());

        assert_eq!(
            editor.buffer_names(),
            [dir_name(dir.path()).as_str(), "a.txt", "*scratch*"]
        );
    }

    #[test]
    fn missing_directories_are_reported() {
        let (dir, mut editor) = workspace();

        assert!(!editor.dired(&dir.path().join("missing")));

        assert!(editor.message().unwrap().contains("missing"));
        assert_eq!(editor.buffer_names(), ["*scratch*"]);
    }

    #[test]
    fn the_default_directory_of_a_listing_is_its_directory() {
        let (dir, mut editor) = workspace();

        editor.dired(&dir.path().join("sub"));

        assert_eq!(editor.default_directory(), dir.path().join("sub"));
    }

    #[test]
    fn c_x_d_offers_directories_and_lists_the_chosen_one() {
        let (dir, mut editor) = workspace();

        press(&mut editor, "C-x d");
        let minibuffer = editor.minibuffer().unwrap();
        assert_eq!(minibuffer.prompt(), "Dired (directory): ");
        assert_eq!(
            minibuffer.input().contents(),
            format!("{}{SEP}", dir.path().display())
        );
        assert_eq!(labels(&editor), [format!("sub{SEP}")]);

        press(&mut editor, "RET");
        assert_eq!(editor.buffer().name(), dir_name(dir.path()));
    }

    #[test]
    fn c_x_d_descends_into_the_selected_directory() {
        let (dir, mut editor) = workspace();

        press(&mut editor, "C-x d");
        type_text(&mut editor, "su");
        press(&mut editor, "RET RET");

        assert_eq!(editor.default_directory(), dir.path().join("sub"));
        assert_eq!(editor.buffer().name(), format!("sub{SEP}"));
    }

    fn listed(editor: &mut Editor, dir: &tempfile::TempDir) {
        editor.dired(dir.path());
    }

    #[test]
    fn n_and_p_move_between_names() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        for (keys, expected) in [
            ("n", "a.txt".to_owned()),
            ("SPC", "b.txt".to_owned()),
            ("<down>", String::new()),
            ("C-n", String::new()),
            ("p", "b.txt".to_owned()),
            ("C-p", "a.txt".to_owned()),
            ("<up>", format!("sub{SEP}")),
            ("p", "..".to_owned()),
            ("C-n", format!("sub{SEP}")),
        ] {
            press(&mut editor, keys);
            assert_eq!(at_point(&editor), expected, "{keys}");
        }
    }

    #[test]
    fn moving_up_from_the_parent_line_reaches_the_header() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "p p p");

        assert_eq!(editor.buffer().point(), 0);
    }

    #[test]
    fn typing_in_a_listing_is_refused() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "z");

        assert_eq!(
            editor.message(),
            Some(format!("Buffer is read-only: {}", dir_name(dir.path())).as_str())
        );
    }

    #[test]
    fn the_minibuffer_ignores_listing_keys() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "C-x C-f n");

        assert!(
            editor
                .minibuffer()
                .unwrap()
                .input()
                .contents()
                .ends_with('n')
        );
    }

    fn command_names(editor: &mut Editor) -> Vec<String> {
        press(editor, "M-x");
        let names = editor
            .completions()
            .into_iter()
            .map(|completion| completion.label)
            .collect();
        press(editor, "C-g");
        names
    }

    #[test]
    fn listing_commands_are_offered_only_in_listings() {
        let (dir, mut editor) = workspace();
        assert!(!command_names(&mut editor).contains(&"dired-next-line".to_owned()));

        listed(&mut editor, &dir);

        assert!(command_names(&mut editor).contains(&"dired-next-line".to_owned()));
    }

    #[test]
    fn return_f_and_e_open_the_file_at_point() {
        for key in ["RET", "f", "e"] {
            let (dir, mut editor) = workspace();
            listed(&mut editor, &dir);

            press(&mut editor, "n");
            press(&mut editor, key);

            assert_eq!(editor.buffer().name(), "a.txt", "{key}");
            assert_eq!(editor.buffer().contents(), "hello", "{key}");
            assert_eq!(editor.buffer_names().len(), 3, "{key}");
        }
    }

    #[test]
    fn return_on_a_directory_moves_the_listing_there() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "RET");

        assert_eq!(editor.default_directory(), dir.path().join("sub"));
        assert_eq!(
            editor.buffer_names(),
            [format!("sub{SEP}").as_str(), "*scratch*"]
        );
        assert_eq!(at_point(&editor), "c.txt");
    }

    #[test]
    fn caret_goes_up_with_point_on_the_directory_left() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);
        press(&mut editor, "RET");

        press(&mut editor, "^");

        assert_eq!(editor.default_directory(), dir.path());
        assert_eq!(
            editor.buffer_names(),
            [dir_name(dir.path()).as_str(), "*scratch*"]
        );
        assert_eq!(at_point(&editor), format!("sub{SEP}"));
    }

    #[test]
    fn return_on_the_parent_line_goes_up() {
        let (dir, mut editor) = workspace();
        editor.dired(&dir.path().join("sub"));

        press(&mut editor, "p RET");

        assert_eq!(editor.default_directory(), dir.path());
        assert_eq!(at_point(&editor), format!("sub{SEP}"));
    }

    #[test]
    fn moving_to_a_directory_listed_elsewhere_switches_to_that_listing() {
        let (dir, mut editor) = workspace();
        editor.dired(&dir.path().join("sub"));
        listed(&mut editor, &dir);

        press(&mut editor, "RET");

        assert_eq!(
            editor.buffer_names(),
            [format!("sub{SEP}").as_str(), "*scratch*"]
        );
    }

    #[test]
    fn return_on_the_header_reports_no_file() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "M-< RET");

        assert_eq!(editor.message(), Some("No file on this line"));
    }

    #[test]
    fn g_rereads_the_directory_keeping_point_on_its_file() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);
        press(&mut editor, "n");
        fs::write(dir.path().join("0.txt"), "").unwrap();

        press(&mut editor, "g");

        assert!(lines(&editor)[3].ends_with(" 0.txt"));
        assert_eq!(at_point(&editor), "a.txt");
        assert!(!editor.buffer().is_modified());
    }

    #[test]
    fn g_keeps_point_near_a_file_that_vanished() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);
        press(&mut editor, "n n");
        fs::remove_file(dir.path().join("b.txt")).unwrap();

        press(&mut editor, "g");

        assert_eq!(at_point(&editor), "a.txt");
    }

    #[test]
    fn revert_buffer_refreshes_a_listing() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);
        fs::write(dir.path().join("aa.txt"), "").unwrap();

        editor.execute(crate::command::Command::RevertBuffer);

        assert!(lines(&editor)[4].ends_with(" aa.txt"));
    }

    #[test]
    fn s_cycles_the_sort_order_keeping_point() {
        let (dir, mut editor) = workspace();
        fs::write(dir.path().join("b.txt"), "a much larger file").unwrap();
        listed(&mut editor, &dir);
        press(&mut editor, "n");

        press(&mut editor, "s");
        assert!(lines(&editor)[0].ends_with(" (newest first):"));

        press(&mut editor, "s");
        assert!(lines(&editor)[0].ends_with(" (largest first):"));
        assert!(lines(&editor)[3].ends_with(" b.txt"));
        assert_eq!(at_point(&editor), "a.txt");

        press(&mut editor, "s");
        assert!(lines(&editor)[0].ends_with(':'));
        assert!(!lines(&editor)[0].contains('('));
    }

    #[test]
    fn parenthesis_hides_and_shows_details() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "(");
        assert_eq!(lines(&editor)[2], format!("  sub{SEP}"));
        assert_eq!(at_point(&editor), format!("sub{SEP}"));

        press(&mut editor, "(");
        assert!(lines(&editor)[2].len() > format!("  sub{SEP}").len());
    }

    #[test]
    fn q_buries_the_listing() {
        let (dir, mut editor) = workspace();
        editor.open_file(&dir.path().join("a.txt"));
        listed(&mut editor, &dir);

        press(&mut editor, "q");

        assert_eq!(
            editor.buffer_names(),
            ["a.txt", "*scratch*", dir_name(dir.path()).as_str()]
        );
    }

    #[test]
    fn opening_a_directory_lists_it() {
        let (dir, mut editor) = workspace();

        assert!(editor.open_file(&dir.path().join("sub")));

        assert_eq!(editor.buffer().name(), format!("sub{SEP}"));
        assert_eq!(at_point(&editor), "c.txt");
    }

    #[test]
    fn find_file_on_a_bare_directory_lists_it() {
        let (dir, mut editor) = workspace();

        press(&mut editor, "C-x C-f RET");

        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.buffer().name(), dir_name(dir.path()));
    }

    #[test]
    fn visiting_a_directory_lists_it_and_ignores_the_line() {
        let (dir, mut editor) = workspace();

        editor.visit(&crate::cli::Visit {
            path: dir.path().join("sub"),
            line: Some(1),
            column: None,
        });

        assert_eq!(at_point(&editor), "c.txt");
    }

    #[test]
    fn c_x_c_j_lists_the_files_directory_with_point_on_it() {
        let (dir, mut editor) = workspace();
        editor.open_file(&dir.path().join("b.txt"));

        press(&mut editor, "C-x C-j");

        assert_eq!(editor.buffer().name(), dir_name(dir.path()));
        assert_eq!(at_point(&editor), "b.txt");
    }

    #[test]
    fn c_x_c_j_in_a_listing_goes_up() {
        let (dir, mut editor) = workspace();
        editor.dired(&dir.path().join("sub"));

        press(&mut editor, "C-x C-j");

        assert_eq!(editor.default_directory(), dir.path());
        assert_eq!(at_point(&editor), format!("sub{SEP}"));
    }

    #[test]
    fn c_x_c_j_without_a_file_lists_the_default_directory() {
        let (dir, mut editor) = workspace();

        press(&mut editor, "C-x C-j");

        assert_eq!(editor.buffer().name(), dir_name(dir.path()));
    }

    #[test]
    fn saving_a_listing_has_nothing_to_save() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "C-x C-s");

        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.message(), Some("(No changes need to be saved)"));
    }

    #[test]
    fn listings_cannot_be_written_to_files() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "C-x C-w");

        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.buffer().file(), None);
        assert_eq!(
            editor.message(),
            Some("This buffer can't be written to a file")
        );
    }

    #[test]
    fn listings_stay_read_only() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "C-x C-q");

        assert!(editor.buffer().is_read_only());
        assert_eq!(editor.message(), Some("This buffer stays read-only"));
    }

    fn mark_column(editor: &Editor) -> String {
        lines(editor)[2..]
            .iter()
            .map(|line| line.chars().next().unwrap())
            .collect()
    }

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

    fn trashed() -> Vec<std::path::PathBuf> {
        crate::files::ops::TRASHED.with_borrow(Clone::clone)
    }

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

    fn permanent(editor: &mut Editor, dir: &tempfile::TempDir) {
        let config = crate::config::Config::parse("delete-to-trash = false").unwrap();
        editor.apply_config(&config);
        listed(editor, dir);
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
    fn r_renames_the_file_at_point() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n R");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Rename a.txt to: ");
        type_text(&mut editor, "c.txt");
        press(&mut editor, "RET");

        assert!(!dir.path().join("a.txt").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("c.txt")).unwrap(),
            "hello"
        );
        assert_eq!(at_point(&editor), "c.txt");
        assert_eq!(editor.message(), Some("Moved 1 file"));
    }

    #[test]
    fn r_into_a_directory_moves_the_file_there() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n R");
        type_text(&mut editor, "sub");
        press(&mut editor, "RET RET");

        assert!(dir.path().join("sub").join("a.txt").exists());
        assert!(!dir.path().join("a.txt").exists());
    }

    #[test]
    fn r_moves_marked_files_into_a_directory() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n m m R");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Move 2 files to: ");
        type_text(&mut editor, "sub");
        press(&mut editor, "RET RET");

        assert!(dir.path().join("sub").join("a.txt").exists());
        assert!(dir.path().join("sub").join("b.txt").exists());
        assert_eq!(editor.message(), Some("Moved 2 files"));
    }

    #[test]
    fn moving_several_files_needs_a_directory() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n m m R");
        type_text(&mut editor, "nowhere");
        press(&mut editor, "M-RET");

        assert!(
            editor
                .message()
                .unwrap()
                .starts_with("Target must be a directory")
        );
        assert!(dir.path().join("a.txt").exists());
    }

    #[test]
    fn renaming_onto_an_existing_file_asks_first() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "n R");
        type_text(&mut editor, "b.txt");
        press(&mut editor, "RET");
        assert_eq!(
            question(&editor),
            Some(format!(
                "Overwrite {}? (y, n, !, q) ",
                dir.path().join("b.txt").display()
            ))
        );
        press(&mut editor, "n");
        assert_eq!(fs::read_to_string(dir.path().join("b.txt")).unwrap(), "bee");

        press(&mut editor, "R");
        type_text(&mut editor, "b.txt");
        press(&mut editor, "RET y");
        assert_eq!(
            fs::read_to_string(dir.path().join("b.txt")).unwrap(),
            "hello"
        );
        assert!(!dir.path().join("a.txt").exists());
    }

    #[test]
    fn renaming_carries_buffers_visiting_the_file() {
        let (dir, mut editor) = workspace();
        editor.open_file(&dir.path().join("a.txt"));
        listed(&mut editor, &dir);

        press(&mut editor, "n R");
        type_text(&mut editor, "c.txt");
        press(&mut editor, "RET C-x b RET");

        assert_eq!(editor.buffer().name(), "c.txt");
        assert_eq!(
            editor.buffer().file().map(|file| file.path.clone()),
            Some(dir.path().join("c.txt"))
        );
    }

    #[test]
    fn c_copies_directories_recursively() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        press(&mut editor, "C");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Copy sub to: ");
        type_text(&mut editor, "dup");
        press(&mut editor, "M-RET");

        assert_eq!(
            fs::read_to_string(dir.path().join("dup").join("c.txt")).unwrap(),
            "sea"
        );
        assert!(dir.path().join("sub").join("c.txt").exists());
        assert_eq!(editor.message(), Some("Copied 1 file"));
    }

    #[test]
    fn bang_overwrites_every_remaining_conflict() {
        let (dir, mut editor) = workspace();
        let target = dir.path().join("sub");
        fs::write(target.join("a.txt"), "old a").unwrap();
        fs::write(target.join("b.txt"), "old b").unwrap();
        listed(&mut editor, &dir);

        press(&mut editor, "n m m C");
        type_text(&mut editor, "sub");
        press(&mut editor, "RET RET !");

        assert_eq!(fs::read_to_string(target.join("a.txt")).unwrap(), "hello");
        assert_eq!(fs::read_to_string(target.join("b.txt")).unwrap(), "bee");
        assert_eq!(editor.message(), Some("Copied 2 files"));
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

    #[test]
    fn only_listing_lines_are_styled() {
        let (dir, mut editor) = workspace();
        assert_eq!(editor.line_style(0), crate::display::LineStyle::default());

        listed(&mut editor, &dir);
        press(&mut editor, "m");

        assert_eq!(
            editor.line_style(2).background,
            Some(crate::theme::Highlight::Marked)
        );
    }

    #[test]
    fn double_clicking_an_entry_opens_it() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);
        let cell = |line| crate::editor::Cell { line, column: 30 };

        editor.click(cell(3), crate::editor::Clicks::Single, false);
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 3);
        assert_eq!(editor.buffer().name(), dir_name(dir.path()));

        editor.click(cell(3), crate::editor::Clicks::Double, false);
        assert_eq!(editor.buffer().name(), "a.txt");
        assert_eq!(editor.buffer().selection(), None);
    }

    #[test]
    fn double_clicking_a_directory_enters_it() {
        let (dir, mut editor) = workspace();
        listed(&mut editor, &dir);

        editor.click(
            crate::editor::Cell { line: 2, column: 0 },
            crate::editor::Clicks::Double,
            false,
        );

        assert_eq!(editor.default_directory(), dir.path().join("sub"));
    }

    #[test]
    fn disk_checks_refresh_changed_listings() {
        let (dir, mut editor) = workspace();
        editor.open_file(&dir.path().join("a.txt"));
        listed(&mut editor, &dir);
        press(&mut editor, "n m");
        fs::write(dir.path().join("0.txt"), "").unwrap();

        editor.check_disk_changes();

        assert!(lines(&editor)[3].ends_with(" 0.txt"));
        assert_eq!(at_point(&editor), "b.txt");
        assert_eq!(mark_column(&editor), "  * ");
    }

    #[test]
    fn listings_refresh_even_when_file_changes_are_ignored() {
        let (dir, mut editor) = workspace();
        let config = crate::config::Config::parse("on-disk-change = \"ignore\"").unwrap();
        editor.apply_config(&config);
        listed(&mut editor, &dir);
        fs::write(dir.path().join("0.txt"), "").unwrap();

        editor.check_disk_changes();

        assert!(lines(&editor)[3].ends_with(" 0.txt"));
    }

    #[test]
    fn watched_directories_are_the_listed_ones() {
        let (dir, mut editor) = workspace();
        editor.dired(&dir.path().join("sub"));
        listed(&mut editor, &dir);
        editor.open_file(&dir.path().join("a.txt"));

        assert_eq!(
            editor.watched_directories(),
            [dir.path().to_owned(), dir.path().join("sub")]
        );
    }

    #[test]
    fn listings_of_directories_sharing_a_name_show_their_parent() {
        let (dir, mut editor) = workspace();
        for parent in ["x", "y"] {
            fs::create_dir_all(dir.path().join(parent).join("src")).unwrap();
            editor.dired(&dir.path().join(parent).join("src"));
        }

        assert_eq!(
            editor.buffer_names(),
            [
                format!("src{SEP}<y>").as_str(),
                format!("src{SEP}<x>").as_str(),
                "*scratch*"
            ]
        );
    }
}
