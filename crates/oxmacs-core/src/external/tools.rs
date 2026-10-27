use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const EDITIONS: &[&str] = &["2015", "2018", "2021", "2024"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    Found { path: PathBuf, version: String },
    Broken { path: PathBuf, error: String },
    Missing,
}

pub fn version(path: &Path) -> Result<String, String> {
    let output = oxmacs_process::run(Command::new(path).arg("--version"), None)
        .map_err(|error| error.to_string())?;
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
    let program = oxmacs_process::which("rustfmt")
        .ok_or_else(|| "not found; rustup component add rustfmt".to_owned())?;
    let mut command = Command::new(program);
    command
        .args(["--edition", edition, "--config", "newline_style=Unix"])
        .current_dir(directory);
    pipe(command, text)
}

pub fn air_format(program: &Path, text: &str, file: &Path) -> Result<String, String> {
    let mut command = Command::new(program);
    command.arg("format").arg("--stdin-file-path").arg(file);
    if let Some(directory) = file.parent().filter(|directory| directory.is_dir()) {
        command.current_dir(directory);
    }
    pipe(command, text)
}

fn pipe(mut command: Command, text: &str) -> Result<String, String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = oxmacs_process::run(&mut command, Some(text.as_bytes()))
        .map_err(|error| format!("cannot run {program}: {error}"))?;
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

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

    #[test]
    fn air_that_cannot_run_says_so() {
        let error =
            air_format(Path::new("oxmacs-no-such-air"), "x<-1", Path::new("a.R")).unwrap_err();

        assert!(
            error.starts_with("cannot run oxmacs-no-such-air"),
            "{error}"
        );
    }

    #[test]
    fn air_formats_text_when_installed() {
        let Some(air) = oxmacs_process::which("air") else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            air_format(&air, "x<-c(1,2)", &dir.path().join("a.R")),
            Ok("x <- c(1, 2)\n".to_owned())
        );
        assert!(air_format(&air, "x <- (", &dir.path().join("a.R")).is_err());
    }

    #[test]
    fn rustfmt_formats_text_when_installed() {
        if oxmacs_process::which("rustfmt").is_none() {
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
