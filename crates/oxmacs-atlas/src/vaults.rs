use std::fs;
use std::path::{Path, PathBuf};

use jiff::civil::Date;
use oxmacs_core::editor::{Editor, Offer, Picked};
use oxmacs_core::files::paths;
use oxmacs_core::text::BufferId;

use crate::model::edit::exact;
use crate::model::note;
use crate::model::vault::{self, Index};
use crate::notes::{Notebook, capture_style, notebook, settings};
use crate::settings::{Vault, add_vault};

const FIRST_VAULT: &str = "personal";
const FIRST_PATH: &str = "~/Atlas";

pub(crate) fn note_template(title: &str) -> String {
    format!("title: {title}\n---\n= {title}\n\n")
}

fn journal_template(day: Date) -> String {
    format!("title: {day}\n---\n= {}\n\n", day.strftime("%A, %-d %B %Y"))
}

fn sorted(index: &Index) -> Vec<usize> {
    let mut found: Vec<usize> = (0..index.pages.len()).collect();
    found.sort_by_key(|at| index.pages[*at].title.to_lowercase());
    found
}

fn offers(editor: &Editor) -> Vec<Offer> {
    let several = settings(editor).vault.len() > 1;
    let Some(index) = editor
        .applet_state::<crate::notes::AtlasState>()
        .and_then(|state| state.index.as_ref())
    else {
        return Vec::new();
    };
    sorted(index)
        .into_iter()
        .map(|at| {
            let page = &index.pages[at];
            Offer {
                label: page.title.clone(),
                note: several.then(|| page.vault.clone()),
            }
        })
        .collect()
}

