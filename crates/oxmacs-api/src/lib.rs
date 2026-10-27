use std::path::{Path, PathBuf};

use ox_lang::{Isolate, Stubs};

pub const DOCS: &str = include_str!("docs.ox");
pub const GAME_DOCS: &str = include_str!("games.ox");
pub const PRELUDE: &str = include_str!("prelude.ox");

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
        "Widgets for plugin views: text, rows, columns, buttons, images, canvases, book pages and PDF pages; in oxmacs only",
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
    isolate.define_stubs(DOCS, Stubs::Modules);
    isolate.define_stubs(GAME_DOCS, Stubs::Modules);
    isolate
}

pub fn editor_isolate() -> Isolate {
    let mut isolate = plain_isolate();
    isolate.define_stubs(DOCS, Stubs::Globals);
    isolate.define_stubs(GAME_DOCS, Stubs::Globals);
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
#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{check_roots, editor_code_in, editor_isolate, isolate_for};

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
