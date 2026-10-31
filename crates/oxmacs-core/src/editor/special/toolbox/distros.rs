use std::env;
use std::path::{Path, PathBuf};

use oxmacs_files::wsl;

use crate::editor::page::Page;
use crate::editor::{Editor, JobEvent, JobSpec, Outcome, ServerSpec};
use crate::external::managed::{self, Form, Managed};
use crate::theme::Highlight;

const WATCHER: &str = "inotifywait";
const WATCHER_PACKAGE: &str = "inotify-tools";
const TOOLS: &str = ".local/share/oxmacs/tools";

const PROBE: &str = r#". /etc/os-release 2>/dev/null
printf 'family\t%s %s\n' "$ID" "$ID_LIKE"
printf 'arch\t%s\n' "$(uname -m)"
printf 'home\t%s\n' "$HOME"
for program in "$@"; do printf 'found\t%s\t%s\n' "$program" "$(command -v "$program")"; done"#;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::editor) struct Probe {
    family: Vec<String>,
    arch: String,
    home: String,
    found: Vec<(String, String)>,
}

impl Probe {
    fn read(&mut self, line: &str) {
        let mut parts = line.splitn(3, '\t');
        match (parts.next(), parts.next(), parts.next()) {
            (Some("family"), Some(words), None) => {
                self.family = words.split_whitespace().map(str::to_owned).collect();
            }
            (Some("arch"), Some(arch), None) => arch.clone_into(&mut self.arch),
            (Some("home"), Some(home), None) => home.clone_into(&mut self.home),
            (Some("found"), Some(program), Some(path)) if !path.is_empty() => {
                self.found.push((program.to_owned(), path.to_owned()));
            }
            _ => {}
        }
    }

    fn path_of(&self, program: &str) -> Option<&str> {
        self.found
            .iter()
            .find(|(known, _)| known == program)
            .map(|(_, path)| path.as_str())
    }

    fn family(&self) -> Vec<&str> {
        self.family.iter().map(String::as_str).collect()
    }
}

fn tools_root(distro: &str, home: &str) -> PathBuf {
    wsl::share(distro, &format!("{}/{TOOLS}", home.trim_end_matches('/')))
}

