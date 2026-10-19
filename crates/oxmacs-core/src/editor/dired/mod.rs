#[cfg(test)]
mod fixtures;
mod marks;
mod operations;
mod transfer;

pub(super) use transfer::{Kind, Transfer};

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use jiff::Zoned;

use crate::dired::Listing;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::prompts::minibuffer::Purpose;
use crate::files::{read_entries, sort};
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

    pub(super) fn refresh_listings(&mut self) {
        for index in 0..self.buffers.len() {
            self.refresh_listing(index);
        }
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
            self.reveal_point();
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
            self.reveal_point();
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

    use super::fixtures::{dir_name, listed, mark_column, workspace};
    use crate::editor::Editor;
    use crate::editor::test_support::{at_point, labels, lines, press, type_text};
    use crate::files::paths::SEPARATOR as SEP;

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
        let cell = |line| crate::editor::Cell {
            row: line,
            column: 30,
        };

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
            crate::editor::Cell { row: 2, column: 0 },
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
