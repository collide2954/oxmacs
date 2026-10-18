use std::path::Path;

use jiff::Zoned;

use crate::dired::Listing;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::minibuffer::Purpose;
use crate::text::Buffer;

impl Editor {
    pub(super) fn dired_prompt(&mut self) {
        self.open_file_prompt("Dired (directory): ", Purpose::Dired);
    }

    pub fn dired(&mut self, directory: &Path) -> bool {
        let directory = self.absolute(directory);
        if let Some(index) = (0..self.buffers.len()).find(|index| {
            self.buffers
                .listing(*index)
                .is_some_and(|listing| listing.directory() == directory)
        }) {
            self.select_buffer(index);
            return true;
        }
        let listing = match Listing::read(directory.clone()) {
            Ok(listing) => listing,
            Err(error) => {
                self.show_message(format!("{}: {error}", directory.display()));
                return false;
            }
        };
        let mut buffer = Buffer::new(directory.display().to_string());
        buffer.set_read_only(true);
        self.add_buffer_with(buffer, Mode::Dired(listing));
        self.redisplay_listing(0);
        true
    }

    pub(super) fn redisplay_listing(&mut self, index: usize) {
        let home = self.home.clone();
        let Some((buffer, listing)) = self.buffers.dired_mut(index) else {
            return;
        };
        let line = buffer.line_of(buffer.point());
        let focus = listing.entry_at(line).map(|entry| entry.name.clone());
        buffer.set_text(&listing.render(&Zoned::now(), home.as_deref()));
        let last = buffer.len_lines() - 1;
        let line = focus
            .and_then(|name| listing.line_of(&name))
            .unwrap_or_else(|| {
                if Some(line) == listing.parent_line() {
                    line
                } else {
                    line.max(listing.first_entry_line()).min(last)
                }
            });
        let start = buffer.line_start(line);
        let column = if line == 0 { 0 } else { listing.name_column() };
        buffer.set_point((start + column).min(buffer.line_end(line)));
        if index == 0 {
            self.viewport.reveal(self.point_line());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::editor::Editor;
    use crate::editor::test_support::{press, type_text};
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

    fn lines(editor: &Editor) -> Vec<String> {
        editor
            .buffer()
            .contents()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn at_point(editor: &Editor) -> String {
        let buffer = editor.buffer();
        let line = buffer.line_of(buffer.point());
        buffer.text_in(buffer.point()..buffer.line_end(line))
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
        let labels: Vec<String> = editor
            .completions()
            .into_iter()
            .map(|completion| completion.label)
            .collect();
        assert_eq!(labels, [format!("sub{SEP}")]);

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
