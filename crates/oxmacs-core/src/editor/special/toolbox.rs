use std::env;
use std::path::PathBuf;

use crate::display::LineStyle;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::editor::{Editor, Effect, FetchId};
use crate::external::managed::{self, Install, MANAGED, Managed, Origin, Release};
use crate::files::paths;
use crate::text::Buffer;
use crate::theme::Highlight;

const NAME: &str = "*Tools*";
const HELP: &str = "i install  u update  d remove  g check again  q quit";
const COLUMNS: [&str; 5] = ["Name", "Installed", "Latest", "Status", "Runs from"];
const MEGABYTE: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Latest {
    Checking,
    Known(Release),
    Failed,
}

#[derive(Debug, Default)]
pub(in crate::editor) struct Toolbox {
    latest: Vec<(Managed, Latest)>,
    checks: Vec<(FetchId, Managed)>,
    installing: Vec<(Managed, String)>,
}

impl Toolbox {
    fn latest(&self, tool: Managed) -> Option<&Latest> {
        self.latest
            .iter()
            .find(|(known, _)| *known == tool)
            .map(|(_, latest)| latest)
    }

    fn set_latest(&mut self, tool: Managed, latest: Latest) {
        self.latest.retain(|(known, _)| *known != tool);
        self.latest.push((tool, latest));
    }

    fn installing(&self, tool: Managed) -> Option<&str> {
        self.installing
            .iter()
            .find(|(known, _)| *known == tool)
            .map(|(_, version)| version.as_str())
    }
}

#[derive(Debug, Default)]
pub(in crate::editor) struct ToolList {
    rows: Vec<(usize, Managed)>,
    styles: Vec<LineStyle>,
}

impl ToolList {
    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    fn row(&self, line: usize) -> Option<Managed> {
        self.rows
            .iter()
            .find(|(row, _)| *row == line)
            .map(|(_, tool)| *tool)
    }
}

fn status(
    installed: Option<&str>,
    latest: Option<&Latest>,
    installing: Option<&str>,
) -> (String, Highlight) {
    if let Some(version) = installing {
        return (format!("installing {version}…"), Highlight::Warning);
    }
    match (installed, latest) {
        (Some(have), Some(Latest::Known(release))) if managed::newer(&release.version, have) => {
            ("update available".to_owned(), Highlight::Warning)
        }
        (Some(_), Some(Latest::Known(_))) => ("up to date".to_owned(), Highlight::Success),
        (Some(_), _) => ("installed".to_owned(), Highlight::Success),
        (None, _) => ("not installed".to_owned(), Highlight::Comment),
    }
}

fn latest_cell(latest: Option<&Latest>, tool: Managed) -> String {
    match latest {
        Some(Latest::Checking) | None => "checking…".to_owned(),
        Some(Latest::Failed) => "check failed".to_owned(),
        Some(Latest::Known(release)) => {
            let size = tool
                .archive_here()
                .and_then(|name| release.asset(&name))
                .map(|asset| format!(" ({})", megabytes(asset.size)))
                .unwrap_or_default();
            format!("{}{size}", release.version)
        }
    }
}

fn megabytes(bytes: u64) -> String {
    let tenths = (bytes * 10 + MEGABYTE / 2) / MEGABYTE;
    format!("{}.{} MB", tenths / 10, tenths % 10)
}

