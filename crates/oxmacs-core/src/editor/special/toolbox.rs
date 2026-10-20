use std::env;
use std::ops::Range;
use std::path::PathBuf;

use crate::display::LineStyle;
use crate::editor::buffers::Mode;
use crate::editor::page::{self, Page};
use crate::editor::{Editor, Effect, FetchId};
use crate::external::managed::{self, Checksum, Expected, Install, Managed, Origin, Release};
use crate::files::paths;
use crate::text::Buffer;
use crate::theme::Highlight;

const NAME: &str = "*Tools*";
const HELP: &str = "i install · u update · d remove · g check again · q quit";
const INDENT: &str = "  ";
const LABEL: usize = 11;
const ROOM: usize = 100;
const MEGABYTE: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Latest {
    Checking,
    Known(Release),
    Failed,
}

#[derive(Debug)]
pub(in crate::editor) struct Toolbox {
    tools: Vec<Managed>,
    latest: Vec<(Managed, Latest)>,
    checks: Vec<(FetchId, Managed)>,
    installing: Vec<(Managed, String)>,
}

impl Default for Toolbox {
    fn default() -> Self {
        Self {
            tools: vec![managed::AIR],
            latest: Vec::new(),
            checks: Vec::new(),
            installing: Vec::new(),
        }
    }
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
    rows: Vec<(Range<usize>, Managed)>,
    styles: Vec<LineStyle>,
}

