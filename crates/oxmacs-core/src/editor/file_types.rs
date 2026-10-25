use std::path::Path;

use crate::editor::Editor;

pub type FileOpener = fn(&mut Editor, &Path) -> bool;

impl Editor {
    pub fn on_open_file_type(&mut self, extension: &str, handler: FileOpener) {
        let extension = extension.to_lowercase();
        self.file_openers.retain(|(known, _)| *known != extension);
        self.file_openers.push((extension, handler));
    }

    pub(super) fn file_opener(&self, path: &Path) -> Option<FileOpener> {
        let extension = path.extension()?.to_str()?.to_lowercase();
        self.file_openers
            .iter()
            .find(|(known, _)| *known == extension)
            .map(|(_, open)| *open)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::editor::Editor;

    fn read(editor: &mut Editor, path: &Path) -> bool {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        editor.show_message(format!("reading {name}"));
        true
    }

    #[test]
    fn files_of_a_claimed_type_go_to_their_opener() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("book.EPUB"), [0x50, 0x4b, 3, 4]).unwrap();
        std::fs::write(dir.path().join("notes.txt"), "hello").unwrap();
        let mut editor = Editor::new();
        editor.on_open_file_type("epub", read);

        assert!(editor.open_file(&dir.path().join("book.EPUB")));
        assert_eq!(editor.message(), Some("reading book.EPUB"));
        assert_eq!(editor.buffer().name(), "*scratch*");

        assert!(editor.open_file(&dir.path().join("notes.txt")));
        assert_eq!(editor.buffer().contents(), "hello");
    }
}
