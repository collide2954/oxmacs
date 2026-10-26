use std::path::{Component, Path, PathBuf};

pub(crate) fn normalized(path: &Path) -> PathBuf {
    let mut parts = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir
                if matches!(parts.components().next_back(), Some(Component::Normal(_))) =>
            {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts
}

pub(crate) fn absolute(path: &Path) -> PathBuf {
    normalized(&std::path::absolute(path).unwrap_or_else(|_| path.to_owned()))
}
