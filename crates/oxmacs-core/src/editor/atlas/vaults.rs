use std::fs;
use std::path::{Path, PathBuf};

use jiff::civil::Date;

use crate::atlas::note;
use crate::atlas::vault::{self, Index};
use crate::command::Command;
use crate::config::{self, AtlasVault};
use crate::editor::atlas::exact;
use crate::editor::prompts::minibuffer::Purpose;
use crate::editor::{Choice, Completion, Editor};
use crate::files::{self, paths};

const FIRST_VAULT: &str = "personal";
const FIRST_PATH: &str = "~/Atlas";

pub(super) fn note_template(title: &str) -> String {
    format!("title: {title}\n---\n= {title}\n\n")
}

fn journal_template(day: Date) -> String {
    format!("title: {day}\n---\n= {}\n\n", day.strftime("%A, %-d %B %Y"))
}

impl Editor {
    pub(super) fn vaults(&self) -> Vec<(String, PathBuf)> {
        self.config
            .atlas_vault
            .iter()
            .map(|vault| (vault.name.clone(), vault.directory(self.home.as_deref())))
            .collect()
    }

    pub(super) fn atlas_index(&mut self) -> &Index {
        let today = (self.today)();
        let vaults = self.vaults();
        self.atlas_index
            .get_or_insert_with(|| Index::build(&vaults, today))
    }

    pub(in crate::editor) fn note_atlas_saved(&mut self, path: &Path, text: &str) {
        let today = (self.today)();
        let Some(vault) = self
            .vaults()
            .into_iter()
            .find(|(_, directory)| path.starts_with(directory))
            .map(|(name, _)| name)
        else {
            return;
        };
        if let Some(index) = &mut self.atlas_index {
            index.update(vault::page(&vault, path, text, today));
        }
    }

    fn have_vault(&mut self, then: Command) -> bool {
        if self.config.atlas_vault.is_empty() {
            self.ask_to_create_vault(FIRST_VAULT, FIRST_PATH, then);
            return false;
        }
        true
    }

