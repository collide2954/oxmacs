use std::fs;
use std::path::{Path, PathBuf};

use oxmacs_files::wsl;
use oxmacs_process::Launch;

pub const PROJECT: &[&str] = &[".git", ".hg", ".svn", ".projectile"];

fn marked(folder: &Path, marker: &str) -> bool {
    match marker.strip_prefix("*.") {
        Some(extension) => fs::read_dir(folder)
            .into_iter()
            .flatten()
            .flatten()
            .any(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|found| found == extension)
            }),
        None => folder.join(marker).exists(),
    }
}

pub fn upward<S: AsRef<str>>(folder: &Path, markers: &[S], home: Option<&Path>) -> Option<PathBuf> {
    if markers.is_empty() {
        return None;
    }
    folder
        .ancestors()
        .take_while(|ancestor| Some(*ancestor) != home)
        .find(|ancestor| {
            markers
                .iter()
                .any(|marker| marked(ancestor, marker.as_ref()))
        })
        .map(Path::to_path_buf)
}

pub fn located<S: AsRef<str>>(folder: &Path, command: &[S]) -> Option<PathBuf> {
    let (program, args) = command.split_first()?;
    let launch = Launch::new(program.as_ref())
        .args(args.iter().map(AsRef::as_ref))
        .directory(folder);
    let output = oxmacs_process::run(&mut launch.command(), None).ok()?;
    if !output.status.success() {
        return None;
    }
    let file = String::from_utf8(output.stdout).ok()?;
    let file = match launch.distro() {
        Some(place) => wsl::share(&place.distro, file.trim()),
        None => PathBuf::from(file.trim()),
    };
    let root = file.parent()?;
    let canonical = fs::canonicalize(root).ok();
    let spelled = folder
        .ancestors()
        .find(|ancestor| canonical.is_some() && fs::canonicalize(ancestor).ok() == canonical);
    Some(spelled.unwrap_or(root).to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_marked_folder_below_home_is_the_root() {
        let home = tempfile::tempdir().unwrap();
        let project = home.path().join("work").join("app");
        let inner = project.join("src").join("deep");
        fs::create_dir_all(&inner).unwrap();
        fs::write(project.join("app.Rproj"), "").unwrap();
        fs::create_dir(home.path().join(".git")).unwrap();

        assert_eq!(
            upward(&inner, &["*.Rproj"], Some(home.path())),
            Some(project.clone())
        );
        assert_eq!(upward(&inner, PROJECT, Some(home.path())), None);
        assert_eq!(upward(&inner, PROJECT, None), Some(home.path().to_owned()));
        assert_eq!(upward::<&str>(&inner, &[], None), None);
    }
}