impl Editor {
    fn tools_index(&self) -> Option<usize> {
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::Tools(_)))
    }

    fn tool_setting(&self, tool: Managed) -> Option<&str> {
        (tool == managed::AIR)
            .then_some(self.config.air_program.as_deref())
            .flatten()
    }

    fn installed_tool(&self, tool: Managed) -> Option<(String, PathBuf)> {
        self.tools_root
            .as_deref()
            .and_then(|root| tool.installed(root))
    }

    pub(in crate::editor) fn list_tools(&mut self) {
        let created = self.tools_index().is_none();
        if created {
            self.beside(|editor| {
                let mut buffer = Buffer::with_text(NAME, "");
                buffer.set_read_only(true);
                editor.add_buffer_with(buffer, Mode::Tools(ToolList::default()));
            });
        }
        self.check_tool_releases();
        self.show_beside(Self::tools_index);
        if created
            && let Some(Mode::Tools(list)) = self.buffers.mode(0)
            && let Some((line, _)) = list.rows.first()
        {
            let line = *line;
            let buffer = self.buffers.current_mut();
            let start = buffer.line_start(line);
            buffer.set_point(start);
        }
    }

    pub(in crate::editor) fn check_tool_releases(&mut self) {
        for tool in MANAGED {
            let id = FetchId(self.next_fetch);
            self.next_fetch += 1;
            self.toolbox.checks.retain(|(_, known)| *known != tool);
            self.toolbox.checks.push((id, tool));
            self.toolbox.set_latest(tool, Latest::Checking);
            self.effects.push(Effect::Fetch(id, tool.latest_url()));
        }
        self.render_tools();
    }

    pub(in crate::editor) fn tool_release_fetched(
        &mut self,
        id: FetchId,
        result: &Result<String, String>,
    ) -> bool {
        let Some(position) = self
            .toolbox
            .checks
            .iter()
            .position(|(known, _)| *known == id)
        else {
            return false;
        };
        let (_, tool) = self.toolbox.checks.remove(position);
        let release = result
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|json| managed::parse_release(json));
        let latest = match release {
            Ok(release) => Latest::Known(release),
            Err(error) => {
                self.show_message(format!("Could not check {}: {error}", tool.name));
                Latest::Failed
            }
        };
        self.toolbox.set_latest(tool, latest);
        self.render_tools();
        true
    }

    fn tool_at_point(&mut self) -> Option<Managed> {
        let line = self.point_line();
        let tool = match self.buffers.mode(0) {
            Some(Mode::Tools(list)) => list.row(line),
            _ => None,
        };
        if tool.is_none() {
            self.show_message("No tool on this line");
        }
        tool
    }

    pub(in crate::editor) fn tool_install(&mut self) {
        let Some(tool) = self.tool_at_point() else {
            return;
        };
        if let Some((version, _)) = self.installed_tool(tool) {
            self.show_message(format!("{} {version} is already installed", tool.name));
            return;
        }
        self.start_install(tool);
    }

    pub(in crate::editor) fn tool_update(&mut self) {
        let Some(tool) = self.tool_at_point() else {
            return;
        };
        let Some((version, _)) = self.installed_tool(tool) else {
            self.show_message(format!("{} is not installed; i installs it", tool.name));
            return;
        };
        match self.toolbox.latest(tool) {
            Some(Latest::Known(release)) if managed::newer(&release.version, &version) => {
                self.start_install(tool);
            }
            Some(Latest::Known(_)) => {
                self.show_message(format!("{} {version} is up to date", tool.name));
            }
            _ => self.start_install(tool),
        }
    }

    fn start_install(&mut self, tool: Managed) {
        if self.toolbox.installing(tool).is_some() {
            self.show_message(format!("Already installing {}", tool.name));
            return;
        }
        let release = match self.toolbox.latest(tool) {
            Some(Latest::Known(release)) => release.clone(),
            Some(Latest::Checking) => {
                self.show_message(format!("Still checking for {}'s latest release", tool.name));
                return;
            }
            Some(Latest::Failed) | None => {
                self.show_message(format!(
                    "Could not check {}'s latest release; g tries again",
                    tool.name
                ));
                return;
            }
        };
        let Some(root) = self.tools_root.clone() else {
            self.show_message("No data folder to install tools into");
            return;
        };
        let assets = tool.archive_here().and_then(|name| {
            let archive = release.asset(&name)?.clone();
            let checksum = release.asset(&format!("{name}.sha256"))?.clone();
            Some((archive, checksum))
        });
        let Some((archive, checksum)) = assets else {
            self.show_message(format!(
                "{} {} has no build for this system",
                tool.name, release.version
            ));
            return;
        };
        self.toolbox
            .installing
            .push((tool, release.version.clone()));
        self.effects.push(Effect::InstallTool(Install {
            tool,
            version: release.version.clone(),
            archive,
            checksum,
            root,
        }));
        self.show_message(format!("Installing {} {}…", tool.name, release.version));
        self.render_tools();
    }

    pub fn tool_installed(&mut self, tool: Managed, result: Result<PathBuf, String>) {
        let position = self
            .toolbox
            .installing
            .iter()
            .position(|(known, _)| *known == tool);
        let version = position
            .map(|position| self.toolbox.installing.remove(position).1)
            .unwrap_or_default();
        match result {
            Ok(_) => self.show_message(format!("Installed {} {version}", tool.name)),
            Err(error) => self.show_message(format!("Could not install {}: {error}", tool.name)),
        }
        self.render_tools();
    }

    pub(in crate::editor) fn tool_remove(&mut self) {
        let Some(tool) = self.tool_at_point() else {
            return;
        };
        let Some((version, _)) = self.installed_tool(tool) else {
            self.show_message(format!("{} is not installed by oxmacs", tool.name));
            return;
        };
        self.ask_to_remove_tool(tool, &version);
    }

    pub(in crate::editor) fn finish_tool_removal(&mut self, tool: Managed) {
        let Some(root) = self.tools_root.clone() else {
            return;
        };
        match managed::remove(tool, &root) {
            Ok(()) => self.show_message(format!("Removed {}", tool.name)),
            Err(error) => self.show_message(format!("Could not remove {}: {error}", tool.name)),
        }
        self.render_tools();
    }

    pub(in crate::editor) fn render_tools(&mut self) {
        let Some(index) = self.tools_index() else {
            return;
        };
        let search = env::var_os("PATH");
        let cells: Vec<([String; 5], Highlight)> = MANAGED
            .iter()
            .map(|tool| {
                let installed = self.installed_tool(*tool).map(|(version, _)| version);
                let latest = self.toolbox.latest(*tool);
                let (status, highlight) =
                    status(installed.as_deref(), latest, self.toolbox.installing(*tool));
                let runs = tool
                    .locate(
                        self.tool_setting(*tool),
                        self.tools_root.as_deref(),
                        search.as_deref(),
                    )
                    .map_or_else(
                        || "–".to_owned(),
                        |(path, origin)| {
                            let from = match origin {
                                Origin::Setting => "air-program",
                                Origin::Managed => "oxmacs",
                                Origin::Path => "PATH",
                            };
                            format!(
                                "{} ({from})",
                                paths::abbreviate(&path, self.home.as_deref())
                            )
                        },
                    );
                (
                    [
                        tool.name.to_owned(),
                        installed.unwrap_or_else(|| "–".to_owned()),
                        latest_cell(latest, *tool),
                        status,
                        runs,
                    ],
                    highlight,
                )
            })
            .collect();
        let header = COLUMNS.map(str::to_owned);
        let widths: [usize; 5] = std::array::from_fn(|column| {
            cells
                .iter()
                .map(|(row, _)| row)
                .chain(std::iter::once(&header))
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(0)
        });
        let layout = |row: &[String; 5]| -> [String; 5] {
            std::array::from_fn(|column| {
                if column + 1 == row.len() {
                    row[column].clone()
                } else {
                    format!("{:<width$}  ", row[column], width = widths[column])
                }
            })
        };
        let mut page = Page::default();
        page.heading("Tools");
        page.line(&[(HELP, Some(Highlight::Comment))]);
        page.line(&[]);
        let titles: String = layout(&header).concat();
        page.line(&[(titles.trim_end(), Some(Highlight::Comment))]);
        let mut rows = Vec::new();
        for (tool, (row, highlight)) in MANAGED.iter().zip(&cells) {
            let [name, installed, latest, status, runs] = layout(row);
            rows.push((page.styles.len(), *tool));
            page.line(&[
                (&name, Some(Highlight::Link)),
                (&installed, None),
                (&latest, None),
                (&status, Some(*highlight)),
                (&runs, Some(Highlight::Comment)),
            ]);
            page.line(&[(
                &format!("  {} · github.com/{}", tool.summary, tool.repository),
                Some(Highlight::Comment),
            )]);
        }
        if let Some((buffer, Mode::Tools(list))) = self.buffers.entry_mut(index) {
            page.fill(buffer);
            list.rows = rows;
            list.styles = page.styles;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::editor::test_support::{lines, press, question, type_text};
    use crate::editor::{Editor, Effect, FetchId};
    use crate::external::managed::{AIR, Install};

    fn release(version: &str) -> String {
        let archive = AIR.archive_here().unwrap();
        format!(
            r#"{{"tag_name": "{version}", "assets": [
                {{"name": "{archive}", "browser_download_url": "https://example.com/{archive}", "size": 5347737}},
                {{"name": "{archive}.sha256", "browser_download_url": "https://example.com/{archive}.sha256", "size": 96}}
            ]}}"#
        )
    }

    fn place(root: &Path, version: &str) {
        let folder = root.join("air").join(version);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(AIR.binary()), "binary").unwrap();
    }

    fn opened(root: &Path) -> Editor {
        let mut editor = Editor::new();
        editor.tools_root = Some(root.to_owned());
        press(&mut editor, "M-x");
        type_text(&mut editor, "list-tools");
        press(&mut editor, "RET");
        editor
    }

    fn checks(editor: &mut Editor) -> Vec<(FetchId, String)> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Fetch(id, url) => Some((id, url)),
                _ => None,
            })
            .collect()
    }

    fn installs(editor: &mut Editor) -> Vec<Install> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::InstallTool(install) => Some(install),
                _ => None,
            })
            .collect()
    }

    fn answered(root: &Path, version: &str) -> Editor {
        let mut editor = opened(root);
        let (id, _) = checks(&mut editor).pop().unwrap();
        editor.fetched(id, Ok(release(version)));
        editor
    }

    fn air_line(editor: &Editor) -> String {
        lines(editor)
            .into_iter()
            .find(|line| line.starts_with("air "))
            .unwrap()
    }

    #[test]
    fn list_tools_shows_air_and_checks_for_its_latest_release() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = opened(root.path());

        assert_eq!(editor.buffer().name(), "*Tools*");
        assert!(
            air_line(&editor).contains("checking…"),
            "{:?}",
            lines(&editor)
        );
        assert!(air_line(&editor).contains("not installed"));
        assert_eq!(
            checks(&mut editor)
                .into_iter()
                .map(|(_, url)| url)
                .collect::<Vec<_>>(),
            ["https://api.github.com/repos/posit-dev/air/releases/latest"]
        );
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 4);
    }

    #[test]
    fn a_newer_release_offers_an_update() {
        let root = tempfile::tempdir().unwrap();
        place(root.path(), "0.11.0");

        let editor = answered(root.path(), "0.12.0");

        let line = air_line(&editor);
        assert!(line.contains("0.11.0"), "{line}");
        assert!(line.contains("0.12.0 (5.1 MB)"), "{line}");
        assert!(line.contains("update available"), "{line}");
        assert!(
            line.contains("0.11.0") && line.ends_with("(oxmacs)"),
            "{line}"
        );
    }

    #[test]
    fn i_installs_the_latest_release_for_this_system() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = answered(root.path(), "0.12.0");

        press(&mut editor, "i");

        let archive = AIR.archive_here().unwrap();
        let install = installs(&mut editor).pop().unwrap();
        assert_eq!(install.version, "0.12.0");
        assert_eq!(install.archive.name, archive);
        assert_eq!(install.checksum.name, format!("{archive}.sha256"));
        assert_eq!(install.root, root.path());
        assert!(air_line(&editor).contains("installing 0.12.0…"));

        place(root.path(), "0.12.0");
        editor.tool_installed(AIR, Ok(root.path().join("air")));

        assert_eq!(editor.message(), Some("Installed air 0.12.0"));
        assert!(air_line(&editor).contains("up to date"));
    }

    #[test]
    fn install_failures_are_reported() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = answered(root.path(), "0.12.0");
        press(&mut editor, "i");

        editor.tool_installed(AIR, Err("connection reset".to_owned()));

        assert_eq!(
            editor.message(),
            Some("Could not install air: connection reset")
        );
        assert!(air_line(&editor).contains("not installed"));
    }

    #[test]
    fn u_updates_only_to_a_newer_release() {
        let root = tempfile::tempdir().unwrap();
        place(root.path(), "0.12.0");
        let mut editor = answered(root.path(), "0.12.0");

        press(&mut editor, "u");
        assert_eq!(editor.message(), Some("air 0.12.0 is up to date"));
        assert_eq!(installs(&mut editor), []);

        let (id, _) = {
            press(&mut editor, "g");
            checks(&mut editor).pop().unwrap()
        };
        editor.fetched(id, Ok(release("0.13.0")));
        press(&mut editor, "u");
        assert_eq!(installs(&mut editor).pop().unwrap().version, "0.13.0");
    }

    #[test]
    fn d_removes_oxmacss_copy_after_asking() {
        let root = tempfile::tempdir().unwrap();
        place(root.path(), "0.12.0");
        let mut editor = answered(root.path(), "0.12.0");

        press(&mut editor, "d");
        assert_eq!(
            question(&editor).as_deref(),
            Some("Remove air 0.12.0? (y or n) ")
        );
        press(&mut editor, "y");

        assert_eq!(editor.message(), Some("Removed air"));
        assert!(!root.path().join("air").exists());
        assert!(air_line(&editor).contains("not installed"));
    }

    #[test]
    fn failed_checks_say_why() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = opened(root.path());
        let (id, _) = checks(&mut editor).pop().unwrap();

        editor.fetched(id, Err("timed out".to_owned()));

        assert_eq!(editor.message(), Some("Could not check air: timed out"));
        assert!(air_line(&editor).contains("check failed"));
        press(&mut editor, "i");
        assert_eq!(
            editor.message(),
            Some("Could not check air's latest release; g tries again")
        );
    }

    #[test]
    fn keys_off_a_tool_line_say_so() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = answered(root.path(), "0.12.0");

        press(&mut editor, "M-< i");

        assert_eq!(editor.message(), Some("No tool on this line"));
    }
}