impl ToolList {
    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    fn row(&self, line: usize) -> Option<Managed> {
        self.rows
            .iter()
            .find(|(lines, _)| lines.contains(&line))
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

    pub fn set_tools_root(&mut self, root: Option<PathBuf>) {
        self.tools_root = root;
    }

    pub fn add_managed_tool(&mut self, tool: Managed) {
        if !self.toolbox.tools.contains(&tool) {
            self.toolbox.tools.push(tool);
        }
    }

    pub fn installed_tool(&self, tool: Managed) -> Option<(String, PathBuf)> {
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
            && let Some((lines, _)) = list.rows.first()
        {
            let line = lines.start;
            let buffer = self.buffers.current_mut();
            let start = buffer.line_start(line);
            buffer.set_point(start);
        }
    }

    pub(in crate::editor) fn check_tool_releases(&mut self) {
        for tool in self.toolbox.tools.clone() {
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
            .and_then(|json| tool.parse_release(json));
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
            let checksum = match tool.checksum {
                Checksum::Sidecar => {
                    Expected::Download(release.asset(&format!("{name}.sha256"))?.clone())
                }
                Checksum::Digest => Expected::Digest(archive.digest.clone()?),
            };
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
        self.effects.push(Effect::InstallTool(Box::new(Install {
            tool,
            version: release.version.clone(),
            archive,
            checksum,
            root,
        })));
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

    fn tool_card(&self, page: &mut Page, tool: Managed) {
        let installed = self.installed_tool(tool).map(|(version, _)| version);
        let latest = self.toolbox.latest(tool);
        let (status, highlight) =
            status(installed.as_deref(), latest, self.toolbox.installing(tool));
        let search = env::var_os("PATH");
        let runs = tool
            .locate(
                self.tool_setting(tool),
                self.tools_root.as_deref(),
                search.as_deref(),
            )
            .map_or_else(
                || "nothing yet".to_owned(),
                |(path, origin)| {
                    let from = match origin {
                        Origin::Setting => tool.setting.unwrap_or("a setting"),
                        Origin::Managed => "oxmacs",
                        Origin::Path => "PATH",
                    };
                    format!(
                        "{} ({from})",
                        paths::abbreviate(&path, self.home.as_deref())
                    )
                },
            );
        page.line(&[
            (tool.name, Some(Highlight::Heading)),
            ("  ", None),
            (&status, Some(highlight)),
        ]);
        page.line(&[(
            &format!("{INDENT}{}", tool.summary),
            Some(Highlight::Comment),
        )]);
        let field = |page: &mut Page, label: &str, value: &str| {
            let label = format!("{INDENT}{label:<LABEL$}");
            page.line(&[(&label, Some(Highlight::Comment)), (value, None)]);
        };
        field(
            page,
            "Installed",
            installed.as_deref().unwrap_or("not by oxmacs"),
        );
        field(page, "Latest", &latest_cell(latest, tool));
        field(page, "Runs from", &runs);
        field(page, "Source", &format!("github.com/{}", tool.repository));
    }

    pub(in crate::editor) fn render_tools(&mut self) {
        let Some(index) = self.tools_index() else {
            return;
        };
        let room = self
            .buffers
            .get(index)
            .and_then(|buffer| self.width_showing(buffer.id()))
            .filter(|width| *width > 0)
            .unwrap_or(ROOM);
        let mut page = Page::default();
        page.heading("Tools");
        for line in page::fill(HELP, room) {
            page.line(&[(&line, Some(Highlight::Comment))]);
        }
        let mut rows = Vec::new();
        for tool in self.toolbox.tools.clone() {
            page.line(&[]);
            let start = page.styles.len();
            self.tool_card(&mut page, tool);
            rows.push((start..page.styles.len(), tool));
        }
        if let Some((buffer, Mode::Tools(list))) = self.buffers.entry_mut(index) {
            let point = buffer.point();
            let line = buffer.line_of(point);
            let column = point - buffer.line_start(line);
            let on_card = list.rows.iter().find_map(|(lines, tool)| {
                lines.contains(&line).then(|| (*tool, line - lines.start))
            });
            page.fill(buffer);
            let moved = on_card.and_then(|(tool, offset)| {
                rows.iter()
                    .find(|(_, known)| *known == tool)
                    .map(|(lines, _)| lines.start + offset)
            });
            if let Some(line) = moved {
                let start = buffer.line_start(line);
                buffer.set_point((start + column).min(buffer.line_end(line)));
            }
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
    use std::env;

    use crate::external::managed::{AIR, Checksum, Expected, Form, Install, Managed};

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
                Effect::InstallTool(install) => Some(*install),
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

    fn card(editor: &Editor) -> Vec<String> {
        lines(editor)
            .into_iter()
            .skip_while(|line| !line.starts_with("air  "))
            .take(6)
            .collect()
    }

    fn air_line(editor: &Editor) -> String {
        card(editor).join("\n")
    }

    #[test]
    fn list_tools_shows_air_and_checks_for_its_latest_release() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = opened(root.path());

        assert_eq!(editor.buffer().name(), "*Tools*");
        assert_eq!(
            card(&editor)[..4],
            [
                "air  not installed",
                "  R formatter",
                "  Installed  not by oxmacs",
                "  Latest     checking…",
            ]
        );
        assert!(card(&editor)[4].starts_with("  Runs from  "));
        assert_eq!(card(&editor)[5], "  Source     github.com/posit-dev/air");
        assert_eq!(
            checks(&mut editor)
                .into_iter()
                .map(|(_, url)| url)
                .collect::<Vec<_>>(),
            ["https://api.github.com/repos/posit-dev/air/releases/latest"]
        );
        let line = editor.buffer().line_of(editor.buffer().point());
        assert_eq!(lines(&editor)[line], "air  not installed");
    }

    #[test]
    fn a_newer_release_offers_an_update() {
        let root = tempfile::tempdir().unwrap();
        place(root.path(), "0.11.0");

        let editor = answered(root.path(), "0.12.0");

        let card = card(&editor);
        assert_eq!(card[0], "air  update available");
        assert_eq!(card[2], "  Installed  0.11.0");
        assert_eq!(card[3], "  Latest     0.12.0 (5.1 MB)");
        assert!(
            card[4].contains("0.11.0") && card[4].ends_with("(oxmacs)"),
            "{card:?}"
        );
    }

    #[test]
    fn point_stays_on_its_card_when_the_window_narrows() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = answered(root.path(), "0.12.0");
        press(&mut editor, "C-n");

        editor.resize_pane(editor.selected_window(), 20, 30);

        let line = editor.buffer().line_of(editor.buffer().point());
        assert_eq!(lines(&editor)[line], "  R formatter");
    }

    #[test]
    fn keys_work_anywhere_on_a_card() {
        let root = tempfile::tempdir().unwrap();
        place(root.path(), "0.12.0");
        let mut editor = answered(root.path(), "0.12.0");

        press(&mut editor, "C-n C-n C-n u");

        assert_eq!(editor.message(), Some("air 0.12.0 is up to date"));
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
        assert!(
            matches!(&install.checksum, Expected::Download(asset) if asset.name == format!("{archive}.sha256")),
            "{:?}",
            install.checksum
        );
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

    const ENGINE: Managed = Managed {
        name: "engine",
        repository: "example/engine",
        summary: "Test engine",
        form: Form::Library,
        tag_prefix: "build/",
        archives: &[(env::consts::OS, env::consts::ARCH, "engine.tgz")],
        checksum: Checksum::Digest,
        setting: None,
    };

    #[test]
    fn applets_add_tools_that_install_by_their_published_digest() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        editor.add_managed_tool(ENGINE);
        let mut editor = {
            editor.tools_root = Some(root.path().to_owned());
            press(&mut editor, "M-x");
            type_text(&mut editor, "list-tools");
            press(&mut editor, "RET");
            editor
        };
        let (id, _) = checks(&mut editor)
            .into_iter()
            .find(|(_, url)| url.contains("example/engine"))
            .unwrap();

        editor.fetched(
            id,
            Ok(r#"{"tag_name": "build/42", "assets": [
                {"name": "engine.tgz", "browser_download_url": "https://example.com/engine.tgz", "size": 2097152, "digest": "sha256:ABCD"}
            ]}"#
            .to_owned()),
        );
        let shown = lines(&editor);
        let card = shown
            .iter()
            .position(|line| line == "engine  not installed")
            .unwrap();
        assert_eq!(shown[card + 3], "  Latest     42 (2.0 MB)");
        assert_eq!(shown[card + 4], "  Runs from  nothing yet");
        editor.go_to_line(card + 1, 1);
        press(&mut editor, "i");

        let install = installs(&mut editor).pop().unwrap();
        assert_eq!(install.tool, ENGINE);
        assert_eq!(install.archive.name, "engine.tgz");
        assert_eq!(install.checksum, Expected::Digest("abcd".to_owned()));
    }

    #[test]
    fn keys_off_a_tool_line_say_so() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = answered(root.path(), "0.12.0");

        press(&mut editor, "M-< i");

        assert_eq!(editor.message(), Some("No tool on this line"));
    }
}
