use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use oxmacs_core::config::schema::{Kind, Setting};
use oxmacs_core::editor::Editor;
use oxmacs_core::external::managed::{Checksum, Form, Managed};
use oxmacs_core::external::tools;
use serde::{Deserialize, Serialize};

pub(crate) const TABLE: &str = "python";
const GROUP: &str = "Python";
const MISSING: &str =
    "Python not found; install it from https://www.python.org or set [python] interpreter";
const VENV: [&str; 3] = if cfg!(windows) {
    [".venv", "Scripts", "python.exe"]
} else {
    [".venv", "bin", "python"]
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ruff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uv: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpreter: Option<String>,
}

pub(crate) const SETTINGS: &[Setting] = &[
    Setting {
        key: "ruff",
        group: GROUP,
        kind: Kind::Text,
        unset: Some("auto-detect"),
        hint: "ruff executable that formats and lints Python; M-x list-tools installs one",
    },
    Setting {
        key: "uv",
        group: GROUP,
        kind: Kind::Text,
        unset: Some("auto-detect"),
        hint: "uv executable that manages Python projects; M-x list-tools installs one",
    },
    Setting {
        key: "interpreter",
        group: GROUP,
        kind: Kind::Text,
        unset: Some("auto-detect"),
        hint: "Python to run when the project has no .venv; python3 or python on PATH otherwise",
    },
];

pub const RUFF: Managed = Managed {
    name: "ruff",
    repository: "astral-sh/ruff",
    summary: "Python formatter and linter",
    form: Form::Program,
    tag_prefix: "v",
    archives: &[
        ("windows", "x86_64", "ruff-x86_64-pc-windows-msvc.zip"),
        ("windows", "aarch64", "ruff-aarch64-pc-windows-msvc.zip"),
        ("macos", "x86_64", "ruff-x86_64-apple-darwin.tar.gz"),
        ("macos", "aarch64", "ruff-aarch64-apple-darwin.tar.gz"),
        ("linux", "x86_64", "ruff-x86_64-unknown-linux-gnu.tar.gz"),
        ("linux", "aarch64", "ruff-aarch64-unknown-linux-gnu.tar.gz"),
    ],
    checksum: Checksum::Sidecar,
    setting: None,
};

pub const UV: Managed = Managed {
    name: "uv",
    repository: "astral-sh/uv",
    summary: "Python package and project manager",
    form: Form::Program,
    tag_prefix: "v",
    archives: &[
        ("windows", "x86_64", "uv-x86_64-pc-windows-msvc.zip"),
        ("windows", "aarch64", "uv-aarch64-pc-windows-msvc.zip"),
        ("macos", "x86_64", "uv-x86_64-apple-darwin.tar.gz"),
        ("macos", "aarch64", "uv-aarch64-apple-darwin.tar.gz"),
        ("linux", "x86_64", "uv-x86_64-unknown-linux-gnu.tar.gz"),
        ("linux", "aarch64", "uv-aarch64-unknown-linux-gnu.tar.gz"),
    ],
    checksum: Checksum::Sidecar,
    setting: None,
};

pub(crate) fn settings(editor: &Editor) -> Settings {
    editor.applet_config(TABLE)
}

pub(crate) fn locate(
    editor: &Editor,
    tool: Managed,
    configured: Option<&str>,
    search: Option<&OsStr>,
) -> Result<PathBuf, String> {
    if let Some(configured) = configured {
        return tool
            .locate(Some(configured), None, search)
            .map(|(path, _)| path)
            .ok_or_else(|| format!("[python] {} not found: {configured}", tool.name));
    }
    editor
        .installed_tool(tool)
        .map(|(_, path)| path)
        .or_else(|| tools::find(tool.name, search))
        .ok_or_else(|| format!("{} is not installed; M-x list-tools installs it", tool.name))
}

pub(crate) fn ruff(editor: &Editor) -> Result<PathBuf, String> {
    let search = env::var_os("PATH");
    locate(
        editor,
        RUFF,
        settings(editor).ruff.as_deref(),
        search.as_deref(),
    )
}

pub(crate) fn interpreter_in(
    start: &Path,
    configured: Option<&str>,
    search: Option<&OsStr>,
) -> Result<PathBuf, String> {
    let venv = start
        .ancestors()
        .map(|folder| {
            VENV.iter()
                .fold(folder.to_path_buf(), |path, part| path.join(part))
        })
        .find(|path| path.is_file());
    if let Some(venv) = venv {
        return Ok(venv);
    }
    if let Some(configured) = configured {
        let path = Path::new(configured);
        if path.is_file() {
            return Ok(path.to_owned());
        }
        return tools::find(configured, search)
            .ok_or_else(|| format!("[python] interpreter not found: {configured}"));
    }
    ["python3", "python"]
        .into_iter()
        .find_map(|name| tools::find(name, search))
        .ok_or_else(|| MISSING.to_owned())
}

pub(crate) fn interpreter(editor: &Editor) -> Result<PathBuf, String> {
    let search = env::var_os("PATH");
    interpreter_in(
        &editor.default_directory(),
        settings(editor).interpreter.as_deref(),
        search.as_deref(),
    )
}

