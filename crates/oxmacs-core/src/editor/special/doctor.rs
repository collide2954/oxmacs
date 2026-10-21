use std::env;
use std::path::Path;

use crate::applet::{DoctorLine, Finding};
use crate::config::Config;
use crate::display::LineStyle;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::editor::{Editor, Effect};
use crate::external::managed::AIR;
use crate::external::tools::{Probe, Tool};
use crate::text::Buffer;
use crate::theme::{Appearance, Highlight};

const NAME: &str = "*oxmacs-doctor*";
const LABEL_WIDTH: usize = 16;
const TOOLS: &[(&str, &str)] = &[
    ("rustfmt", "rustup component add rustfmt"),
    ("cargo", "install Rust from https://rustup.rs"),
    ("cargo-clippy", "rustup component add clippy"),
    ("rust-analyzer", "rustup component add rust-analyzer"),
];

const R_HINT: &str = "install R from https://cran.r-project.org or set r-program";

#[derive(Debug, Default)]
pub(in crate::editor) struct Doctor {
    styles: Vec<LineStyle>,
}

impl Doctor {
    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }
}

#[derive(Debug, Default)]
pub(in crate::editor) struct ToolReport {
    probes: Vec<(Tool, Probe)>,
    pending: usize,
}

impl ToolReport {
    fn find(&self, wanted: impl Fn(&Tool) -> bool) -> Option<&Probe> {
        self.probes
            .iter()
            .find(|(tool, _)| wanted(tool))
            .map(|(_, probe)| probe)
    }
}

fn field(page: &mut Page, label: &str, value: &str, highlight: Option<Highlight>) {
    let label = format!("  {label:<LABEL_WIDTH$}");
    page.line(&[(&label, None), (value, highlight)]);
}

impl Editor {
    pub fn set_font_note(&mut self, note: impl Into<String>) {
        self.font_note = Some(note.into());
    }

    pub(in crate::editor) fn oxmacs_doctor(&mut self) {
        self.check_tools();
        if let Some(index) = self.doctor_index() {
            self.select_buffer(index);
        } else {
            let mut buffer = Buffer::new(NAME);
            buffer.set_read_only(true);
            self.add_buffer_with(buffer, Mode::Doctor(Doctor::default()));
        }
        self.render_doctor();
    }

    pub fn tools_probed(&mut self, probes: Vec<(Tool, Probe)>) {
        self.tool_report.pending = self.tool_report.pending.saturating_sub(1);
        self.tool_report.probes = probes;
        self.render_doctor();
    }

    fn check_tools(&mut self) {
        let mut tools: Vec<Tool> = TOOLS.iter().map(|(name, _)| Tool::Program(name)).collect();
        tools.push(Tool::R(self.config.r_program.clone()));
        tools.push(Tool::Managed {
            tool: AIR,
            configured: self.config.air_program.clone(),
            root: self.tools_root.clone(),
        });
        self.tool_report.pending += 1;
        self.effects.push(Effect::ProbeTools(tools));
    }

