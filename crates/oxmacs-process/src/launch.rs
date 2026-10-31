use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use oxmacs_files::wsl::{self, Place};

static INSIDE_WSL: AtomicBool = AtomicBool::new(false);

const LOGIN: &str = r#"exec "${SHELL:-/bin/sh}" -l -c 'exec "$@"' sh "$@""#;

pub fn run_inside_wsl(on: bool) {
    INSIDE_WSL.store(on, Ordering::Relaxed);
}

#[derive(Debug, Clone, Default)]
pub struct Launch {
    program: OsString,
    args: Vec<OsString>,
    directory: Option<PathBuf>,
    env: Vec<(OsString, Option<OsString>)>,
}

impl Launch {
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_owned());
        self
    }

    #[must_use]
    pub fn args(mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Self {
        self.args
            .extend(args.into_iter().map(|arg| arg.as_ref().to_owned()));
        self
    }

    #[must_use]
    pub fn directory(mut self, directory: impl AsRef<Path>) -> Self {
        self.directory = Some(directory.as_ref().to_owned());
        self
    }

    #[must_use]
    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.env
            .push((key.as_ref().to_owned(), Some(value.as_ref().to_owned())));
        self
    }

    #[must_use]
    pub fn env_remove(mut self, key: impl AsRef<OsStr>) -> Self {
        self.env.push((key.as_ref().to_owned(), None));
        self
    }

    pub fn distro(&self) -> Option<Place> {
        self.distro_with(INSIDE_WSL.load(Ordering::Relaxed))
    }

    fn distro_with(&self, inside_wsl: bool) -> Option<Place> {
        inside_wsl
            .then(|| self.directory.as_deref().and_then(wsl::place))
            .flatten()
    }

    pub fn command(&self) -> Command {
        self.command_with(INSIDE_WSL.load(Ordering::Relaxed))
    }

    fn command_with(&self, inside_wsl: bool) -> Command {
        match self.distro_with(inside_wsl) {
            Some(place) => self.inside(&place),
            None => self.outside(),
        }
    }

    fn outside(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        if let Some(directory) = &self.directory {
            command.current_dir(directory);
        }
        for (key, value) in &self.env {
            match value {
                Some(value) => command.env(key, value),
                None => command.env_remove(key),
            };
        }
        command
    }

    fn inside(&self, place: &Place) -> Command {
        let mut command = Command::new("wsl.exe");
        command
            .args([
                "-d",
                &place.distro,
                "--cd",
                &place.path,
                "--exec",
                "/bin/sh",
            ])
            .args(["-c", LOGIN, "sh"]);
        if !self.env.is_empty() {
            command.arg("env");
            for (key, value) in &self.env {
                match value {
                    Some(value) => {
                        let mut pair = key.clone();
                        pair.push("=");
                        pair.push(value);
                        command.arg(pair)
                    }
                    None => command.arg("-u").arg(key),
                };
            }
        }
        command.arg(program_inside(&self.program, &place.distro));
        command.args(self.args.iter().map(|arg| arg_inside(arg, &place.distro)));
        command
    }
}

fn linux(text: &str, distro: &str) -> Option<String> {
    wsl::place(Path::new(text))
        .filter(|place| place.distro.eq_ignore_ascii_case(distro))
        .map(|place| place.path)
}

fn program_inside(program: &OsStr, distro: &str) -> OsString {
    let text = program.to_string_lossy();
    if let Some(path) = linux(&text, distro) {
        return path.into();
    }
    let path = Path::new(&*text);
    if path.is_absolute() || text.contains(['\\', '/']) {
        return path
            .file_stem()
            .map_or_else(|| program.to_owned(), OsStr::to_owned);
    }
    program.to_owned()
}

fn arg_inside(arg: &OsStr, distro: &str) -> OsString {
    let text = arg.to_string_lossy();
    if let Some(path) = linux(&text, distro) {
        return path.into();
    }
    if let Some((key, value)) = text.split_once('=')
        && let Some(path) = linux(value, distro)
    {
        return format!("{key}={path}").into();
    }
    arg.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(command: &Command) -> Vec<String> {
        std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(|part| part.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn launches_in_a_distro_folder_run_inside_it_through_the_login_shell() {
        let launch = Launch::new(r"C:\Users\ana\.cargo\bin\cargo.exe")
            .args([
                "build",
                r"--manifest-path=\\wsl.localhost\Fedora\home\ana\x\Cargo.toml",
                r"\\wsl$\Fedora\home\ana\x\src",
                r"\\wsl.localhost\Other\tmp",
            ])
            .directory(r"\\wsl.localhost\Fedora\home\ana\x")
            .env("RUST_LOG", "debug")
            .env_remove("PAGER");
        let inside = shown(&launch.command_with(true));
        let outside = launch.command_with(false);

        assert_eq!(
            inside,
            [
                "wsl.exe",
                "-d",
                "Fedora",
                "--cd",
                "/home/ana/x",
                "--exec",
                "/bin/sh",
                "-c",
                LOGIN,
                "sh",
                "env",
                "RUST_LOG=debug",
                "-u",
                "PAGER",
                "cargo",
                "build",
                "--manifest-path=/home/ana/x/Cargo.toml",
                "/home/ana/x/src",
                r"\\wsl.localhost\Other\tmp",
            ]
        );
        assert_eq!(outside.get_program(), r"C:\Users\ana\.cargo\bin\cargo.exe");
    }

    #[test]
    fn launches_elsewhere_run_as_they_are() {
        let command = Launch::new("git")
            .arg("status")
            .directory(r"C:\work")
            .command_with(true);

        assert_eq!(shown(&command), ["git", "status"]);
        assert_eq!(command.get_current_dir(), Some(Path::new(r"C:\work")));
    }

    #[test]
    #[ignore = "runs inside the WSL distro named by OXMACS_WSL_DISTRO"]
    fn programs_really_run_inside_a_distro_with_the_login_path() {
        let Ok(distro) = std::env::var("OXMACS_WSL_DISTRO") else {
            return;
        };
        let output = crate::run(
            &mut Launch::new("sh")
                .args(["-c", "pwd; printf %s \"$GREETING\""])
                .directory(wsl::share(&distro, "/tmp"))
                .env("GREETING", "hello")
                .command_with(true),
            None,
        )
        .unwrap();

        assert_eq!(String::from_utf8_lossy(&output.stdout), "/tmp\nhello");
    }
}