pub(crate) fn project_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|folder| folder.join("pyproject.toml").is_file())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};

    use oxmacs_core::config::Config;

    use super::*;
    use crate::testing::{installed, lines, run};

    fn program(folder: &Path, name: &str) -> PathBuf {
        fs::create_dir_all(folder).unwrap();
        let path = folder.join(format!("{name}{}", env::consts::EXE_SUFFIX));
        fs::write(&path, "").unwrap();
        path
    }

    #[test]
    fn the_python_table_holds_ruff_uv_and_the_interpreter() {
        let mut editor = installed();

        editor.apply_config(
            &Config::parse("[python]\nruff = \"r\"\nuv = \"u\"\ninterpreter = \"py\"\n").unwrap(),
        );

        assert_eq!(
            settings(&editor),
            Settings {
                ruff: Some("r".to_owned()),
                uv: Some("u".to_owned()),
                interpreter: Some("py".to_owned()),
            }
        );
        assert_eq!(editor.unclaimed_config_tables(), Vec::<&str>::new());
        assert_eq!(
            SETTINGS
                .iter()
                .map(|setting| setting.key)
                .collect::<Vec<_>>(),
            ["ruff", "uv", "interpreter"]
        );
    }

    #[test]
    fn ruff_and_uv_install_from_their_github_releases() {
        assert_eq!(RUFF.repository, "astral-sh/ruff");
        assert_eq!(UV.repository, "astral-sh/uv");
        assert_eq!(
            RUFF.archive("windows", "x86_64").as_deref(),
            Some("ruff-x86_64-pc-windows-msvc.zip")
        );
        assert_eq!(
            UV.archive("linux", "aarch64").as_deref(),
            Some("uv-aarch64-unknown-linux-gnu.tar.gz")
        );
        assert_eq!(
            RUFF.archive("macos", "aarch64").as_deref(),
            Some("ruff-aarch64-apple-darwin.tar.gz")
        );
        assert_eq!(RUFF.checksum, Checksum::Sidecar);
        assert_eq!(UV.checksum, Checksum::Sidecar);
        assert_eq!(RUFF.form, Form::Program);
    }

    #[test]
    fn list_tools_offers_ruff_and_uv() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = installed();
        editor.set_tools_root(Some(root.path().to_owned()));

        run(&mut editor, "list-tools");

        let shown = lines(&editor);
        assert!(
            shown.iter().any(|line| line == "ruff  not installed"),
            "{shown:?}"
        );
        assert!(
            shown.iter().any(|line| line == "uv  not installed"),
            "{shown:?}"
        );
    }

    #[test]
    fn the_setting_wins_then_oxmacss_copy_then_the_path() {
        let root = tempfile::tempdir().unwrap();
        let path_folder = tempfile::tempdir().unwrap();
        let on_path = program(path_folder.path(), "ruff");
        let search = env::join_paths([path_folder.path()]).unwrap();
        let mut editor = installed();
        editor.set_tools_root(Some(root.path().to_owned()));

        assert_eq!(locate(&editor, RUFF, None, Some(&search)), Ok(on_path));
        let managed = program(&root.path().join("ruff").join("0.14.0"), "ruff");
        assert_eq!(locate(&editor, RUFF, None, Some(&search)), Ok(managed));
        let configured = program(root.path(), "my-ruff");
        assert_eq!(
            locate(&editor, RUFF, configured.to_str(), Some(&search)),
            Ok(configured)
        );
    }

    #[test]
    fn missing_tools_say_how_to_get_them() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = installed();
        editor.set_tools_root(Some(root.path().to_owned()));

        assert_eq!(
            locate(&editor, UV, None, None),
            Err("uv is not installed; M-x list-tools installs it".to_owned())
        );
        assert_eq!(
            locate(&editor, RUFF, Some("no-such-ruff"), None),
            Err("[python] ruff not found: no-such-ruff".to_owned())
        );
    }

    fn venv_python(project: &Path) -> PathBuf {
        let folder = if cfg!(windows) {
            project.join(".venv").join("Scripts")
        } else {
            project.join(".venv").join("bin")
        };
        program(&folder, "python")
    }

    #[test]
    fn the_projects_venv_comes_first_looking_upward() {
        let project = tempfile::tempdir().unwrap();
        let python = venv_python(project.path());
        let inner = project.path().join("src").join("app");
        fs::create_dir_all(&inner).unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let configured = program(elsewhere.path(), "configured");

        assert_eq!(
            interpreter_in(&inner, configured.to_str(), None),
            Ok(python)
        );
    }

    #[test]
    fn without_a_venv_the_setting_then_python3_then_python_run() {
        let project = tempfile::tempdir().unwrap();
        let path_folder = tempfile::tempdir().unwrap();
        let search = env::join_paths([path_folder.path()]).unwrap();
        let python = program(path_folder.path(), "python");

        assert_eq!(
            interpreter_in(project.path(), None, Some(&search)),
            Ok(python.clone())
        );
        let python3 = program(path_folder.path(), "python3");
        assert_eq!(
            interpreter_in(project.path(), None, Some(&search)),
            Ok(python3)
        );
        assert_eq!(
            interpreter_in(project.path(), python.to_str(), Some(&search)),
            Ok(python)
        );
        assert_eq!(
            interpreter_in(project.path(), Some("no-such-python"), Some(&search)),
            Err("[python] interpreter not found: no-such-python".to_owned())
        );
        assert_eq!(
            interpreter_in(project.path(), None, None),
            Err(
                "Python not found; install it from https://www.python.org or set [python] interpreter"
                    .to_owned()
            )
        );
    }

    #[test]
    fn projects_are_found_by_their_pyproject() {
        let project = tempfile::tempdir().unwrap();
        fs::write(project.path().join("pyproject.toml"), "[project]\n").unwrap();
        let inner = project.path().join("src").join("app");
        fs::create_dir_all(&inner).unwrap();
        let loose = tempfile::tempdir().unwrap();

        assert_eq!(project_root(&inner), Some(project.path().to_owned()));
        assert_eq!(project_root(loose.path()), None);
    }
}