impl Notebook<'_> {
    fn have_vault(&mut self, then: fn(&mut Editor)) -> bool {
        if !self.vaults().is_empty() {
            return true;
        }
        self.ask(
            &format!("No Atlas vault yet; create {FIRST_VAULT} at {FIRST_PATH}?"),
            move |editor, yes| {
                if yes && notebook(editor, Notebook::create_first_vault) == Some(true) {
                    then(editor);
                }
            },
        );
        false
    }

    fn create_first_vault(&mut self) -> bool {
        let vault = Vault {
            name: FIRST_VAULT.to_owned(),
            path: FIRST_PATH.to_owned(),
            color: None,
        };
        let directory = vault.directory(self.home());
        if let Err(error) = fs::create_dir_all(directory.join(vault::JOURNAL)) {
            self.show_message(format!("{}: {error}", directory.display()));
            return false;
        }
        if let Err(error) = self.edit_config_file(|document| add_vault(document, &vault)) {
            self.show_message(error);
            return false;
        }
        self.configured();
        true
    }

    pub(crate) fn find_note(&mut self) {
        if self.have_vault(|editor| {
            notebook(editor, Notebook::find_note);
        }) {
            self.index();
            self.prompt_pick("Find note: ", offers, |editor, picked| {
                notebook(editor, |book| book.note_chosen(picked));
            });
        }
    }

    fn note_chosen(&mut self, picked: Picked) {
        let typed = match picked {
            Picked::Offered(at) => {
                let index = self.index();
                let path = sorted(index)
                    .get(at)
                    .and_then(|page| index.pages.get(*page))
                    .map(|page| page.path.clone());
                if let Some(path) = path {
                    self.open_file(&path);
                }
                return;
            }
            Picked::Typed(typed) => typed,
        };
        let title = typed.trim();
        if title.is_empty() {
            return;
        }
        let existing = self.index().find(title).map(|page| page.path.clone());
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
            self.note_saved(path, template);
        }
        if self.open_file(path) && created {
            let buffer = self.buffer_mut();
            buffer.set_point(buffer.len_chars().saturating_sub(1));
        }
    }

    fn journal(&self) -> Option<PathBuf> {
        let (_, directory) = self.vaults().into_iter().next()?;
        Some(vault::journal_path(&directory, self.today()))
    }

    pub(crate) fn today_page(&mut self) {
        if !self.have_vault(|editor| {
            notebook(editor, Notebook::today_page);
        }) {
            return;
        }
        if let Some(path) = self.journal() {
            let template = journal_template(self.today());
            self.open_or_create(&path, &template);
        }
    }

    pub(crate) fn capture(&mut self) {
        if self.have_vault(|editor| {
            notebook(editor, Notebook::capture);
        }) {
            self.prompt_styled("Capture: ", capture_style, |editor, text| {
                notebook(editor, |book| book.captured(&text));
            });
        }
    }

    fn captured(&mut self, text: &str) {
        let text = text.trim();
        let Some(path) = self.journal().filter(|_| !text.is_empty()) else {
            return;
        };
        let today = self.today();
        let task = !note::parse(text, today).tasks.is_empty();
        let line = if task {
            exact(text, today).unwrap_or_else(|| text.to_owned())
        } else {
            format!("- {}  {text}", (self.atlas.time_now)().strftime("%H:%M"))
        };
        let shown = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let result = match self.buffer_visiting(&path) {
            Some(id) => self.capture_into_buffer(id, &line),
            None => self.capture_into_file(&path, &line, today),
        };
        match result {
            Ok(()) => self.show_message(format!("Captured to {shown}")),
            Err(error) => self.show_message(error),
        }
    }

    fn capture_into_buffer(&mut self, id: BufferId, line: &str) -> Result<(), String> {
        let Some(buffer) = self.buffer_by_id_mut(id) else {
            return Ok(());
        };
        let end = buffer.len_chars();
        let separator = if buffer.contents().ends_with('\n') || end == 0 {
            ""
        } else {
            "\n"
        };
        buffer.insert_at(end, &format!("{separator}{line}\n"));
        if self.save(id) {
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
            .map_err(|error| format!("{}: {error}", paths::abbreviate(path, self.home())))?;
        self.note_saved(path, &text);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use oxmacs_core::config::Config;
    use oxmacs_core::editor::Editor;
    use oxmacs_core::theme::Highlight;

    use crate::settings::{AtlasSettings, TABLE};
    use crate::testing::{labels, lines, press, question, type_text, vault_config};

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
            vault_config(&[("personal", &root)])
        } else {
            String::new()
        };
        fs::write(&config, &text).unwrap();
        if vault {
            fs::create_dir_all(&root).unwrap();
            fs::write(root.join("Garden plan.atlas"), "title: Garden plan\n---\n").unwrap();
        }
        let mut editor = crate::testing::configured(&text);
        editor.set_config_path(Some(config.clone()));
        editor.set_home(Some(home.path().to_owned()));
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
        let saved = Config::load(&setup.config)
            .unwrap()
            .applet::<AtlasSettings>(TABLE)
            .unwrap();
        assert_eq!(saved.vault[0].name, "personal");
        assert_eq!(saved.vault[0].path, "~/Atlas");
        assert_eq!(
            setup.editor.applet_config::<AtlasSettings>(TABLE).vault,
            saved.vault
        );
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
        assert_eq!(labels(&setup.editor), ["Seed catalog"]);
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
    fn notes_sharing_a_title_open_from_their_own_vault() {
        let home = tempfile::tempdir().unwrap();
        let personal = home.path().join("personal");
        let work = home.path().join("work");
        crate::testing::write(&personal.join("Inbox.atlas"), "personal inbox\n");
        crate::testing::write(&work.join("Inbox.atlas"), "work inbox\n");
        let mut editor =
            crate::testing::configured(&vault_config(&[("personal", &personal), ("work", &work)]));

        press(&mut editor, "C-c n f");
        type_text(&mut editor, "inbox");
        press(&mut editor, "C-n RET");

        assert_eq!(lines(&editor), ["work inbox"]);
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
    fn capture_input_is_highlighted_like_a_note() {
        let mut setup = with_vault(true);
        assert_eq!(setup.editor.minibuffer_highlights(), []);

        press(&mut setup.editor, "C-c n c");
        type_text(&mut setup.editor, "[ ] Buy seeds  due:fri  #home");

        let highlights = setup.editor.minibuffer_highlights();
        assert_eq!(highlights[0], (0..3, Highlight::Function));
        assert!(
            highlights.contains(&(15..22, Highlight::Warning)),
            "{highlights:?}"
        );
        assert!(
            highlights.contains(&(24..29, Highlight::Type)),
            "{highlights:?}"
        );
        press(&mut setup.editor, "C-g C-c n f");
        assert_eq!(setup.editor.minibuffer_highlights(), []);
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
