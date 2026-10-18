use std::path::{self, Component, Path, PathBuf};

pub const SEPARATOR: char = path::MAIN_SEPARATOR;

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

pub fn unique_names(paths: &[PathBuf]) -> Vec<String> {
    let parts: Vec<Vec<String>> = paths
        .iter()
        .map(|path| {
            path.components()
                .rev()
                .filter_map(|component| match component {
                    Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect()
        })
        .collect();
    let mut depths = vec![0; paths.len()];
    loop {
        let key = |index: usize| {
            &parts[index][..=depths[index].min(parts[index].len().saturating_sub(1))]
        };
        let deeper: Vec<usize> = (0..paths.len())
            .filter(|&index| {
                depths[index] + 1 < parts[index].len()
                    && (0..paths.len()).any(|other| other != index && key(other) == key(index))
            })
            .collect();
        if deeper.is_empty() {
            break;
        }
        for index in deeper {
            depths[index] += 1;
        }
    }
    parts
        .iter()
        .zip(depths)
        .map(|(parts, depth)| {
            let name = parts.first().cloned().unwrap_or_default();
            if depth == 0 {
                return name;
            }
            let parents: Vec<&str> = parts[1..=depth].iter().rev().map(String::as_str).collect();
            format!("{name}<{}>", parents.join("/"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
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
    fn normalizing_resolves_dots_lexically() {
        let root = Path::new(path::MAIN_SEPARATOR_STR);
        let path = root.join("a").join("b").join("..").join(".").join("c");

        assert_eq!(normalize(&path), root.join("a").join("c"));
        assert_eq!(normalize(&root.join("..").join("x")), root.join("x"));
        assert_eq!(
            normalize(Path::new(&format!(
                "{}{SEPARATOR}",
                root.join("d").display()
            )))
            .as_os_str(),
            root.join("d").as_os_str()
        );
    }

    fn names(paths: &[&str]) -> Vec<String> {
        let paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        unique_names(&paths)
    }

    #[test]
    fn distinct_file_names_stay_plain() {
        assert_eq!(names(&["/a/x.rs", "/b/y.rs"]), ["x.rs", "y.rs"]);
    }

    #[test]
    fn clashing_names_gain_their_parent_directory() {
        assert_eq!(
            names(&["/a/x/main.rs", "/b/y/main.rs", "/c/lib.rs"]),
            ["main.rs<x>", "main.rs<y>", "lib.rs"]
        );
    }

    #[test]
    fn clashing_parents_add_more_directories() {
        assert_eq!(
            names(&["/a/src/main.rs", "/b/src/main.rs"]),
            ["main.rs<a/src>", "main.rs<b/src>"]
        );
    }
}
