use std::path::{self, Path, PathBuf};
use std::{fs, io};

pub const SEPARATOR: char = path::MAIN_SEPARATOR;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
}

pub fn is_separator(c: char) -> bool {
    path::is_separator(c)
}

pub fn split(input: &str) -> (&str, &str) {
    match input.rfind(is_separator) {
        Some(index) => input.split_at(index + 1),
        None => ("", input),
    }
}

pub fn expand(input: &str, home: Option<&Path>) -> PathBuf {
    if let (Some(rest), Some(home)) = (input.strip_prefix('~'), home)
        && (rest.is_empty() || rest.starts_with(is_separator))
    {
        return home.join(rest.trim_start_matches(is_separator));
    }
    PathBuf::from(input)
}

pub fn abbreviate(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~{SEPARATOR}{}", rest.display()),
        None => path.display().to_string(),
    }
}

pub fn list_dir(dir: &Path) -> io::Result<Vec<DirEntry>> {
    let mut entries: Vec<DirEntry> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| DirEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir: entry.path().is_dir(),
        })
        .collect();
    entries.sort_by_cached_key(|entry| {
        (
            !entry.is_dir,
            entry.name.starts_with('.'),
            entry.name.to_lowercase(),
        )
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    #[test]
    fn input_splits_at_the_last_separator() {
        assert_eq!(split("~/mx/cr"), ("~/mx/", "cr"));
        assert_eq!(split("dir/"), ("dir/", ""));
        assert_eq!(split("abc"), ("", "abc"));
    }

    #[test]
    fn a_leading_tilde_expands_to_home() {
        let home = Path::new("/home/u");

        assert_eq!(expand("~/x/y", Some(home)), home.join("x/y"));
        assert_eq!(expand("~", Some(home)), home);
        assert_eq!(expand("~other/x", Some(home)), Path::new("~other/x"));
        assert_eq!(expand("~/x", None), Path::new("~/x"));
        assert_eq!(expand("/abs/x", Some(home)), Path::new("/abs/x"));
    }

    #[test]
    fn paths_under_home_abbreviate_to_a_tilde() {
        let home = Path::new("/home/u");

        assert_eq!(
            abbreviate(&home.join("mx"), Some(home)),
            format!("~{SEPARATOR}mx")
        );
        assert_eq!(abbreviate(home, Some(home)), "~");
        assert_eq!(
            abbreviate(Path::new("/srv/x"), Some(home)),
            Path::new("/srv/x").display().to_string()
        );
    }

    #[test]
    fn listings_put_directories_first_and_dotfiles_last() {
        let dir = tempfile::tempdir().unwrap();
        for file in ["b.txt", "A.txt", ".hidden"] {
            fs::write(dir.path().join(file), "").unwrap();
        }
        for subdir in ["zdir", ".git"] {
            fs::create_dir(dir.path().join(subdir)).unwrap();
        }

        let entries = list_dir(dir.path()).unwrap();

        let names: Vec<(&str, bool)> = entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.is_dir))
            .collect();
        assert_eq!(
            names,
            [
                ("zdir", true),
                (".git", true),
                ("A.txt", false),
                ("b.txt", false),
                (".hidden", false),
            ]
        );
    }
}
