use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    Found { path: PathBuf, version: String },
    Broken { path: PathBuf, error: String },
    Missing,
}

pub fn probe(program: &str) -> Probe {
    let Some(path) = find(program, env::var_os("PATH").as_deref()) else {
        return Probe::Missing;
    };
    match version(&path) {
        Ok(version) => Probe::Found { path, version },
        Err(error) => Probe::Broken { path, error },
    }
}

pub fn find(program: &str, search_path: Option<&OsStr>) -> Option<PathBuf> {
    env::split_paths(search_path?)
        .flat_map(|directory| candidates(&directory, program))
        .find(|candidate| candidate.is_file())
}

fn candidates(directory: &Path, program: &str) -> Vec<PathBuf> {
    let mut names = vec![directory.join(program)];
    if cfg!(windows) {
        names.insert(0, directory.join(format!("{program}.exe")));
    }
    names
}

pub fn version(path: &Path) -> Result<String, String> {
    let mut command = Command::new(path);
    command.arg("--version");
    hide_console(&mut command);
    let output = command.output().map_err(|error| error.to_string())?;
    let first_line = |bytes: &[u8]| {
        String::from_utf8_lossy(bytes)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    if output.status.success() {
        Ok(first_line(&output.stdout))
    } else {
        let error = first_line(&output.stderr);
        Err(if error.is_empty() {
            output.status.to_string()
        } else {
            error
        })
    }
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_: &mut Command) {}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn programs_are_found_on_the_search_path() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) { "tool.exe" } else { "tool" };
        fs::write(second.path().join(name), "").unwrap();
        let search = env::join_paths([first.path(), second.path()]).unwrap();

        assert_eq!(find("tool", Some(&search)), Some(second.path().join(name)));
        assert_eq!(find("other", Some(&search)), None);
        assert_eq!(find("tool", None), None);
    }

    #[test]
    fn missing_programs_are_reported() {
        assert_eq!(probe("mx-no-such-tool"), Probe::Missing);
    }

    #[test]
    fn versions_come_from_the_first_line() {
        let cargo = version(Path::new(env!("CARGO"))).unwrap();

        assert!(cargo.starts_with("cargo "), "{cargo}");
    }

    #[test]
    fn programs_that_cannot_run_report_why() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-a-program.txt");
        fs::write(&path, "text").unwrap();

        assert!(version(&path).is_err());
    }
}
