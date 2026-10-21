use std::fs;
use std::path::{Path, PathBuf};

use oxmacs_core::editor::Outcome;
use serde::Deserialize;

#[derive(Default, Deserialize)]
struct Pyproject {
    project: Option<ProjectTable>,
}

#[derive(Deserialize)]
struct ProjectTable {
    name: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
}

#[derive(Deserialize)]
struct LockFile {
    #[serde(default)]
    package: Vec<Package>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: Option<String>,
}

#[derive(Deserialize)]
struct Outdated {
    name: String,
    version: String,
    latest_version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Project {
    pub(crate) name: Option<String>,
    pub(crate) required: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Newer {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) latest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Lock {
    Missing,
    Checking,
    Fresh,
    Stale,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Probe<T> {
    Checking,
    Known(T),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Facts {
    pub(crate) project: Project,
    pub(crate) locked: Vec<(String, String)>,
    pub(crate) python: Option<String>,
    pub(crate) venv: Option<PathBuf>,
    pub(crate) lock: Lock,
    pub(crate) newer: Probe<Vec<Newer>>,
}

impl Default for Facts {
    fn default() -> Self {
        Self {
            project: Project::default(),
            locked: Vec::new(),
            python: None,
            venv: None,
            lock: Lock::Missing,
            newer: Probe::Checking,
        }
    }
}

pub(crate) fn normalized(name: &str) -> String {
    name.to_ascii_lowercase().replace(['_', '.'], "-")
}

pub(crate) fn project(text: &str) -> Project {
    let Some(table) = toml::from_str::<Pyproject>(text)
        .ok()
        .and_then(|parsed| parsed.project)
    else {
        return Project::default();
    };
    Project {
        name: table.name,
        required: table.dependencies,
    }
}

pub(crate) fn locked(text: &str, project: Option<&str>) -> Vec<(String, String)> {
    let own = project.map(normalized);
    toml::from_str::<LockFile>(text)
        .map(|lock| lock.package)
        .unwrap_or_default()
        .into_iter()
        .filter(|package| Some(normalized(&package.name)) != own)
        .filter_map(|package| Some((package.name, package.version?)))
        .collect()
}

pub(crate) fn venv_version(cfg: &str) -> Option<String> {
    let value = |wanted: &str| {
        cfg.lines().find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == wanted).then(|| value.trim().to_owned())
        })
    };
    value("version_info").or_else(|| value("version"))
}

fn reason(outcome: &Outcome, output: &[String]) -> String {
    if let Outcome::Failed(error) = outcome {
        return error.clone();
    }
    output
        .iter()
        .find(|line| line.starts_with("error"))
        .or_else(|| output.iter().rev().find(|line| !line.trim().is_empty()))
        .map_or_else(|| "uv failed".to_owned(), |line| line.trim().to_owned())
}

pub(crate) fn newer(outcome: &Outcome, output: &[String]) -> Probe<Vec<Newer>> {
    if *outcome != Outcome::Exited(Some(0)) {
        return Probe::Failed(reason(outcome, output));
    }
    let listed = output
        .iter()
        .find(|line| line.trim_start().starts_with('['))
        .and_then(|json| serde_json::from_str::<Vec<Outdated>>(json).ok());
    match listed {
        Some(listed) => Probe::Known(
            listed
                .into_iter()
                .map(|outdated| Newer {
                    name: outdated.name,
                    version: outdated.version,
                    latest: outdated.latest_version,
                })
                .collect(),
        ),
        None => Probe::Failed("uv gave no list of packages".to_owned()),
    }
}

pub(crate) fn lock(outcome: &Outcome, output: &[String]) -> Lock {
    match outcome {
        Outcome::Exited(Some(0)) => Lock::Fresh,
        _ if output
            .iter()
            .any(|line| line.contains("needs to be updated")) =>
        {
            Lock::Stale
        }
        _ => Lock::Failed(reason(outcome, output)),
    }
}

pub(crate) fn read(root: &Path) -> Facts {
    let declared = fs::read_to_string(root.join("pyproject.toml"))
        .map(|text| project(&text))
        .unwrap_or_default();
    let lock_text = fs::read_to_string(root.join("uv.lock")).ok();
    let pinned = lock_text
        .as_deref()
        .map(|text| locked(text, declared.name.as_deref()))
        .unwrap_or_default();
    let venv = Some(root.join(".venv")).filter(|path| path.is_dir());
    let python = venv
        .as_ref()
        .and_then(|venv| fs::read_to_string(venv.join("pyvenv.cfg")).ok())
        .and_then(|cfg| venv_version(&cfg));
    Facts {
        lock: if lock_text.is_some() {
            Lock::Checking
        } else {
            Lock::Missing
        },
        newer: if venv.is_some() {
            Probe::Checking
        } else {
            Probe::Failed("no .venv".to_owned())
        },
        project: declared,
        locked: pinned,
        python,
        venv,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PYPROJECT: &str = r#"[project]
name = "demo"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "requests>=2.31",
    "Numpy",
]

[tool.ruff]
line-length = 100
"#;

    const LOCK: &str = r#"version = 1
requires-python = ">=3.12"

[[package]]
name = "certifi"
version = "2024.8.30"
source = { registry = "https://pypi.org/simple" }

[[package]]
name = "demo"
version = "0.1.0"
source = { editable = "." }

[[package]]
name = "numpy"
version = "2.1.0"
source = { registry = "https://pypi.org/simple" }
"#;

    #[test]
    fn names_compare_the_way_python_packaging_does() {
        assert_eq!(normalized("Zope.Interface_x"), "zope-interface-x");
    }

    #[test]
    fn the_project_names_itself_and_its_requirements() {
        assert_eq!(
            project(PYPROJECT),
            Project {
                name: Some("demo".to_owned()),
                required: vec!["requests>=2.31".to_owned(), "Numpy".to_owned()],
            }
        );
        assert_eq!(project("not toml ["), Project::default());
        assert_eq!(project("[tool.uv]\n"), Project::default());
    }

    #[test]
    fn the_lock_lists_every_package_but_the_project() {
        assert_eq!(
            locked(LOCK, Some("demo")),
            [
                ("certifi".to_owned(), "2024.8.30".to_owned()),
                ("numpy".to_owned(), "2.1.0".to_owned()),
            ]
        );
        assert_eq!(locked("broken", Some("demo")), []);
    }

    #[test]
    fn the_venv_says_which_python_it_holds() {
        assert_eq!(
            venv_version("home = C:\\py\nimplementation = CPython\nversion_info = 3.12.4\n"),
            Some("3.12.4".to_owned())
        );
        assert_eq!(
            venv_version("home = /usr/bin\nversion = 3.11.2\n"),
            Some("3.11.2".to_owned())
        );
        assert_eq!(venv_version("home = /usr/bin\n"), None);
    }

    #[test]
    fn outdated_packages_come_from_pip_list_json() {
        let output = vec![
            r#"[{"name":"requests","version":"2.32.3","latest_version":"2.33.0","latest_filetype":"wheel"}]"#
                .to_owned(),
        ];

        assert_eq!(
            newer(&Outcome::Exited(Some(0)), &output),
            Probe::Known(vec![Newer {
                name: "requests".to_owned(),
                version: "2.32.3".to_owned(),
                latest: "2.33.0".to_owned(),
            }])
        );
        assert_eq!(
            newer(
                &Outcome::Exited(Some(2)),
                &["error: No virtual environment found".to_owned()]
            ),
            Probe::Failed("error: No virtual environment found".to_owned())
        );
        assert_eq!(
            newer(&Outcome::Failed("cannot start".to_owned()), &[]),
            Probe::Failed("cannot start".to_owned())
        );
    }

    #[test]
    fn the_lock_check_says_fresh_stale_or_why_it_failed() {
        let stale = ["error: The lockfile at `uv.lock` needs to be updated, but `--check` was provided. To update the lockfile, run `uv lock`.".to_owned()];

        assert_eq!(lock(&Outcome::Exited(Some(0)), &[]), Lock::Fresh);
        assert_eq!(lock(&Outcome::Exited(Some(1)), &stale), Lock::Stale);
        assert_eq!(
            lock(&Outcome::Exited(Some(2)), &["error: No network".to_owned()]),
            Lock::Failed("error: No network".to_owned())
        );
    }

    #[test]
    fn projects_are_read_from_their_files() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("pyproject.toml"), PYPROJECT).unwrap();
        fs::write(root.path().join("uv.lock"), LOCK).unwrap();
        let venv = root.path().join(".venv");
        fs::create_dir(&venv).unwrap();
        fs::write(venv.join("pyvenv.cfg"), "version_info = 3.12.4\n").unwrap();

        let facts = read(root.path());

        assert_eq!(facts.project.name.as_deref(), Some("demo"));
        assert_eq!(facts.locked.len(), 2);
        assert_eq!(facts.python.as_deref(), Some("3.12.4"));
        assert_eq!(facts.venv, Some(venv));
        assert_eq!(facts.lock, Lock::Checking);
        assert_eq!(facts.newer, Probe::Checking);
    }

    #[test]
    fn missing_files_leave_their_facts_out() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("pyproject.toml"), PYPROJECT).unwrap();

        let facts = read(root.path());

        assert_eq!(facts.locked, []);
        assert_eq!(facts.python, None);
        assert_eq!(facts.venv, None);
        assert_eq!(facts.lock, Lock::Missing);
        assert_eq!(facts.newer, Probe::Failed("no .venv".to_owned()));
    }
}
