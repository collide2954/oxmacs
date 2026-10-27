use std::path::{self, Component, Path, PathBuf};

pub const SEPARATOR: char = path::MAIN_SEPARATOR;

pub fn is_separator(c: char) -> bool {
    path::is_separator(c)
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

pub fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    normalized.pop();
                } else if !normalized.has_root() {
                    normalized.push(component);
                }
            }
            _ => normalized.push(component),
        }
    }
    normalized
}

pub fn absolute(path: &Path) -> PathBuf {
    normalize(&path::absolute(path).unwrap_or_else(|_| path.to_owned()))
}

fn folded(component: Component<'_>) -> String {
    let text = component.as_os_str().to_string_lossy();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text.into_owned()
    }
}

fn same_parts(first: &[Component<'_>], second: &[Component<'_>]) -> usize {
    first
        .iter()
        .zip(second)
        .take_while(|(a, b)| folded(**a) == folded(**b))
        .count()
}

pub fn inside(path: &Path, folder: &Path) -> bool {
    let (path, folder) = (normalize(path), normalize(folder));
    let path: Vec<_> = path.components().collect();
    let folder: Vec<_> = folder.components().collect();
    folder.len() <= path.len() && same_parts(&path, &folder) == folder.len()
}

pub fn relative(path: &Path, base: &Path) -> Option<PathBuf> {
    let (path, base) = (normalize(path), normalize(base));
    let path: Vec<_> = path.components().collect();
    let base: Vec<_> = base.components().collect();
    let shared = same_parts(&path, &base);
    let rooted = |parts: &[Component<'_>]| {
        parts
            .first()
            .is_some_and(|first| matches!(first, Component::Prefix(_) | Component::RootDir))
    };
    if shared == 0 && (rooted(&path) || rooted(&base)) {
        return None;
    }
    let mut found = PathBuf::new();
    for part in &base[shared..] {
        if matches!(part, Component::ParentDir) {
            return None;
        }
        found.push("..");
    }
    for part in &path[shared..] {
        found.push(part);
    }
    if found.as_os_str().is_empty() {
        found.push(".");
    }
    Some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rooted(parts: &[&str]) -> PathBuf {
        let mut path = PathBuf::from(if cfg!(windows) { r"C:\" } else { "/" });
        path.extend(parts);
        path
    }

    #[test]
    fn a_leading_tilde_expands_to_home_but_another_users_does_not() {
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
            abbreviate(&home.join("oxmacs"), Some(home)),
            format!("~{SEPARATOR}oxmacs")
        );
        assert_eq!(abbreviate(home, Some(home)), "~");
        assert_eq!(
            abbreviate(Path::new("/srv/x"), Some(home)),
            Path::new("/srv/x").display().to_string()
        );
    }

    #[test]
    fn normalizing_resolves_dots_lexically() {
        let path = rooted(&["a", "b", "..", ".", "c"]);

        assert_eq!(normalize(&path), rooted(&["a", "c"]));
        assert_eq!(normalize(&rooted(&["..", "x"])), rooted(&["x"]));
        assert_eq!(normalize(Path::new("../a/./b/..")), Path::new("../a"));
    }

    #[test]
    fn paths_inside_a_folder_are_told_apart_from_lookalikes() {
        let folder = rooted(&["work", "app"]);

        assert!(inside(&rooted(&["work", "app", "src", "a.ox"]), &folder));
        assert!(inside(&folder, &folder));
        assert!(inside(&rooted(&["work", "app", "..", "app", "b"]), &folder));
        assert!(!inside(&rooted(&["work", "apple", "a"]), &folder));
        assert!(!inside(&rooted(&["work"]), &folder));
        assert_eq!(
            inside(&rooted(&["WORK", "APP", "a"]), &folder),
            cfg!(windows)
        );
        let root = rooted(&[]);
        assert!(inside(&rooted(&["any"]), &root));
    }

    #[test]
    fn relative_paths_climb_out_of_the_base_when_needed() {
        let base = rooted(&["work", "app"]);

        assert_eq!(
            relative(&rooted(&["work", "app", "src", "a.ox"]), &base),
            Some(Path::new("src").join("a.ox"))
        );
        assert_eq!(
            relative(&rooted(&["work", "lib", "b.ox"]), &base),
            Some(Path::new("..").join("lib").join("b.ox"))
        );
        assert_eq!(relative(&base, &base), Some(PathBuf::from(".")));
        assert_eq!(
            relative(Path::new("a/b"), Path::new("a")),
            Some(PathBuf::from("b"))
        );
        if cfg!(windows) {
            assert_eq!(relative(Path::new(r"D:\x"), Path::new(r"C:\y")), None);
        }
    }
}
