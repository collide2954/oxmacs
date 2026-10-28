use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::Config;
use crate::editor::{Editor, Effect};

impl Editor {
    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn reconfigure(&mut self, config: &Config) {
        self.apply_config(config);
        self.effects.push(Effect::Reconfigure);
    }

    pub fn init_path(&self) -> Option<&Path> {
        self.init_path.as_deref()
    }

    pub fn set_init_path(&mut self, path: Option<PathBuf>) {
        self.init_path = path;
    }

    pub(in crate::editor) fn change_config(&mut self, key: &str, value: Option<Value>) {
        match self.config.with(key, value) {
            Ok(config) => self.reconfigure(&config),
            Err(error) => self.show_message(error.to_string()),
        }
    }

    pub(in crate::editor) fn find_init_file(&mut self) {
        match self.init_path.clone() {
            Some(path) => {
                self.open_file(&path);
            }
            None => self.show_message("There is no oxmacs home folder for init.ox"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use crate::config::Config;
    use crate::editor::{Editor, Effect};
    use crate::testing::{press, type_text};

    #[test]
    fn reconfiguring_applies_at_once_and_tells_the_ui() {
        let mut editor = Editor::new();

        editor.reconfigure(&Config::default().with("tab-width", json!(4)).unwrap());

        assert_eq!(editor.tab_width(), 4);
        assert_eq!(editor.config().tab_width, 4);
        assert_eq!(editor.take_effects(), [Effect::Reconfigure]);
    }

    #[test]
    fn find_init_file_visits_init_ox() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("init.ox");
        fs::write(&path, "(settings {tab-width: 4})\n").unwrap();
        let mut editor = Editor::new();
        editor.set_init_path(Some(path.clone()));

        press(&mut editor, "M-x");
        type_text(&mut editor, "find-init-file");
        press(&mut editor, "RET");

        assert_eq!(
            editor.buffer().file().map(|file| file.path.clone()),
            Some(path)
        );
        assert_eq!(editor.buffer().contents(), "(settings {tab-width: 4})\n");
    }

    #[test]
    fn without_a_home_there_is_no_init_file_to_visit() {
        let mut editor = Editor::new();
        editor.set_init_path(None);

        press(&mut editor, "M-x");
        type_text(&mut editor, "find-init-file");
        press(&mut editor, "RET");

        assert_eq!(
            editor.message(),
            Some("There is no oxmacs home folder for init.ox")
        );
    }
}