    pub(in crate::editor) fn create_first_vault(&mut self, then: Command) {
        let vault = AtlasVault {
            name: FIRST_VAULT.to_owned(),
            path: FIRST_PATH.to_owned(),
            color: None,
        };
        let directory = vault.directory(self.home.as_deref());
        if let Err(error) = fs::create_dir_all(directory.join(vault::JOURNAL)) {
            self.show_message(format!("{}: {error}", directory.display()));
            return;
        }
        if let Some(path) = self.config_path.clone() {
            let written = files::load(&path)
                .map_err(|error| error.to_string())
                .and_then(|loaded| {
                    let text = config::add_atlas_vault(&loaded.text, &vault)
                        .map_err(|error| error.to_string())?;
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                    }
                    files::save(&path, &text, loaded.format).map_err(|error| error.to_string())
                });
            if let Err(error) = written {
                self.show_message(error);
                return;
            }
        }
        let mut live = self.config.clone();
        live.atlas_vault.push(vault.clone());
        self.reconfigure(&live);
        self.note_saved_settings(|saved| saved.atlas_vault.push(vault));
        self.execute(then);
    }

    pub(in crate::editor) fn atlas_find_note(&mut self) {
        if self.have_vault(Command::AtlasFindNote) {
            self.atlas_index();
            self.open_minibuffer("Find note: ", Purpose::AtlasNote);
        }
    }

    pub(in crate::editor) fn atlas_note_completions(
        &self,
        matches: impl Fn(&str) -> bool,
    ) -> Vec<Completion> {
        let Some(index) = &self.atlas_index else {
            return Vec::new();
        };
        let several = self.config.atlas_vault.len() > 1;
        let mut found: Vec<(usize, &str)> = index
            .pages
            .iter()
            .enumerate()
            .filter(|(_, page)| matches(&page.title))
            .map(|(at, page)| (at, page.title.as_str()))
            .collect();
        found.sort_by_key(|(_, title)| title.to_lowercase());
        found
            .into_iter()
            .map(|(at, title)| Completion {
                label: title.to_owned(),
                annotation: several.then(|| index.pages[at].vault.clone()),
                summary: None,
                choice: Choice::Page(at),
            })
            .collect()
    }

    pub(in crate::editor) fn atlas_note_chosen(&mut self, page: Option<usize>, typed: &str) {
        let known = page.and_then(|at| {
            self.atlas_index
                .as_ref()
                .and_then(|index| index.pages.get(at))
                .map(|page| page.path.clone())
        });
        if let Some(path) = known {
            self.open_file(&path);
            return;
        }
        let title = typed.trim();
        if title.is_empty() {
            return;
        }
        let existing = self.atlas_index().find(title).map(|page| page.path.clone());
        if let Some(path) = existing {
            self.open_file(&path);
            return;
        }
        let Some((_, directory)) = self.vaults().into_iter().next() else {
            return;
        };
        self.open_or_create(&vault::page_path(&directory, title), &note_template(title));
    }

    fn open_or_create(&mut self, path: &Path, template: &str) {
        let created = !path.exists();
        if created {
            let written = path
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| fs::write(path, template));
            if let Err(error) = written {
                self.show_message(format!("{}: {error}", path.display()));
                return;
            }
            self.note_atlas_saved(path, template);
        }
        if self.open_file(path) && created {
            let buffer = self.buffers.current_mut();
            buffer.set_point(buffer.len_chars().saturating_sub(1));
        }
    }

    fn journal(&self) -> Option<PathBuf> {
        let (_, directory) = self.vaults().into_iter().next()?;
        Some(vault::journal_path(&directory, (self.today)()))
    }

    pub(in crate::editor) fn atlas_today(&mut self) {
        if !self.have_vault(Command::AtlasToday) {
            return;
        }
        if let Some(path) = self.journal() {
            self.open_or_create(&path, &journal_template((self.today)()));
        }
    }

    pub(in crate::editor) fn atlas_capture(&mut self) {
        if self.have_vault(Command::AtlasCapture) {
            self.open_minibuffer("Capture: ", Purpose::AtlasCapture);
        }
    }

    pub(in crate::editor) fn captured(&mut self, text: &str) {
        let text = text.trim();
        let Some(path) = self.journal().filter(|_| !text.is_empty()) else {
            return;
        };
        let today = (self.today)();
        let task = !note::parse(text, today).tasks.is_empty();
        let line = if task {
            exact(text, today).unwrap_or_else(|| text.to_owned())
        } else {
            format!("- {}  {text}", (self.time_now)().strftime("%H:%M"))
        };
        let shown = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let open = self
            .buffers
            .position(|buffer| buffer.file().is_some_and(|file| file.path == path));
        let result = match open {
            Some(index) => self.capture_into_buffer(index, &line),
            None => self.capture_into_file(&path, &line, today),
        };
        match result {
            Ok(()) => self.show_message(format!("Captured to {shown}")),
            Err(error) => self.show_message(error),
        }
    }

    fn capture_into_buffer(&mut self, index: usize, line: &str) -> Result<(), String> {
        let Some(buffer) = self.buffers.get_mut(index) else {
            return Ok(());
        };
        let end = buffer.len_chars();
        let separator = if buffer.contents().ends_with('\n') || end == 0 {
            ""
        } else {
            "\n"
        };
        buffer.insert_at(end, &format!("{separator}{line}\n"));
        if self.save_buffer_at(index) {
            Ok(())
        } else {
            Err("Could not save the journal".to_owned())
        }
    }

    fn capture_into_file(&mut self, path: &Path, line: &str, today: Date) -> Result<(), String> {
        let mut text = fs::read_to_string(path).unwrap_or_else(|_| journal_template(today));
        if !text.ends_with('\n') && !text.is_empty() {
            text.push('\n');
        }
        text.push_str(line);
        text.push('\n');
        path.parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(path, &text))
            .map_err(|error| {
                format!("{}: {error}", paths::abbreviate(path, self.home.as_deref()))
            })?;
        self.note_atlas_saved(path, &text);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use jiff::civil::{date, time};

    use crate::config::Config;
    use crate::editor::Editor;
    use crate::editor::test_support::{lines, press, question, type_text};

    struct Setup {
        _home: tempfile::TempDir,
        root: PathBuf,
        config: PathBuf,
        editor: Editor,
    }

    fn with_vault(vault: bool) -> Setup {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("Atlas");
        let config = home.path().join("config.toml");
        let text = if vault {
            format!(
                "[[atlas-vault]]\nname = \"personal\"\npath = \"{}\"\n",
                root.display().to_string().replace('\\', "/")
            )
        } else {
            String::new()
        };
        fs::write(&config, &text).unwrap();
        if vault {
            fs::create_dir_all(&root).unwrap();
            fs::write(root.join("Garden plan.atlas"), "title: Garden plan\n---\n").unwrap();
        }
        let mut editor = Editor::new();
        editor.apply_config(&Config::parse(&text).unwrap());
        editor.config_path = Some(config.clone());
        editor.home = Some(home.path().to_owned());
        editor.today = || date(2026, 10, 19);
        editor.time_now = || time(9, 30, 0, 0);
        Setup {
            _home: home,
            root,
            config,
            editor,
        }
    }

    fn journal(root: &Path) -> PathBuf {
        root.join("journal").join("2026-10-19.atlas")
    }

    #[test]
    fn the_first_atlas_key_offers_to_make_a_vault() {
        let mut setup = with_vault(false);

        press(&mut setup.editor, "C-c n f");
        assert_eq!(
            question(&setup.editor).as_deref(),
            Some("No Atlas vault yet; create personal at ~/Atlas? (y or n) ")
        );
        press(&mut setup.editor, "y");

        assert!(setup.root.join("journal").is_dir());
        let saved = Config::load(&setup.config).unwrap();
        assert_eq!(saved.atlas_vault[0].name, "personal");
        assert_eq!(saved.atlas_vault[0].path, "~/Atlas");
        assert_eq!(setup.editor.config().atlas_vault, saved.atlas_vault);
        assert_eq!(setup.editor.minibuffer().unwrap().prompt(), "Find note: ");
    }

    #[test]
    fn find_note_opens_a_page_by_title() {
        let mut setup = with_vault(true);

        press(&mut setup.editor, "C-c n f");
        type_text(&mut setup.editor, "gard");
        press(&mut setup.editor, "RET");

        assert_eq!(setup.editor.buffer().name(), "Garden plan.atlas");
    }

    #[test]
    fn find_note_makes_a_page_for_a_new_title() {
        let mut setup = with_vault(true);

        press(&mut setup.editor, "C-c n f");
        type_text(&mut setup.editor, "Seed catalog");
        press(&mut setup.editor, "RET");

        let path = setup.root.join("Seed catalog.atlas");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "title: Seed catalog\n---\n= Seed catalog\n\n"
        );
        assert_eq!(setup.editor.buffer().name(), "Seed catalog.atlas");
        assert_eq!(setup.editor.buffer().point(), 39);

        press(&mut setup.editor, "C-c n f");
        type_text(&mut setup.editor, "seed");
        assert_eq!(
            crate::editor::test_support::labels(&setup.editor),
            ["Seed catalog"]
        );
    }

    #[test]
    fn m_ret_makes_a_page_even_when_another_title_matches() {
        let mut setup = with_vault(true);

        press(&mut setup.editor, "C-c n f");
        type_text(&mut setup.editor, "Garden");
        press(&mut setup.editor, "M-RET");

        assert_eq!(setup.editor.buffer().name(), "Garden.atlas");
    }

    #[test]
    fn today_opens_the_journal_page() {
        let mut setup = with_vault(true);

        press(&mut setup.editor, "C-c n t");

        assert_eq!(setup.editor.buffer().name(), "2026-10-19.atlas");
        assert_eq!(
            lines(&setup.editor),
            ["title: 2026-10-19", "---", "= Monday, 19 October 2026", ""]
        );
    }

    #[test]
    fn capture_appends_to_the_journal_without_leaving_the_buffer() {
        let mut setup = with_vault(true);

        press(&mut setup.editor, "C-c n c");
        type_text(&mut setup.editor, "Call Ana about seeds");
        press(&mut setup.editor, "RET");
        press(&mut setup.editor, "C-c n c");
        type_text(&mut setup.editor, "[ ] Buy seeds due:fri");
        press(&mut setup.editor, "RET");

        assert_eq!(setup.editor.buffer().name(), "*scratch*");
        assert_eq!(setup.editor.message(), Some("Captured to 2026-10-19"));
        assert_eq!(
            fs::read_to_string(journal(&setup.root)).unwrap(),
            "title: 2026-10-19\n---\n= Monday, 19 October 2026\n\n- 09:30  Call Ana about seeds\n[ ] Buy seeds due:2026-10-23\n"
        );
    }

    #[test]
    fn capture_goes_into_an_open_journal_and_saves_it() {
        let mut setup = with_vault(true);
        press(&mut setup.editor, "C-c n t");

        press(&mut setup.editor, "C-c n c");
        type_text(&mut setup.editor, "Water plants");
        press(&mut setup.editor, "RET");

        assert!(!setup.editor.buffer().is_modified());
        assert!(
            fs::read_to_string(journal(&setup.root))
                .unwrap()
                .ends_with("\n- 09:30  Water plants\n")
        );
    }
}