    fn doctor_index(&self) -> Option<usize> {
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::Doctor(_)))
    }

    fn render_doctor(&mut self) {
        let Some(index) = self.doctor_index() else {
            return;
        };
        let page = self.examine();
        if let Some((buffer, Mode::Doctor(doctor))) = self.buffers.entry_mut(index) {
            page.fill(buffer);
            doctor.styles = page.styles;
        }
    }

    fn examine(&self) -> Page {
        let mut page = Page::default();
        page.line(&[
            ("oxmacs doctor", Some(Highlight::Heading)),
            ("   g refreshes, q quits", None),
        ]);
        page.line(&[]);
        self.examine_environment(&mut page);
        page.line(&[]);
        page.heading("Languages");
        field(
            &mut page,
            "Known",
            &format!(
                "{} languages; M-x list-languages shows what works in each",
                self.languages().len()
            ),
            None,
        );
        page.line(&[]);
        self.examine_tools(&mut page);
        for (title, lines) in &self.doctor_sections {
            page.line(&[]);
            page.heading(title);
            for line in lines(self) {
                finding(&mut page, &line);
            }
        }
        page
    }

    fn examine_environment(&self, page: &mut Page) {
        page.heading("Environment");
        field(
            page,
            "Version",
            &format!("oxmacs {}", env!("CARGO_PKG_VERSION")),
            None,
        );
        let applets = self
            .installed()
            .iter()
            .filter(|applet| applet.opt_in)
            .map(|applet| (applet.name, applet.version));
        let features: Vec<String> = applets
            .map(|(name, version)| format!("{name} {version}"))
            .collect();
        let features = if features.is_empty() {
            "none beyond the defaults".to_owned()
        } else {
            features.join(", ")
        };
        field(page, "Features", &features, None);
        field(
            page,
            "System",
            &format!("{} {}", env::consts::OS, env::consts::ARCH),
            None,
        );
        let appearance = match self.appearance {
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        };
        let theme = match self.configured_theme {
            Some(theme) => theme.name.to_owned(),
            None => format!(
                "{}, following the system's {appearance} mode",
                self.theme().name
            ),
        };
        field(page, "Theme", &theme, None);
        let font = self.font_note.as_deref().unwrap_or("unknown");
        field(page, "Font", font, None);
        match &self.config_path {
            Some(path) => {
                let (status, highlight) = config_status(path);
                field(page, "Config", &path.display().to_string(), None);
                field(page, "", &status, Some(highlight));
            }
            None => field(
                page,
                "Config",
                "no config directory",
                Some(Highlight::Warning),
            ),
        }
        for table in self.unclaimed_config_tables() {
            field(
                page,
                "",
                &format!("[{table}] is not read by any applet in this build"),
                Some(Highlight::Warning),
            );
        }
        let data = dirs::data_dir().map(|dir| dir.join("oxmacs").display().to_string());
        field(page, "Data", data.as_deref().unwrap_or("none"), None);
    }

    fn examine_tools(&self, page: &mut Page) {
        let report = &self.tool_report;
        if report.pending > 0 && !report.probes.is_empty() {
            page.line(&[
                ("Tools", Some(Highlight::Heading)),
                ("   checking again…", Some(Highlight::Comment)),
            ]);
        } else {
            page.heading("Tools");
        }
        for (name, hint) in TOOLS {
            tool(
                page,
                name,
                report.find(|tool| *tool == Tool::Program(name)),
                hint,
            );
        }
        tool(
            page,
            "R",
            report.find(|tool| matches!(tool, Tool::R(_))),
            R_HINT,
        );
        tool(
            page,
            AIR.name,
            report.find(|tool| matches!(tool, Tool::Managed { tool, .. } if *tool == AIR)),
            "M-x list-tools installs it, or set air-program",
        );
    }
}

fn finding(page: &mut Page, line: &DoctorLine) {
    let label = format!("  {:<LABEL_WIDTH$}", line.label);
    let mark = match line.finding {
        Finding::Info => None,
        Finding::Good => Some(("✓ ", Highlight::Success)),
        Finding::Warning => Some(("! ", Highlight::Warning)),
        Finding::Problem => Some(("✗ ", Highlight::Error)),
    };
    match mark {
        Some((mark, highlight)) => {
            page.line(&[(&label, None), (mark, Some(highlight)), (&line.text, None)]);
        }
        None => page.line(&[(&label, None), (&line.text, None)]),
    }
}

fn tool(page: &mut Page, name: &str, probe: Option<&Probe>, hint: &str) {
    let label = format!("  {name:<LABEL_WIDTH$}");
    let Some(probe) = probe else {
        page.line(&[(&label, None), ("checking…", Some(Highlight::Comment))]);
        return;
    };
    match probe {
        Probe::Found { path, version } => page.line(&[
            (&label, None),
            ("✓ ", Some(Highlight::Success)),
            (&format!("{version}  {}", path.display()), None),
        ]),
        Probe::Broken { error, .. } => page.line(&[
            (&label, None),
            ("! ", Some(Highlight::Warning)),
            (&format!("{}; {hint}", error.trim_end_matches('.')), None),
        ]),
        Probe::Missing => page.line(&[
            (&label, None),
            ("✗ ", Some(Highlight::Error)),
            (&format!("not found; {hint}"), None),
        ]),
    }
}