pub(super) fn package_install(family: &[&str], package: &str) -> Option<(&'static str, String)> {
    family.iter().find_map(|kind| match *kind {
        "fedora" | "rhel" | "centos" | "rocky" | "almalinux" | "amzn" => {
            Some(("dnf", format!("dnf install -y {package}")))
        }
        "debian" | "ubuntu" => Some((
            "apt",
            format!("apt-get update && apt-get install -y {package}"),
        )),
        "arch" | "manjaro" => Some(("pacman", format!("pacman -S --noconfirm {package}"))),
        "suse" | "opensuse" | "sles" => Some((
            "zypper",
            format!("zypper --non-interactive install {package}"),
        )),
        "alpine" => Some(("apk", format!("apk add {package}"))),
        _ => None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::editor) enum Check {
    Stopped,
    Checking,
    Failed(String),
    Known(Probe),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::editor) struct Distro {
    pub(super) name: String,
    pub(super) check: Check,
}

impl Distro {
    pub(super) fn probe(&self) -> Option<&Probe> {
        match &self.check {
            Check::Known(probe) => Some(probe),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::editor) enum Row {
    Tool(Managed),
    Server(ServerSpec),
    Watcher,
}

fn listing(wsl: &Path, args: &[&str], directory: PathBuf) -> JobSpec {
    JobSpec {
        program: wsl.display().to_string(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        directory,
        env: vec![("WSL_UTF8".to_owned(), "1".to_owned())],
        label: "list-tools",
        name: format!("wsl {}", args.join(" ")),
        buffer: None,
        interactive: false,
        input: None,
    }
}

fn collected(
    then: impl FnOnce(&mut Editor, Vec<String>, bool) + Send + 'static,
) -> impl FnMut(&mut Editor, crate::editor::JobId, JobEvent<'_>) + Send + 'static {
    let mut lines = Vec::new();
    let mut then = Some(then);
    move |editor, _, event| match event {
        JobEvent::Line(line) => lines.push(line.to_owned()),
        JobEvent::Output(_) => {}
        JobEvent::Finished(outcome) => {
            if let Some(then) = then.take() {
                then(
                    editor,
                    std::mem::take(&mut lines),
                    matches!(outcome, Outcome::Exited(Some(0))),
                );
            }
        }
    }
}

impl Editor {
    pub(in crate::editor) fn check_distros(&mut self) {
        if !oxmacs_process::inside_wsl() {
            return;
        }
        if let Some(wsl) = wsl::installed() {
            self.check_distros_with(wsl);
        }
    }

    fn outside_distros(&self) -> PathBuf {
        self.home
            .clone()
            .filter(|home| wsl::place(home).is_none())
            .unwrap_or_else(env::temp_dir)
    }

    pub(in crate::editor) fn check_distros_with(&mut self, wsl: PathBuf) {
        let directory = self.outside_distros();
        let again = (wsl.clone(), directory.clone());
        self.start_job(
            listing(&wsl, &["--list", "--quiet"], directory),
            collected(move |editor, lines, worked| {
                let all = if worked {
                    wsl::names(&lines.join("\n"))
                } else {
                    Vec::new()
                };
                let (wsl, directory) = again;
                editor.start_job(
                    listing(&wsl, &["--list", "--running", "--quiet"], directory),
                    collected(move |editor, lines, worked| {
                        let running = if worked {
                            wsl::names(&lines.join("\n"))
                        } else {
                            Vec::new()
                        };
                        editor.distros_listed(&all, &running);
                    }),
                );
            }),
        );
    }

    fn distro_programs(&self) -> Vec<String> {
        let mut programs: Vec<String> = self
            .shown_server_specs()
            .iter()
            .flat_map(|spec| spec.programs.iter().map(|(program, _)| program.clone()))
            .collect();
        programs.push(WATCHER.to_owned());
        programs.sort();
        programs.dedup();
        programs
    }

    pub(in crate::editor) fn distros_listed(&mut self, all: &[String], running: &[String]) {
        self.toolbox.distros = all
            .iter()
            .map(|name| Distro {
                name: name.clone(),
                check: if running.contains(name) {
                    Check::Checking
                } else {
                    Check::Stopped
                },
            })
            .collect();
        let programs = self.distro_programs();
        for name in running.iter().filter(|name| all.contains(name)) {
            let mut args = vec!["-c".to_owned(), PROBE.to_owned(), "sh".to_owned()];
            args.extend(programs.iter().cloned());
            let distro = name.clone();
            self.start_job(
                JobSpec {
                    program: "sh".to_owned(),
                    args,
                    directory: wsl::share(name, "/"),
                    env: Vec::new(),
                    label: "list-tools",
                    name: format!("tools in {name}"),
                    buffer: None,
                    interactive: false,
                    input: None,
                },
                collected(move |editor, lines, worked| {
                    let mut probe = Probe::default();
                    for line in &lines {
                        probe.read(line);
                    }
                    let check = if worked && !probe.home.is_empty() {
                        Check::Known(probe)
                    } else {
                        Check::Failed(
                            lines
                                .last()
                                .cloned()
                                .unwrap_or_else(|| "it didn't answer".to_owned()),
                        )
                    };
                    editor.distro_checked(&distro, check);
                }),
            );
        }
        self.render_tools();
    }

    fn distro_checked(&mut self, name: &str, check: Check) {
        if let Some(distro) = self
            .toolbox
            .distros
            .iter_mut()
            .find(|distro| distro.name == name)
        {
            distro.check = check;
        }
        self.render_tools();
    }

    pub(in crate::editor) fn distro_home(&self, distro: &str) -> Option<String> {
        self.toolbox
            .distros
            .iter()
            .find(|known| known.name == distro)
            .and_then(Distro::probe)
            .map(|probe| probe.home.clone())
            .or_else(|| oxmacs_process::home_inside(distro))
    }

    pub(super) fn make_runnable_inside(&mut self, distro: &str, path: &str) {
        self.start_job(
            JobSpec {
                program: "chmod".to_owned(),
                args: vec!["+x".to_owned(), path.to_owned()],
                directory: wsl::share(distro, "/"),
                env: Vec::new(),
                label: "list-tools",
                name: format!("chmod +x {path}"),
                buffer: None,
                interactive: false,
                input: None,
            },
            |editor, _, event| {
                if matches!(event, JobEvent::Finished(_)) {
                    editor.render_tools();
                }
            },
        );
    }

    pub(in crate::editor) fn tool_in_distro(&self, distro: &str, name: &str) -> Option<PathBuf> {
        let tool = self.managed_tool(name)?;
        let home = self.distro_home(distro)?;
        tool.installed(&tools_root(distro, &home))
            .map(|(_, path)| path)
    }

    pub(super) fn distro_sections(
        &self,
        page: &mut Page,
        specs: &[ServerSpec],
        name_width: usize,
    ) -> Vec<(usize, String, Row)> {
        let mut rows = Vec::new();
        for distro in &self.toolbox.distros {
            page.heading(&format!("{} (WSL)", distro.name));
            let probe = match &distro.check {
                Check::Stopped => {
                    page.line(&[(
                        "  stopped; S on *WSL* starts it, then g checks it",
                        Some(Highlight::Comment),
                    )]);
                    continue;
                }
                Check::Checking => {
                    page.line(&[("  checking…", Some(Highlight::Comment))]);
                    continue;
                }
                Check::Failed(error) => {
                    page.line(&[(
                        &format!("  couldn't check it: {error}"),
                        Some(Highlight::Comment),
                    )]);
                    continue;
                }
                Check::Known(probe) => probe,
            };
            let mut row = |page: &mut Page, name: &str, found: bool, state: &str, kind: Row| {
                let (mark, highlight) = if found {
                    ("✓", Highlight::Success)
                } else {
                    ("✗", Highlight::Comment)
                };
                let line = page.line_count();
                page.line(&[
                    (&format!("  {name:<name_width$}"), None),
                    (mark, Some(highlight)),
                    (&format!(" {state}"), None),
                ]);
                rows.push((line, distro.name.clone(), kind));
            };
            let family = probe.family();
            for tool in self
                .toolbox
                .tools
                .iter()
                .filter(|tool| tool.form == Form::Program)
            {
                let root = tools_root(&distro.name, &probe.home);
                let state = match (
                    self.toolbox.installing_into(tool, &root),
                    tool.installed(&root),
                    probe.path_of(&tool.name),
                ) {
                    (Some(version), ..) => format!("installing {version}…"),
                    (None, Some((version, _)), _) => format!("{version} (oxmacs)"),
                    (None, None, Some(path)) => format!("found {path}"),
                    (None, None, None) => "not installed; i installs its Linux build".to_owned(),
                };
                let found = state.contains("(oxmacs)") || state.starts_with("found");
                row(page, &tool.name, found, &state, Row::Tool(tool.clone()));
            }
            for spec in specs {
                let found = spec
                    .programs
                    .iter()
                    .find_map(|(program, _)| probe.path_of(program));
                let state = match (found, spec.package_for(&family)) {
                    (Some(path), _) => format!("found {path}"),
                    (None, Some((_, package))) => match package_install(&family, package) {
                        Some((manager, _)) => {
                            format!("not installed; i installs {package} with {manager}")
                        }
                        None => format!("not installed; {}", spec.install),
                    },
                    (None, None) if spec.runs => {
                        format!("not installed; i runs {} there", spec.install)
                    }
                    (None, None) => format!("not installed; {}", spec.install),
                };
                row(
                    page,
                    &spec.name,
                    found.is_some(),
                    &state,
                    Row::Server(spec.clone()),
                );
            }
            let state = match (
                probe.path_of(WATCHER),
                package_install(&family, WATCHER_PACKAGE),
            ) {
                (Some(path), _) => format!("found {path}; changes show at once"),
                (None, Some((manager, _))) => format!(
                    "not installed, so changes are checked every 2 seconds; i installs {WATCHER_PACKAGE} with {manager}"
                ),
                (None, None) => format!(
                    "not installed, so changes are checked every 2 seconds; install {WATCHER_PACKAGE}"
                ),
            };
            row(
                page,
                WATCHER,
                probe.path_of(WATCHER).is_some(),
                &state,
                Row::Watcher,
            );
        }
        rows
    }

    pub(super) fn distro_row_at_point(&self) -> Option<(String, Row)> {
        let line = self.point_line();
        self.buffers
            .special::<super::ToolList>(0)?
            .distro_rows
            .iter()
            .find(|(row, ..)| *row == line)
            .map(|(_, distro, kind)| (distro.clone(), kind.clone()))
    }

    fn known_probe(&self, distro: &str) -> Option<Probe> {
        self.toolbox
            .distros
            .iter()
            .find(|known| known.name == distro)
            .and_then(Distro::probe)
            .cloned()
    }

    fn install_as_root(&mut self, distro: &str, what: &str, command: &str) {
        let line = format!("wsl.exe -d {distro} -u root --exec sh -c \"{command}\"");
        let directory = self.outside_distros();
        self.ask(
            &format!("Install {what} in {distro} as root with `{command}`?"),
            move |editor, yes| {
                if yes {
                    editor.start_compile(&line, directory.clone());
                    editor.show_message("When it finishes, g checks again");
                }
            },
        );
    }

    pub(super) fn distro_install(&mut self) -> bool {
        let Some((distro, kind)) = self.distro_row_at_point() else {
            return false;
        };
        let Some(probe) = self.known_probe(&distro) else {
            return true;
        };
        let family = probe.family();
        match kind {
            Row::Watcher => {
                if probe.path_of(WATCHER).is_some() {
                    self.show_message(format!("{WATCHER} is already installed in {distro}"));
                } else if let Some((_, command)) = package_install(&family, WATCHER_PACKAGE) {
                    self.install_as_root(&distro, WATCHER_PACKAGE, &command);
                } else {
                    self.show_message(format!(
                        "No package manager oxmacs knows in {distro}; install {WATCHER_PACKAGE} there"
                    ));
                }
            }
            Row::Server(spec) => {
                let found = spec
                    .programs
                    .iter()
                    .any(|(program, _)| probe.path_of(program).is_some());
                if found {
                    self.show_message(format!("{} is already installed in {distro}", spec.name));
                } else if let Some((_, package)) = spec.package_for(&family)
                    && let Some((_, command)) = package_install(&family, package)
                {
                    self.install_as_root(&distro, package, &command);
                } else if spec.runs {
                    let directory = wsl::share(&distro, &probe.home);
                    let install = spec.install.clone();
                    self.ask(
                        &format!("Run `{install}` in {distro} to install {}?", spec.name),
                        move |editor, yes| {
                            if yes {
                                editor.retry_servers();
                                editor.start_compile(&install, directory.clone());
                                editor.show_message("When it finishes, g checks again");
                            }
                        },
                    );
                } else {
                    self.show_message(format!("{}: {}", spec.name, spec.install));
                }
            }
            Row::Tool(tool) => {
                let root = tools_root(&distro, &probe.home);
                if let Some((version, _)) = tool.installed(&root) {
                    self.show_message(format!(
                        "{} {version} is already installed in {distro}",
                        tool.name
                    ));
                } else {
                    self.start_install_into(&tool, root, "linux", &probe.arch);
                }
            }
        }
        true
    }

    pub(super) fn distro_update(&mut self) -> bool {
        let Some((distro, kind)) = self.distro_row_at_point() else {
            return false;
        };
        let (Row::Tool(tool), Some(probe)) = (kind, self.known_probe(&distro)) else {
            self.show_message("u updates the tools oxmacs installs");
            return true;
        };
        let root = tools_root(&distro, &probe.home);
        self.update_into(&tool, root, "linux", &probe.arch);
        true
    }

    pub(super) fn distro_remove(&mut self) -> bool {
        let Some((distro, kind)) = self.distro_row_at_point() else {
            return false;
        };
        let (Row::Tool(tool), Some(probe)) = (kind, self.known_probe(&distro)) else {
            self.show_message("d removes the tools oxmacs installs");
            return true;
        };
        let root = tools_root(&distro, &probe.home);
        let Some((version, _)) = tool.installed(&root) else {
            self.show_message(format!(
                "{} is not installed by oxmacs in {distro}",
                tool.name
            ));
            return true;
        };
        self.ask(
            &format!("Remove {} {version} from {distro}?", tool.name),
            move |editor, yes| {
                if yes {
                    match managed::remove(&tool, &root) {
                        Ok(()) => {
                            editor.show_message(format!("Removed {} from {distro}", tool.name))
                        }
                        Err(error) => editor.show_message(format!(
                            "Could not remove {} from {distro}: {error}",
                            tool.name
                        )),
                    }
                    editor.render_tools();
                }
            },
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Effect, Job};
    use crate::testing::{air, kits, lines, press, question, type_text};

    const WSL: &str = r"C:\Program Files\WSL\wsl.exe";
    const FEDORA: &str = "FedoraLinux-44";

    fn server(name: &str, language: &str, install: &str, packages: &[(&str, &str)]) -> ServerSpec {
        ServerSpec {
            name: name.to_owned(),
            languages: vec![(language.to_owned(), language.to_lowercase())],
            programs: vec![(name.to_owned(), Vec::new())],
            install: install.to_owned(),
            runs: packages.is_empty(),
            packages: packages
                .iter()
                .map(|(family, package)| ((*family).to_owned(), (*package).to_owned()))
                .collect(),
        }
    }

    fn spawned(editor: &mut Editor) -> Vec<Job> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Spawn(job) => Some(job),
                _ => None,
            })
            .collect()
    }

    fn finish(editor: &mut Editor, job: &Job, output: &[&str]) {
        for line in output {
            editor.job_output(job.id, line);
        }
        editor.job_finished(job.id, &Outcome::Exited(Some(0)));
    }

    fn release(version: &str) -> String {
        let archive = "air-x86_64-unknown-linux-gnu.tar.gz";
        format!(
            r#"{{"tag_name": "{version}", "assets": [
                {{"name": "{archive}", "browser_download_url": "https://example.com/{archive}", "size": 5347737}},
                {{"name": "{archive}.sha256", "browser_download_url": "https://example.com/{archive}.sha256", "size": 96}}
            ]}}"#
        )
    }

    fn listed() -> (tempfile::TempDir, Editor, Job) {
        let root = tempfile::tempdir().unwrap();
        let mut editor = kits();
        editor.add_managed_tool(air());
        editor.tools_root = Some(root.path().to_owned());
        editor.add_language_servers(&[
            server(
                "rust-analyzer",
                "Rust",
                "rustup component add rust-analyzer",
                &[],
            ),
            server(
                "packaged-ls",
                "R",
                "install it from your system's packages",
                &[("fedora", "packaged-ls-tools")],
            ),
        ]);
        press(&mut editor, "M-x");
        type_text(&mut editor, "list-tools");
        press(&mut editor, "RET");
        let checks: Vec<_> = editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Fetch(id, _) => Some(id),
                _ => None,
            })
            .collect();
        for id in checks {
            editor.fetched(id, Ok(release("0.12.0")));
        }
        editor.check_distros_with(PathBuf::from(WSL));
        let all = spawned(&mut editor).pop().unwrap();
        assert_eq!(
            (all.program.as_str(), all.args.as_slice()),
            (WSL, ["--list", "--quiet"].map(str::to_owned).as_slice())
        );
        assert!(all.env.contains(&("WSL_UTF8".to_owned(), "1".to_owned())));
        finish(&mut editor, &all, &[FEDORA, "docker-desktop"]);
        let running = spawned(&mut editor).pop().unwrap();
        assert_eq!(running.args, ["--list", "--running", "--quiet"]);
        finish(&mut editor, &running, &[FEDORA]);
        let probe = spawned(&mut editor).pop().unwrap();
        (root, editor, probe)
    }

    fn checked() -> (tempfile::TempDir, Editor) {
        let (root, mut editor, probe) = listed();
        finish(
            &mut editor,
            &probe,
            &[
                "family\tfedora ",
                "arch\tx86_64",
                "home\t/home/ana",
                "found\tinotifywait\t",
                "found\tpackaged-ls\t",
                "found\trust-analyzer\t/home/ana/.cargo/bin/rust-analyzer",
            ],
        );
        (root, editor)
    }

    fn section(editor: &Editor, distro: &str) -> Vec<String> {
        let shown = lines(editor);
        let heading = format!("{distro} (WSL)");
        shown
            .iter()
            .skip_while(|line| **line != heading)
            .skip(1)
            .take_while(|line| line.starts_with("  "))
            .cloned()
            .collect()
    }

    fn go_to(editor: &mut Editor, distro: &str, name: &str) {
        let heading = format!("{distro} (WSL)");
        let shown = lines(editor);
        let start = shown.iter().position(|line| *line == heading).unwrap();
        let line = (start..shown.len())
            .find(|line| shown[*line].starts_with(&format!("  {name} ")))
            .unwrap();
        let point = editor.buffer().line_start(line);
        editor.buffer_mut().set_point(point);
    }

    #[test]
    fn running_distributions_are_checked_inside_and_stopped_ones_left_alone() {
        let (_root, editor, probe) = listed();

        assert_eq!(probe.program, "sh");
        assert_eq!(probe.directory, wsl::share(FEDORA, "/"));
        assert_eq!(
            probe.args[..3],
            ["-c".to_owned(), PROBE.to_owned(), "sh".to_owned()]
        );
        assert_eq!(
            probe.args[3..],
            ["inotifywait", "packaged-ls", "rust-analyzer"]
        );
        assert_eq!(section(&editor, FEDORA), ["  checking…"]);
        assert_eq!(
            section(&editor, "docker-desktop"),
            ["  stopped; S on *WSL* starts it, then g checks it"]
        );
    }

    #[test]
    fn each_distribution_lists_its_own_tools_with_what_installs_them() {
        let (_root, editor) = checked();

        let shown = section(&editor, FEDORA);
        let row = |name: &str| {
            shown
                .iter()
                .find(|line| line.starts_with(&format!("  {name} ")))
                .cloned()
                .unwrap_or_default()
        };
        assert!(
            row("air").ends_with("✗ not installed; i installs its Linux build"),
            "{shown:?}"
        );
        assert!(
            row("rust-analyzer").ends_with("✓ found /home/ana/.cargo/bin/rust-analyzer"),
            "{shown:?}"
        );
        assert!(
            row("packaged-ls").ends_with("✗ not installed; i installs packaged-ls-tools with dnf"),
            "{shown:?}"
        );
        assert!(
            row("inotifywait").ends_with("✗ not installed, so changes are checked every 2 seconds; i installs inotify-tools with dnf"),
            "{shown:?}"
        );
    }

    #[test]
    fn packages_install_as_root_through_wsl_after_asking() {
        let (_root, mut editor) = checked();
        go_to(&mut editor, FEDORA, "inotifywait");

        press(&mut editor, "i");
        assert!(
            question(&editor).is_some_and(|asked| asked.starts_with(
                "Install inotify-tools in FedoraLinux-44 as root with `dnf install -y inotify-tools`?"
            )),
            "{:?}",
            question(&editor)
        );
        press(&mut editor, "y");

        let job = spawned(&mut editor).pop().unwrap();
        assert_eq!(job.program, "wsl.exe");
        assert_eq!(
            job.args,
            [
                "-d",
                FEDORA,
                "-u",
                "root",
                "--exec",
                "sh",
                "-c",
                "dnf install -y inotify-tools"
            ]
        );
        assert!(wsl::place(&job.directory).is_none());
    }

    #[test]
    fn servers_with_an_install_command_run_it_in_the_distribution() {
        let (_root, mut editor, probe) = listed();
        finish(
            &mut editor,
            &probe,
            &["family\tubuntu debian", "arch\tx86_64", "home\t/home/ana"],
        );
        go_to(&mut editor, FEDORA, "rust-analyzer");

        press(&mut editor, "i y");

        let job = spawned(&mut editor).pop().unwrap();
        assert_eq!(
            (job.program.as_str(), job.args.as_slice()),
            (
                "rustup",
                ["component", "add", "rust-analyzer"]
                    .map(str::to_owned)
                    .as_slice()
            )
        );
        assert_eq!(job.directory, wsl::share(FEDORA, "/home/ana"));
        assert_eq!(
            package_install(&["ubuntu", "debian"], "inotify-tools"),
            Some((
                "apt",
                "apt-get update && apt-get install -y inotify-tools".to_owned()
            ))
        );
        assert_eq!(package_install(&["gentoo"], "inotify-tools"), None);
    }

    #[test]
    fn oxmacs_tools_install_their_linux_build_into_the_distribution() {
        let (_root, mut editor) = checked();
        go_to(&mut editor, FEDORA, "air");

        press(&mut editor, "i");

        let install = editor
            .take_effects()
            .into_iter()
            .find_map(|effect| match effect {
                Effect::InstallTool(install) => Some(*install),
                _ => None,
            })
            .unwrap();
        let root = tools_root(FEDORA, "/home/ana");
        assert_eq!(install.archive.name, "air-x86_64-unknown-linux-gnu.tar.gz");
        assert_eq!(install.root, root);
        assert!(section(&editor, FEDORA)[0].contains("installing 0.12.0…"));

        editor.tool_installed(&air(), Ok(root.join("air").join("0.12.0").join("air")));

        assert_eq!(
            editor.message(),
            Some("Installed air 0.12.0 in FedoraLinux-44")
        );
        let chmod = spawned(&mut editor).pop().unwrap();
        assert_eq!(
            (chmod.program.as_str(), chmod.args.as_slice()),
            (
                "chmod",
                ["+x", "/home/ana/.local/share/oxmacs/tools/air/0.12.0/air"]
                    .map(str::to_owned)
                    .as_slice()
            )
        );
        assert_eq!(chmod.directory, wsl::share(FEDORA, "/"));
    }
}
