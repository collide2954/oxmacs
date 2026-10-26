use std::fs;
use std::path::{Path, PathBuf};

pub fn ox_files(path: &Path, most: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if path.is_dir() {
        walk(path, most, &mut found);
    } else {
        found.push(path.to_path_buf());
    }
    found
}

fn walk(folder: &Path, most: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if found.len() >= most {
            return;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if !name.starts_with('.') && name != "target" {
                walk(&path, most, found);
            }
        } else if path.extension().is_some_and(|extension| extension == "ox") {
            found.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::ox_files;

    #[test]
    fn folders_give_their_ox_files_in_order_without_hidden_or_target_folders() {
        let dir = tempfile::tempdir().unwrap();
        for file in [
            "b.ox",
            "a.ox",
            "notes.txt",
            "lib/c.ox",
            ".git/d.ox",
            "target/e.ox",
        ] {
            let path = dir.path().join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }

        let found: Vec<String> = ox_files(dir.path(), usize::MAX)
            .iter()
            .map(|path| {
                path.strip_prefix(dir.path())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        assert_eq!(found, ["a.ox", "b.ox", "lib/c.ox"]);
        assert_eq!(ox_files(dir.path(), 1).len(), 1);
        assert_eq!(
            ox_files(&dir.path().join("notes.txt"), usize::MAX),
            [dir.path().join("notes.txt")]
        );
    }
}
