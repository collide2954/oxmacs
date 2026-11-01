use std::path::{Path, PathBuf};

use ox_lang::{Isolate, Stubs};

pub const DOCS: &str = include_str!("docs.ox");
pub const GAME_DOCS: &str = include_str!("games.ox");
pub const SOUND_DOCS: &str = include_str!("sound.ox");
pub const PRELUDE: &str = include_str!("prelude.ox");
pub const WAITING: &[&str] = &[
    "delete-path",
    "keepass/read",
    "move-path",
    "ox-evaluate",
    "ox-reference",
    "pdf/open",
    "pdf/search",
    "prompt",
    "prompt-path",
    "yes-or-no",
];

pub fn home() -> Option<PathBuf> {
    oxmacs_files::oxmacs_home()
}

pub fn editor_code(path: &Path) -> bool {
    editor_code_in(path, home().as_deref())
}

pub fn editor_code_in(path: &Path, home: Option<&Path>) -> bool {
    path.file_name()
        .is_some_and(|name| name == "init.ox" || name == "custom.ox")
        || path
            .parent()
            .is_some_and(|folder| folder.join("plugin.ox").is_file())
        || home.is_some_and(|home| path.starts_with(home))
}

pub fn check_roots(path: &Path) -> Vec<PathBuf> {
    let Some(folder) = path.parent() else {
        return Vec::new();
    };
    let mut roots = vec![folder.to_path_buf()];
    if folder.join("plugin.ox").is_file()
        && let Some(plugins) = folder.parent()
    {
        roots.push(plugins.to_path_buf());
    }
    roots
}

const EDITOR_MODULES: &[(&str, &str)] = &[
    (
        "ox/pdf",
        "PDF documents drawn by PDFium: find the library, open and search them; in oxmacs only",
    ),
    (
        "ox/keepass",
        "KeePass databases: reading their entries; in oxmacs only",
    ),
    (
        "ox/ui",
        "Widgets for plugin views: text, rows, columns, buttons, sliders, images, canvases, book pages and PDF pages; in oxmacs only",
    ),
    (
        "ox/rodeo",
        "Games played with rodeo-play: drawing, images, keys, actions and game time; in oxmacs only",
    ),
];

pub fn add_editor_modules(isolate: &mut Isolate) {
    for (name, summary) in EDITOR_MODULES {
        isolate.add_module(name, summary);
    }
}

pub fn extend_with_prelude(isolate: &mut Isolate) {
    if let Err(error) = isolate.extend_prelude("oxmacs/prelude.ox", PRELUDE) {
        panic!("The oxmacs prelude failed to load: {error}");
    }
}

pub fn plain_isolate() -> Isolate {
    let mut isolate = ox_library::isolate();
    add_editor_modules(&mut isolate);
    isolate.define_stubs(DOCS, Stubs::Modules, WAITING);
    isolate.define_stubs(GAME_DOCS, Stubs::Modules, WAITING);
    isolate.define_stubs(SOUND_DOCS, Stubs::Modules, WAITING);
    isolate
}

pub fn editor_isolate() -> Isolate {
    let mut isolate = plain_isolate();
    isolate.define_stubs(DOCS, Stubs::Globals, WAITING);
    isolate.define_stubs(GAME_DOCS, Stubs::Globals, WAITING);
    isolate.define_stubs(SOUND_DOCS, Stubs::Globals, WAITING);
    extend_with_prelude(&mut isolate);
    isolate
}

pub fn isolate_for(path: &Path) -> Isolate {
    let mut isolate = if editor_code(path) {
        editor_isolate()
    } else {
        plain_isolate()
    };
    for root in check_roots(path) {
        isolate.add_root(root);
    }
    isolate
}

const KEPT: usize = 4;

#[derive(Default)]
pub struct Isolates {
    kept: Vec<(bool, Vec<PathBuf>, Isolate)>,
}

impl std::fmt::Debug for Isolates {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_list()
            .entries(self.kept.iter().map(|(editor, roots, _)| (editor, roots)))
            .finish()
    }
}

