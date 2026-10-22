use std::env;
use std::ops::Range;
use std::path::PathBuf;

use crate::display::LineStyle;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::editor::{Editor, Effect, FetchId};
use crate::external::managed::{self, Checksum, Expected, Install, Managed, Origin, Release};
use crate::files::paths;
use crate::text::Buffer;
use crate::theme::Highlight;

const NAME: &str = "*Tools*";
const HELP: &str = "i installs, u updates, d removes, TAB details, g checks again, q quits";
const NAME_WIDTH: usize = 8;
const STATE_WIDTH: usize = 20;
const DETAIL_WIDTH: usize = 11;
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
    open: Vec<Managed>,
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

fn latest_cell(latest: Option<&Latest>, tool: Managed, installed: Option<&str>) -> String {
    match latest {
        Some(Latest::Checking) | None => "checking…".to_owned(),
        Some(Latest::Failed) => "check failed".to_owned(),
        Some(Latest::Known(release)) => {
            let size = tool
                .archive_here()
                .and_then(|name| release.asset(&name))
                .map(|asset| format!(" ({})", megabytes(asset.size)))
                .unwrap_or_default();
            match installed {
                Some(have) if managed::newer(&release.version, have) => {
                    format!("{} available{size}", release.version)
                }
                Some(_) => format!("latest {}", release.version),
                None => format!("latest {}{size}", release.version),
            }
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

    fn tool_state(&self, tool: Managed) -> (&'static str, Highlight, String) {
        if let Some(version) = self.toolbox.installing(tool) {
            return ("!", Highlight::Warning, format!("installing {version}…"));
        }
        if let Some((have, _)) = self.installed_tool(tool) {
            let behind = matches!(
                self.toolbox.latest(tool),
                Some(Latest::Known(release)) if managed::newer(&release.version, &have)
            );
            let (mark, highlight) = if behind {
                ("!", Highlight::Warning)
            } else {
                ("✓", Highlight::Success)
            };
            return (mark, highlight, format!("{have} (oxmacs)"));
        }
        let search = env::var_os("PATH");
        match tool.locate(self.tool_setting(tool), None, search.as_deref()) {
            Some((_, Origin::Setting)) => (
                "✓",
                Highlight::Success,
                format!("found ({})", tool.setting.unwrap_or("a setting")),
            ),
            Some(_) => ("✓", Highlight::Success, "found (PATH)".to_owned()),
            None => ("✗", Highlight::Comment, "not installed".to_owned()),
        }
    }

    fn tool_details_lines(&self, page: &mut Page, tool: Managed) {
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
        let detail = |page: &mut Page, label: &str, value: &str| {
            page.line(&[
                (
                    &format!("    {label:<DETAIL_WIDTH$}"),
                    Some(Highlight::Comment),
                ),
                (value, None),
            ]);
        };
        page.line(&[(&format!("    {}", tool.summary), Some(Highlight::Comment))]);
        detail(page, "Runs from", &runs);
        detail(page, "Source", &format!("github.com/{}", tool.repository));
    }

    pub(in crate::editor) fn tool_details(&mut self) {
        let Some(tool) = self.tool_at_point() else {
            return;
        };
        if let Some(Mode::Tools(list)) = self.buffers.mode_mut(0) {
            if list.open.contains(&tool) {
                list.open.retain(|open| *open != tool);
            } else {
                list.open.push(tool);
            }
        }
        self.render_tools();
    }
    fn tools_page(&self, open: &[Managed]) -> (Page, Vec<(Range<usize>, Managed)>) {
        let tools = &self.toolbox.tools;
        let cells: Vec<_> = tools
            .iter()
            .map(|tool| {
                let installed = self.installed_tool(*tool).map(|(version, _)| version);
                let latest = latest_cell(self.toolbox.latest(*tool), *tool, installed.as_deref());
                (*tool, self.tool_state(*tool), latest)
            })
            .collect();
        let name_width = tools
            .iter()
            .map(|tool| tool.name.chars().count() + 2)
            .max()
            .unwrap_or(0)
            .max(NAME_WIDTH);
        let state_width = cells
            .iter()
            .map(|(_, (_, _, state), _)| state.chars().count() + 4)
            .max()
            .unwrap_or(0)
            .max(STATE_WIDTH)
            - 2;
        let mut groups: Vec<&str> = Vec::new();
        for tool in tools {
            if !groups.contains(&tool.group) {
                groups.push(tool.group);
            }
        }
        let mut page = Page::default();
        page.line(&[
            ("Tools", Some(Highlight::Heading)),
            ("   ", None),
            (HELP, Some(Highlight::Comment)),
        ]);
        page.line(&[]);
        let mut rows = Vec::new();
        for group in groups {
            page.heading(group);
            for (tool, (mark, highlight, state), latest) in
                cells.iter().filter(|(tool, ..)| tool.group == group)
            {
                let line = page.line_count();
                page.line(&[
                    (&format!("  {:<name_width$}", tool.name), None),
                    (mark, Some(*highlight)),
                    (&format!(" {state:<state_width$}"), None),
                    (latest, Some(Highlight::Comment)),
                ]);
                if open.contains(tool) {
                    self.tool_details_lines(&mut page, *tool);
                }
                rows.push((line..page.line_count(), *tool));
            }
        }
        (page, rows)
    }
    pub(in crate::editor) fn render_tools(&mut self) {
        let Some(index) = self.tools_index() else {
            return;
        };
        let open = match self.buffers.mode(index) {
            Some(Mode::Tools(list)) => list.open.clone(),
            _ => Vec::new(),
        };
        let (page, rows) = self.tools_page(&open);

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

    fn row(editor: &Editor, name: &str) -> String {
        let start = format!("  {name} ");
        lines(editor)
            .into_iter()
            .find(|line| line.starts_with(&start))
            .unwrap_or_default()
    }

    fn air_line(editor: &Editor) -> String {
        row(editor, "air")
    }

    #[test]
    fn list_tools_groups_air_under_r_and_checks_for_its_latest_release() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = opened(root.path());

        assert_eq!(editor.buffer().name(), "*Tools*");
        assert_eq!(
            lines(&editor)[..3],
            [
                "Tools   i installs, u updates, d removes, TAB details, g checks again, q quits",
                "",
                "R",
            ]
        );
        assert!(
            air_line(&editor).ends_with("   checking…"),
            "{}",
            air_line(&editor)
        );
        assert_eq!(
            checks(&mut editor)
                .into_iter()
                .map(|(_, url)| url)
                .collect::<Vec<_>>(),
            ["https://api.github.com/repos/posit-dev/air/releases/latest"]
        );
        let line = editor.buffer().line_of(editor.buffer().point());
        assert_eq!(lines(&editor)[line], air_line(&editor));
    }

    #[test]
    fn a_newer_release_offers_an_update() {
        let root = tempfile::tempdir().unwrap();
        place(root.path(), "0.11.0");

        let editor = answered(root.path(), "0.12.0");

        assert_eq!(
            air_line(&editor),
            "  air     ! 0.11.0 (oxmacs)   0.12.0 available (5.1 MB)"
        );
    }

    #[test]
    fn tools_that_are_up_to_date_show_their_version() {
        let root = tempfile::tempdir().unwrap();
        place(root.path(), "0.12.0");

        let editor = answered(root.path(), "0.12.0");

        assert_eq!(
            air_line(&editor),
            "  air     ✓ 0.12.0 (oxmacs)   latest 0.12.0"
        );
    }
    #[test]
    fn tab_shows_and_hides_the_details_of_a_tool() {
        let root = tempfile::tempdir().unwrap();
        place(root.path(), "0.12.0");
        let mut editor = answered(root.path(), "0.12.0");
        let before = lines(&editor);

        press(&mut editor, "TAB");

        let shown = lines(&editor);
        let at = shown
            .iter()
            .position(|line| line.starts_with("  air "))
            .unwrap();
        assert_eq!(shown[at + 1], "    R formatter");
        assert!(
            shown[at + 2].starts_with("    Runs from  ") && shown[at + 2].ends_with("(oxmacs)"),
            "{}",
            shown[at + 2]
        );
        assert_eq!(shown[at + 3], "    Source     github.com/posit-dev/air");
        press(&mut editor, "C-n C-n u");
        assert_eq!(editor.message(), Some("air 0.12.0 is up to date"));
        press(&mut editor, "TAB");
        assert_eq!(lines(&editor), before);
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
        assert!(air_line(&editor).contains("✓ 0.12.0 (oxmacs)"));
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
        assert!(air_line(&editor).ends_with("latest 0.12.0 (5.1 MB)"));
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
        assert!(air_line(&editor).ends_with("latest 0.12.0 (5.1 MB)"));
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
        group: "Engines",
        form: Form::Library,
        tag_prefix: "build/",
        archives: &[(env::consts::OS, env::consts::ARCH, "engine.tgz")],
        checksum: Checksum::Digest,
        setting: None,
    };

    #[test]
    fn plugins_add_tools_that_install_by_their_published_digest() {
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
        assert!(shown.contains(&"Engines".to_owned()), "{shown:?}");
        assert_eq!(
            row(&editor, "engine"),
            "  engine  ✗ not installed     latest 42 (2.0 MB)"
        );
        let line = shown
            .iter()
            .position(|line| line.starts_with("  engine "))
            .unwrap();
        editor.go_to_line(line + 1, 1);
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
