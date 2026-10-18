use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fs, thread};

const EDITIONS: &[&str] = &["2015", "2018", "2021", "2024"];

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

pub fn rustfmt(text: &str, directory: &Path, edition: &str) -> Result<String, String> {
    let program = find("rustfmt", env::var_os("PATH").as_deref())
        .ok_or_else(|| "not found; rustup component add rustfmt".to_owned())?;
    let mut command = Command::new(program);
    command
        .args(["--edition", edition, "--config", "newline_style=Unix"])
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut command);
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| "could not write to rustfmt".to_owned())?;
    let output = thread::scope(|scope| {
        scope.spawn(move || input.write_all(text.as_bytes()));
        child.wait_with_output()
    })
    .map_err(|error| error.to_string())?;
    if output.status.success() {
        return String::from_utf8(output.stdout).map_err(|error| error.to_string());
    }
    let errors = String::from_utf8_lossy(&output.stderr);
    Err(errors
        .lines()
        .find(|line| !line.trim().is_empty())
        .map_or_else(|| output.status.to_string(), |line| line.trim().to_owned()))
}

pub fn cargo_edition(directory: &Path) -> &'static str {
    directory
        .ancestors()
        .filter_map(|ancestor| fs::read_to_string(ancestor.join("Cargo.toml")).ok())
        .find_map(|manifest| {
            manifest.lines().find_map(|line| {
                let value = line.trim().strip_prefix("edition")?.trim_start();
                let value = value.strip_prefix('=')?.trim().trim_matches('"');
                EDITIONS.iter().copied().find(|edition| *edition == value)
            })
        })
        .unwrap_or("2024")
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
        assert_eq!(probe("oxmacs-no-such-tool"), Probe::Missing);
    }

    #[test]
    fn versions_come_from_the_first_line() {
        let cargo = version(Path::new(env!("CARGO"))).unwrap();

        assert!(cargo.starts_with("cargo "), "{cargo}");
    }

    #[test]
    fn editions_come_from_the_nearest_manifest_that_names_one() {
        let root = tempfile::tempdir().unwrap();
        let member = root.path().join("crates").join("a");
        fs::create_dir_all(member.join("src")).unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace.package]\nedition = \"2018\"\n",
        )
        .unwrap();
        fs::write(
            member.join("Cargo.toml"),
            "[package]\nedition.workspace = true\n",
        )
        .unwrap();

        assert_eq!(cargo_edition(&member.join("src")), "2018");

        fs::write(member.join("Cargo.toml"), "[package]\nedition = \"2021\"\n").unwrap();
        assert_eq!(cargo_edition(&member.join("src")), "2021");
    }

    #[test]
    fn rustfmt_formats_text_when_installed() {
        if find("rustfmt", env::var_os("PATH").as_deref()).is_none() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            rustfmt("fn  main(){}", dir.path(), "2024"),
            Ok("fn main() {}\n".to_owned())
        );
        assert!(rustfmt("fn main( {", dir.path(), "2024").is_err());
    }

    #[test]
    fn programs_that_cannot_run_report_why() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-a-program.txt");
        fs::write(&path, "text").unwrap();

        assert!(version(&path).is_err());
    }
}
