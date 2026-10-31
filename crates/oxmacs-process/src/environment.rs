use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{PoisonError, RwLock};

use oxmacs_files::paths;

type Variables = Vec<(String, String)>;

static FOLDERS: RwLock<Vec<(PathBuf, Variables)>> = RwLock::new(Vec::new());

pub fn set_environment(folder: &Path, variables: Variables) {
    let folder = paths::absolute(folder);
    let mut folders = FOLDERS.write().unwrap_or_else(PoisonError::into_inner);
    folders.retain(|(known, _)| *known != folder);
    folders.push((folder, variables));
}

pub fn clear_environment(folder: &Path) {
    let folder = paths::absolute(folder);
    FOLDERS
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .retain(|(known, _)| *known != folder);
}

pub fn environment_for(directory: &Path) -> Variables {
    if directory.as_os_str().is_empty() {
        return Vec::new();
    }
    let directory = paths::absolute(directory);
    FOLDERS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|(folder, _)| paths::inside(&directory, folder))
        .max_by_key(|(folder, _)| folder.components().count())
        .map(|(_, variables)| variables.clone())
        .unwrap_or_default()
}

pub fn search_path_for(directory: &Path) -> Option<OsString> {
    environment_for(directory)
        .into_iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("PATH"))
        .map(|(_, value)| OsString::from(value))
        .or_else(|| env::var_os("PATH"))
}

pub fn which_in(program: &str, directory: &Path) -> Option<PathBuf> {
    crate::find(program, search_path_for(directory).as_deref())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn programs_in_a_folder_get_its_innermost_environment() {
        let outer = tempfile::tempdir().unwrap();
        let inner = outer.path().join("inner");
        fs::create_dir(&inner).unwrap();
        set_environment(
            outer.path(),
            vec![("PIXI_ENVIRONMENT_NAME".to_owned(), "default".to_owned())],
        );
        set_environment(
            &inner,
            vec![("PIXI_ENVIRONMENT_NAME".to_owned(), "cuda".to_owned())],
        );

        assert_eq!(
            environment_for(&inner.join("src")),
            [("PIXI_ENVIRONMENT_NAME".to_owned(), "cuda".to_owned())]
        );
        assert_eq!(
            environment_for(outer.path()),
            [("PIXI_ENVIRONMENT_NAME".to_owned(), "default".to_owned())]
        );
        clear_environment(&inner);
        assert_eq!(
            environment_for(&inner),
            [("PIXI_ENVIRONMENT_NAME".to_owned(), "default".to_owned())]
        );
        clear_environment(outer.path());
        assert_eq!(environment_for(&inner), []);
        assert_eq!(environment_for(Path::new("")), []);
    }

    #[test]
    fn programs_are_looked_up_on_the_folders_own_path() {
        let project = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) {
            "only-here.exe"
        } else {
            "only-here"
        };
        fs::write(bin.path().join(name), "").unwrap();
        set_environment(
            project.path(),
            vec![("PATH".to_owned(), bin.path().display().to_string())],
        );

        assert_eq!(
            which_in("only-here", &project.path().join("src")),
            Some(bin.path().join(name))
        );
        assert_eq!(which_in("only-here", bin.path()), None);
        clear_environment(project.path());
    }
}
