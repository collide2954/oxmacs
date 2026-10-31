use std::path::{Component, PathBuf};

pub use oxmacs_files::paths::{SEPARATOR, abbreviate, expand, is_separator, normalize};

pub fn split(input: &str) -> (&str, &str) {
    match input.rfind(is_separator) {
        Some(index) => input.split_at(index + 1),
        None => ("", input),
    }
}

fn starts_anew(rest: &str) -> bool {
    let mut chars = rest.chars();
    match (chars.next(), chars.next()) {
        (Some(first), _) if is_separator(first) => true,
        (Some('~'), Some(next)) => is_separator(next),
        (Some(letter), Some(':')) => cfg!(windows) && letter.is_ascii_alphabetic(),
        _ => cfg!(windows) && rest.starts_with("wsl:"),
    }
}

pub fn restart(input: &str) -> Option<&str> {
    input
        .char_indices()
        .rev()
        .filter(|&(index, c)| index > 0 && is_separator(c))
        .map(|(index, c)| index + c.len_utf8())
        .find(|&start| starts_anew(&input[start..]))
        .map(|start| &input[start..])
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
    use super::*;

    #[test]
    fn input_splits_at_the_last_separator() {
        assert_eq!(split("~/oxmacs/cr"), ("~/oxmacs/", "cr"));
        assert_eq!(split("dir/"), ("dir/", ""));
        assert_eq!(split("abc"), ("", "abc"));
    }

    #[test]
    fn a_new_root_typed_after_a_separator_starts_the_path_over() {
        let s = SEPARATOR;

        assert_eq!(
            restart(&format!("~{s}src{s}{s}etc")),
            Some(format!("{s}etc").as_str())
        );
        assert_eq!(
            restart(&format!("{s}srv{s}x{s}~{s}notes")),
            Some(format!("~{s}notes").as_str())
        );
        assert_eq!(restart(&format!("~{s}src{s}a~b")), None);
        assert_eq!(restart(&format!("{s}{s}server")), None);
        assert_eq!(restart(&format!("~{s}x")), None);
        if cfg!(windows) {
            assert_eq!(restart(r"C:\Users\ana\D:"), Some("D:"));
            assert_eq!(restart(r"C:\Users\ana\D:\x"), Some(r"D:\x"));
            assert_eq!(
                restart(r"C:\Users\ana\wsl:Fedora:/etc"),
                Some("wsl:Fedora:/etc")
            );
        }
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