impl Isolates {
    pub fn get(&mut self, editor: bool, roots: Vec<PathBuf>) -> &mut Isolate {
        match self
            .kept
            .iter()
            .position(|(known, found, _)| *known == editor && *found == roots)
        {
            Some(at) => {
                let found = self.kept.remove(at);
                self.kept.push(found);
            }
            None => {
                let mut isolate = if editor {
                    editor_isolate()
                } else {
                    plain_isolate()
                };
                for root in &roots {
                    isolate.add_root(root.clone());
                }
                if self.kept.len() >= KEPT {
                    self.kept.remove(0);
                }
                self.kept.push((editor, roots, isolate));
            }
        }
        let last = self.kept.len() - 1;
        &mut self.kept[last].2
    }
}
#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{Isolates, check_roots, editor_code_in, editor_isolate, isolate_for};

    #[test]
    fn files_in_two_folders_each_find_their_own_module_of_the_same_name() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        fs::write(
            first.path().join("util.ox"),
            "(module (export one))\n(def one 1)",
        )
        .unwrap();
        fs::write(
            second.path().join("util.ox"),
            "(module (export two))\n(def two 2)",
        )
        .unwrap();
        let mut isolates = Isolates::default();

        for (folder, used) in [
            (&first, "util/one"),
            (&second, "util/two"),
            (&first, "util/one"),
        ] {
            let found: Vec<String> = isolates
                .get(false, vec![folder.path().to_owned()])
                .check("main.ox", &format!("(module (import (util)))\n{used}"))
                .into_iter()
                .map(|diagnostic| diagnostic.message)
                .collect();
            assert_eq!(found, Vec::<String>::new(), "{used}");
        }
    }

    fn problems(path: &Path, source: &str) -> Vec<String> {
        isolate_for(path)
            .check("game.ox", source)
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    #[test]
    fn plain_files_and_games_check_with_the_editor_modules_but_not_the_editor() {
        let dir = tempfile::tempdir().unwrap();

        let found = problems(
            &dir.path().join("game.ox"),
            "(module (import (ox rodeo) (ox geometry) (ox ui)))\n(def (draw) (rodeo/rect [0 0 1 1] \"#fff\") (geometry/add [1 2] [3 4]) (ui/text \"x\"))\n(message \"hi\")",
        );

        assert_eq!(found, ["message is not defined"]);
    }

    #[test]
    fn editor_code_checks_with_the_editor_api_and_its_modules() {
        let dir = tempfile::tempdir().unwrap();

        let found = problems(
            &dir.path().join("init.ox"),
            "(module (import (ox rodeo)))\n(message (str (rodeo/held? :left)))",
        );

        assert_eq!(found, Vec::<String>::new());
    }

    #[test]
    fn init_custom_plugin_and_home_files_are_editor_code() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");

        assert!(editor_code_in(&dir.path().join("custom.ox"), None));
        assert!(editor_code_in(
            &home.join("notes").join("tools.ox"),
            Some(&home)
        ));
        assert!(!editor_code_in(&dir.path().join("tools.ox"), Some(&home)));
    }

    #[test]
    fn checking_editor_code_knows_the_editor_api_without_the_editor() {
        let mut isolate = editor_isolate();

        let found: Vec<String> = isolate
            .check(
                "plugin.ox",
                "(module (import (ox ui)))\n(defcommand hi \"Hi\" (message (ui/text \"x\")))\n(open-buffer)",
            )
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect();

        assert_eq!(found, ["open-buffer takes 2 arguments, got 0"]);
        assert_eq!(
            isolate.eval("(message \"hi\")").unwrap_err().message,
            "message isn't available here"
        );
    }

    #[test]
    fn checking_editor_code_knows_which_editor_calls_wait_for_their_result() {
        let found: Vec<String> = editor_isolate()
            .check(
                "plugin.ox",
                "(module (import (ox pdf)))\n(def (f) [(await (prompt \"x\")) (await (pdf/open \"a.pdf\")) (await (spawn 1))])",
            )
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect();

        assert_eq!(
            found,
            [
                "prompt already waits for its result; drop the await",
                "pdf/open already waits for its result; drop the await"
            ]
        );
    }

    #[test]
    fn init_files_and_plugin_folders_hold_editor_code() {
        let dir = tempfile::tempdir().unwrap();
        let plugin = dir.path().join("plugins").join("hello");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(plugin.join("plugin.ox"), "(plugin hello)").unwrap();

        assert!(editor_code_in(&dir.path().join("init.ox"), None));
        assert!(editor_code_in(&plugin.join("hello.ox"), None));
        assert!(!editor_code_in(&dir.path().join("notes.ox"), None));
        assert_eq!(
            check_roots(&plugin.join("hello.ox")),
            [plugin.clone(), dir.path().join("plugins")]
        );
    }
}