fn config_status(path: &Path) -> (String, Highlight) {
    if !path.exists() {
        return ("not found; using defaults".to_owned(), Highlight::Success);
    }
    match Config::load(path) {
        Ok(_) => ("loaded".to_owned(), Highlight::Success),
        Err(error) => (format!("error: {error}"), Highlight::Error),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::applet::{DoctorLine, Finding};
    use crate::config::Config;
    use crate::editor::test_support::{lines, press, type_text};
    use crate::editor::{Editor, Effect};
    use crate::external::managed::AIR;
    use crate::external::tools::{Probe, Tool};
    use crate::theme::Highlight;

    fn fake_probe(program: &str) -> Probe {
        match program {
            "rustfmt" => Probe::Found {
                path: PathBuf::from("/bin/rustfmt"),
                version: "rustfmt 1.0".to_owned(),
            },
            "cargo" => Probe::Broken {
                path: PathBuf::from("/bin/cargo"),
                error: "no toolchain".to_owned(),
            },
            _ => Probe::Missing,
        }
    }

    fn opened() -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        editor.config_path = Some(dir.path().join("config.toml"));
        editor.tools_root = Some(dir.path().join("tools"));
        editor.set_font_note("JetBrains Mono (bundled)");
        press(&mut editor, "M-x");
        type_text(&mut editor, "oxmacs-doctor");
        press(&mut editor, "RET");
        (dir, editor)
    }

    fn asked(editor: &mut Editor) -> Vec<Tool> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::ProbeTools(tools) => Some(tools),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn answer(editor: &mut Editor) {
        let probes = asked(editor)
            .into_iter()
            .map(|tool| {
                let probe = match &tool {
                    Tool::Program(name) => fake_probe(name),
                    Tool::R(_) => Probe::Missing,
                    Tool::Managed { .. } => Probe::Found {
                        path: PathBuf::from("/oxmacs/air"),
                        version: "air 0.12.0".to_owned(),
                    },
                };
                (tool, probe)
            })
            .collect();
        editor.tools_probed(probes);
    }

    fn doctor() -> (tempfile::TempDir, Editor) {
        let (dir, mut editor) = opened();
        answer(&mut editor);
        (dir, editor)
    }

    #[test]
    fn the_report_opens_at_once_and_checks_tools_in_the_background() {
        let (dir, mut editor) = opened();

        assert!(line_with(&editor, "rustfmt").ends_with("checking…"));
        assert!(line_with(&editor, "  R  ").ends_with("checking…"));
        assert!(line_with(&editor, "  air  ").ends_with("checking…"));
        assert_eq!(
            asked(&mut editor),
            [
                Tool::Program("rustfmt"),
                Tool::Program("cargo"),
                Tool::Program("cargo-clippy"),
                Tool::Program("rust-analyzer"),
                Tool::R(None),
                Tool::Managed {
                    tool: AIR,
                    configured: None,
                    root: Some(dir.path().join("tools")),
                },
            ]
        );
    }

    #[test]
    fn refreshing_keeps_the_last_results_while_checking_again() {
        let (_dir, mut editor) = doctor();
        assert_eq!(line_with(&editor, "Tools").trim_end(), "Tools");

        press(&mut editor, "g");

        assert!(line_with(&editor, "Tools").ends_with("checking again…"));
        assert!(line_with(&editor, "rustfmt").contains("✓ rustfmt 1.0"));
        answer(&mut editor);
        assert_eq!(line_with(&editor, "Tools").trim_end(), "Tools");
    }

    #[test]
    fn results_are_kept_for_the_next_report() {
        let (_dir, mut editor) = doctor();
        press(&mut editor, "C-x k RET");

        press(&mut editor, "M-x");
        type_text(&mut editor, "oxmacs-doctor");
        press(&mut editor, "RET");

        assert!(line_with(&editor, "rustfmt").contains("✓ rustfmt 1.0"));
        assert_eq!(asked(&mut editor).len(), 6);
    }

    #[test]
    fn results_for_a_closed_report_change_nothing_else() {
        let (_dir, mut editor) = opened();
        press(&mut editor, "C-x k RET");

        answer(&mut editor);

        assert_eq!(editor.buffer().name(), "*scratch*");
    }

    fn line_with(editor: &Editor, text: &str) -> String {
        lines(editor)
            .into_iter()
            .find(|line| line.contains(text))
            .unwrap_or_else(|| panic!("no line with {text}: {:?}", lines(editor)))
    }

    #[test]
    fn oxmacs_doctor_opens_a_read_only_report() {
        let (_dir, editor) = doctor();

        assert_eq!(editor.buffer().name(), "*oxmacs-doctor*");
        assert!(editor.buffer().is_read_only());
        assert!(line_with(&editor, "Version").contains(env!("CARGO_PKG_VERSION")));
        assert!(line_with(&editor, "Font").contains("JetBrains Mono (bundled)"));
        assert_eq!(editor.line_style(0).spans[0].1, Highlight::Heading);
    }

    #[test]
    fn languages_are_counted_with_a_pointer_to_their_list() {
        let (_dir, editor) = doctor();

        assert!(
            line_with(&editor, "Known")
                .ends_with("languages; M-x list-languages shows what works in each"),
            "{}",
            line_with(&editor, "Known")
        );
    }

    #[test]
    fn tools_show_their_versions_or_how_to_get_them() {
        let (_dir, editor) = doctor();

        assert!(line_with(&editor, "rustfmt").contains("✓ rustfmt 1.0"));
        assert!(line_with(&editor, "  air  ").contains("✓ air 0.12.0"));
        assert!(
            line_with(&editor, "  cargo")
                .contains("! no toolchain; install Rust from https://rustup.rs")
        );
        assert!(
            line_with(&editor, "  R  ").contains(
                "✗ not found; install R from https://cran.r-project.org or set r-program"
            )
        );
        assert!(
            line_with(&editor, "cargo-clippy").contains("✗ not found; rustup component add clippy")
        );
        assert!(
            line_with(&editor, "rust-analyzer")
                .contains("✗ not found; rustup component add rust-analyzer")
        );
    }

    struct Sample {
        opt_in: bool,
    }

    impl crate::applet::Applet for Sample {
        fn name(&self) -> &'static str {
            if self.opt_in { "sample" } else { "built-in" }
        }

        fn version(&self) -> &'static str {
            "0.4.0"
        }

        fn opt_in(&self) -> bool {
            self.opt_in
        }

        fn install(&self, _editor: &mut Editor) {}
    }

    #[test]
    fn installed_opt_in_applets_are_listed_with_their_versions() {
        let (_dir, mut editor) = doctor();
        editor.install(&Sample { opt_in: true });
        editor.install(&Sample { opt_in: false });

        press(&mut editor, "g");

        let line = line_with(&editor, "Features");
        assert!(line.contains("sample 0.4.0"), "{line}");
        assert!(!line.contains("built-in"), "{line}");
    }

    fn sample_findings(editor: &Editor) -> Vec<DoctorLine> {
        vec![
            DoctorLine {
                label: "buffers".to_owned(),
                finding: Finding::Info,
                text: format!("{} open", editor.buffer_names().len()),
            },
            DoctorLine {
                label: "server".to_owned(),
                finding: Finding::Good,
                text: "reachable".to_owned(),
            },
            DoctorLine {
                label: "token".to_owned(),
                finding: Finding::Problem,
                text: "missing".to_owned(),
            },
        ]
    }

    #[test]
    fn applets_add_their_own_sections() {
        let (_dir, mut editor) = doctor();
        editor.add_doctor_section("Sample", sample_findings);

        press(&mut editor, "g");

        let lines = lines(&editor);
        let heading = lines.iter().position(|line| line == "Sample").unwrap();
        assert!(
            lines[heading + 1].starts_with("  buffers"),
            "{}",
            lines[heading + 1]
        );
        assert!(lines[heading + 1].ends_with(" open"));
        assert!(lines[heading + 2].ends_with("✓ reachable"));
        assert!(lines[heading + 3].ends_with("✗ missing"));
    }

    #[test]
    fn config_tables_no_applet_reads_are_pointed_out() {
        let (_dir, mut editor) = doctor();
        editor.apply_config(&Config::parse("[news]\nfeeds = 3\n").unwrap());

        press(&mut editor, "g");

        assert!(
            line_with(&editor, "[news]")
                .ends_with("[news] is not read by any applet in this build")
        );
    }

    #[test]
    fn a_build_without_opt_in_applets_says_so() {
        let (_dir, editor) = doctor();

        let line = line_with(&editor, "Features");
        assert!(line.ends_with("none beyond the defaults"), "{line}");
    }

    #[test]
    fn the_config_file_is_checked_on_refresh() {
        let (dir, mut editor) = doctor();
        assert!(
            lines(&editor)
                .iter()
                .any(|line| line.contains("not found; using defaults"))
        );

        fs::write(dir.path().join("config.toml"), "tab-width = 0").unwrap();
        press(&mut editor, "g");
        assert!(line_with(&editor, "error:").contains("tab-width must be at least 1"));

        fs::write(dir.path().join("config.toml"), "tab-width = 4").unwrap();
        press(&mut editor, "g");
        assert!(lines(&editor).iter().any(|line| line.trim() == "loaded"));
    }

    #[test]
    fn q_buries_the_report() {
        let (_dir, mut editor) = doctor();

        press(&mut editor, "q");

        assert_eq!(editor.buffer().name(), "*scratch*");
    }
}
