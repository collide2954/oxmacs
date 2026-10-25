use std::path::{Path, PathBuf};

use ox::Isolate;

pub const DOCS: &str = include_str!("docs.ox");
pub const PRELUDE: &str = include_str!("prelude.ox");

pub fn editor_code(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "init.ox")
        || path
            .parent()
            .is_some_and(|folder| folder.join("plugin.ox").is_file())
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

pub fn extend_with_prelude(isolate: &mut Isolate) {
    if let Err(error) = isolate.extend_prelude("oxmacs/prelude.ox", PRELUDE) {
        panic!("The oxmacs prelude failed to load: {error}");
    }
}

pub fn checking_isolate() -> Isolate {
    let mut isolate = Isolate::new();
    isolate.define_stubs(DOCS);
    extend_with_prelude(&mut isolate);
    isolate
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{check_roots, checking_isolate, editor_code};

    #[test]
    fn checking_editor_code_knows_the_editor_api_without_the_editor() {
        let mut isolate = checking_isolate();

        let found: Vec<String> = isolate
            .check(
                "plugin.ox",
                "(module (import (ox ui)))\n(defcommand hi \"Hi\" (message (ui/text \"x\")))\n(open-buffer)",
            )
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect();

        assert_eq!(found, ["open-buffer takes at least 2 arguments, got 0"]);
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

        assert!(editor_code(&dir.path().join("init.ox")));
        assert!(editor_code(&plugin.join("hello.ox")));
        assert!(!editor_code(&dir.path().join("notes.ox")));
        assert_eq!(
            check_roots(&plugin.join("hello.ox")),
            [plugin.clone(), dir.path().join("plugins")]
        );
    }
}
