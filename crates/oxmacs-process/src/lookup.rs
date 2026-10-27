use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

const EXTENSIONS: &str = ".COM;.EXE;.BAT;.CMD";

pub fn which(program: &str) -> Option<PathBuf> {
    find(program, env::var_os("PATH").as_deref())
}

pub fn find(program: &str, search_path: Option<&OsStr>) -> Option<PathBuf> {
    let given = Path::new(program);
    if given.components().count() > 1 {
        return runnable(given);
    }
    env::split_paths(search_path?).find_map(|directory| runnable(&directory.join(program)))
}

fn runnable(path: &Path) -> Option<PathBuf> {
    if !cfg!(windows) {
        return path.is_file().then(|| path.to_owned());
    }
    let extensions = extensions();
    let known = path.extension().is_some_and(|extension| {
        let extension = format!(".{}", extension.to_string_lossy()).to_ascii_lowercase();
        extensions.contains(&extension)
    });
    if known && path.is_file() {
        return Some(path.to_owned());
    }
    extensions.iter().find_map(|extension| {
        let mut name = OsString::from(path.as_os_str());
        name.push(extension);
        let candidate = PathBuf::from(name);
        candidate.is_file().then_some(candidate)
    })
}

fn extensions() -> Vec<String> {
    let listed = env::var("PATHEXT")
        .ok()
        .filter(|listed| !listed.trim().is_empty())
        .unwrap_or_else(|| EXTENSIONS.to_owned());
    listed
        .split(';')
        .map(str::trim)
        .filter(|extension| extension.starts_with('.') && extension.len() > 1)
        .map(str::to_ascii_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn program(name: &str) -> String {
        if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_owned()
        }
    }

    #[test]
    fn programs_are_found_on_the_search_path() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        fs::write(second.path().join(program("tool")), "").unwrap();
        let search = env::join_paths([first.path(), second.path()]).unwrap();

        assert_eq!(
            find("tool", Some(&search)),
            Some(second.path().join(program("tool")))
        );
        assert_eq!(find("other", Some(&search)), None);
        assert_eq!(find("tool", None), None);
    }

    #[test]
    fn programs_given_by_path_are_found_without_searching() {
        let dir = tempfile::tempdir().unwrap();
        let full = dir.path().join(program("tool"));
        fs::write(&full, "").unwrap();

        assert_eq!(find(full.to_str().unwrap(), None), Some(full.clone()));
        assert_eq!(
            find(dir.path().join("absent").to_str().unwrap(), None),
            None
        );
    }

    #[test]
    #[cfg(windows)]
    fn scripts_are_found_on_windows_but_extensionless_shims_are_not() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("pyright-langserver"), "#!/bin/sh").unwrap();
        fs::write(dir.path().join("pyright-langserver.cmd"), "").unwrap();
        fs::write(dir.path().join("only-a-shim"), "#!/bin/sh").unwrap();
        let search = Some(dir.path().as_os_str());

        assert_eq!(
            find("pyright-langserver", search),
            Some(dir.path().join("pyright-langserver.cmd"))
        );
        assert_eq!(
            find("pyright-langserver.cmd", search),
            Some(dir.path().join("pyright-langserver.cmd"))
        );
        assert_eq!(find("only-a-shim", search), None);
    }

    #[test]
    #[cfg(windows)]
    fn dotted_names_still_get_an_extension_on_windows() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("python3.13.exe"), "").unwrap();

        assert_eq!(
            find("python3.13", Some(dir.path().as_os_str())),
            Some(dir.path().join("python3.13.exe"))
        );
    }
}
