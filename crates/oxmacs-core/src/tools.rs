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

pub fn cargo_root(directory: &Path) -> Option<PathBuf> {
    let manifests: Vec<(PathBuf, String)> = directory
        .ancestors()
        .filter_map(|ancestor| {
            let manifest = fs::read_to_string(ancestor.join("Cargo.toml")).ok()?;
            Some((ancestor.to_owned(), manifest))
        })
        .collect();
    manifests
        .iter()
        .rev()
        .find(|(_, manifest)| manifest.lines().any(|line| line.trim() == "[workspace]"))
        .or_else(|| manifests.first())
        .map(|(root, _)| root.clone())
}

pub fn split_command(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

const R_PROGRAM: &str = if cfg!(windows) { "Rterm" } else { "R" };

pub fn r_arguments() -> Vec<String> {
    let arguments: &[&str] = if cfg!(windows) {
        &["--ess", "--no-save", "--no-restore", "--quiet"]
    } else {
        &[
            "--no-save",
            "--no-restore",
            "--quiet",
            "--interactive",
            "--no-readline",
        ]
    };
    arguments
        .iter()
        .map(|argument| (*argument).to_owned())
        .collect()
}

pub fn probe_r(configured: Option<&str>) -> Probe {
    let Some(path) = find_r(configured) else {
        return Probe::Missing;
    };
    match version(&path) {
        Ok(version) => Probe::Found { path, version },
        Err(error) => Probe::Broken { path, error },
    }
}

pub fn find_r(configured: Option<&str>) -> Option<PathBuf> {
    let r_home = env::var_os("R_HOME").map(PathBuf::from);
    find_r_in(
        configured,
        env::var_os("PATH").as_deref(),
        r_home.as_deref(),
        &r_installs(),
    )
}

pub fn find_r_in(
    configured: Option<&str>,
    search_path: Option<&OsStr>,
    r_home: Option<&Path>,
    installs: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(configured) = configured {
        let path = Path::new(configured);
        return if path.is_file() {
            Some(path.to_owned())
        } else {
            find(configured, search_path)
        };
    }
    find(R_PROGRAM, search_path)
        .or_else(|| r_home.and_then(r_binary))
        .or_else(|| {
            installs
                .iter()
                .filter_map(|install| Some((r_version(install), r_binary(install)?)))
                .max_by(|a, b| a.0.cmp(&b.0))
                .map(|(_, path)| path)
        })
}

fn r_binary(home: &Path) -> Option<PathBuf> {
    let bin = home.join("bin");
    let candidates = if cfg!(windows) {
        vec![bin.join("x64").join("Rterm.exe"), bin.join("Rterm.exe")]
    } else {
        vec![bin.join("R")]
    };
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn r_version(install: &Path) -> Vec<u32> {
    install
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
        .trim_start_matches("R-")
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

fn r_installs() -> Vec<PathBuf> {
    let mut installs: Vec<PathBuf> = ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(env::var_os)
        .map(|root| {
            let root = PathBuf::from(root);
            if root.ends_with("Local") {
                root.join("Programs").join("R")
            } else {
                root.join("R")
            }
        })
        .filter_map(|root| fs::read_dir(root).ok())
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    installs.extend(registry_installs());
    if cfg!(target_os = "macos") {
        installs.push(PathBuf::from("/Library/Frameworks/R.framework/Resources"));
    }
    installs
}

fn registry_installs() -> Vec<PathBuf> {
    if !cfg!(windows) {
        return Vec::new();
    }
    ["HKLM\\SOFTWARE\\R-core\\R", "HKCU\\SOFTWARE\\R-core\\R"]
        .into_iter()
        .filter_map(|key| {
            let mut command = Command::new("reg");
            command.args(["query", key, "/v", "InstallPath"]);
            hide_console(&mut command);
            let output = command.output().ok()?;
            let text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.lines()
                .find_map(|line| line.split_once("REG_SZ"))
                .map(|(_, path)| PathBuf::from(path.trim()))
        })
        .collect()
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
pub(crate) fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub(crate) fn hide_console(_: &mut Command) {}

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
    fn cargo_roots_prefer_the_workspace() {
        let root = tempfile::tempdir().unwrap();
        let member = root.path().join("crates").join("a");
        fs::create_dir_all(member.join("src")).unwrap();
        fs::write(member.join("Cargo.toml"), "[package]\n").unwrap();
        assert_eq!(cargo_root(&member.join("src")), Some(member.clone()));

        fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        assert_eq!(
            cargo_root(&member.join("src")),
            Some(root.path().to_owned())
        );
    }

    #[test]
    fn command_lines_split_on_spaces_outside_quotes() {
        assert_eq!(split_command("cargo  build"), ["cargo", "build"]);
        assert_eq!(
            split_command("cargo test \"two words\" \"\""),
            ["cargo", "test", "two words", ""]
        );
        assert_eq!(split_command("   "), Vec::<String>::new());
    }

    fn r_home_in(root: &Path, name: &str) -> PathBuf {
        let home = root.join(name);
        let bin = if cfg!(windows) {
            home.join("bin").join("x64")
        } else {
            home.join("bin")
        };
        fs::create_dir_all(&bin).unwrap();
        let program = if cfg!(windows) { "Rterm.exe" } else { "R" };
        fs::write(bin.join(program), "").unwrap();
        home
    }

    #[test]
    fn r_comes_from_the_setting_then_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let on_path = r_home_in(dir.path(), "path");
        let configured = dir.path().join("my-r");
        fs::write(&configured, "").unwrap();
        let bin = r_binary(&on_path).unwrap();
        let search = env::join_paths([bin.parent().unwrap()]).unwrap();

        assert_eq!(
            find_r_in(configured.to_str(), Some(&search), None, &[]),
            Some(configured)
        );
        assert_eq!(find_r_in(None, Some(&search), None, &[]), Some(bin));
    }

    #[test]
    fn r_home_and_the_newest_install_are_fallbacks() {
        let dir = tempfile::tempdir().unwrap();
        let home = r_home_in(dir.path(), "home");
        let old = r_home_in(dir.path(), "R-4.9.2");
        let new = r_home_in(dir.path(), "R-4.10.0");

        assert_eq!(find_r_in(None, None, Some(&home), &[]), r_binary(&home));
        assert_eq!(
            find_r_in(None, None, None, &[old, new.clone()]),
            r_binary(&new)
        );
        assert_eq!(find_r_in(None, None, None, &[]), None);
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
