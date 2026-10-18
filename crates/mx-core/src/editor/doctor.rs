use std::env;
use std::fmt::Write;
use std::path::Path;

use crate::config::Config;
use crate::display::LineStyle;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::syntax::{Feature, Language};
use crate::text::Buffer;
use crate::theme::{Appearance, Highlight};
use crate::tools::Probe;

const NAME: &str = "*mx-doctor*";
const LABEL_WIDTH: usize = 16;
const TOOLS: &[(&str, &str)] = &[
    ("rustfmt", "rustup component add rustfmt"),
    ("cargo", "install Rust from https://rustup.rs"),
    ("rust-analyzer", "rustup component add rust-analyzer"),
];

#[derive(Debug)]
pub(super) struct Doctor {
    styles: Vec<LineStyle>,
}

impl Doctor {
    pub(super) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }
}

#[derive(Default)]
struct Page {
    text: String,
    styles: Vec<LineStyle>,
}

impl Page {
    fn line(&mut self, parts: &[(&str, Option<Highlight>)]) {
        let mut style = LineStyle::default();
        let mut column = 0;
        for (part, highlight) in parts {
            let width = part.chars().count();
            if let Some(highlight) = highlight {
                style.spans.push((column..column + width, *highlight));
            }
            self.text.push_str(part);
            column += width;
        }
        self.text.push('\n');
        self.styles.push(style);
    }

    fn heading(&mut self, title: &str) {
        self.line(&[(title, Some(Highlight::Heading))]);
    }

    fn field(&mut self, label: &str, value: &str, highlight: Option<Highlight>) {
        let label = format!("  {label:<LABEL_WIDTH$}");
        self.line(&[(&label, None), (value, highlight)]);
    }
}

impl Editor {
    pub fn set_font_note(&mut self, note: impl Into<String>) {
        self.font_note = Some(note.into());
    }

    pub(super) fn mx_doctor(&mut self) {
        let page = self.examine();
        let doctor = Doctor {
            styles: page.styles,
        };
        let Some(index) = self.buffers.position(|buffer| buffer.name() == NAME) else {
            let mut buffer = Buffer::with_text(NAME, &page.text);
            buffer.set_read_only(true);
            self.add_buffer_with(buffer, Mode::Doctor(doctor));
            return;
        };
        self.select_buffer(index);
        let buffer = self.buffers.current_mut();
        let point = buffer.point();
        buffer.set_text(&page.text);
        buffer.set_point(point);
        self.buffers.set_mode(0, Mode::Doctor(doctor));
    }

    fn examine(&self) -> Page {
        let mut page = Page::default();
        page.line(&[
            ("mx doctor", Some(Highlight::Heading)),
            ("   g refreshes, q quits", None),
        ]);
        page.line(&[]);
        self.examine_environment(&mut page);
        page.line(&[]);
        examine_languages(&mut page);
        page.line(&[]);
        self.examine_tools(&mut page);
        page
    }

    fn examine_environment(&self, page: &mut Page) {
        page.heading("Environment");
        page.field(
            "Version",
            &format!("mx {}", env!("CARGO_PKG_VERSION")),
            None,
        );
        page.field(
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
        page.field("Theme", &theme, None);
        let font = self.font_note.as_deref().unwrap_or("unknown");
        page.field("Font", font, None);
        match &self.config_path {
            Some(path) => {
                let (status, highlight) = config_status(path);
                page.field("Config", &path.display().to_string(), None);
                page.field("", &status, Some(highlight));
            }
            None => page.field("Config", "no config directory", Some(Highlight::Warning)),
        }
        let data = dirs::data_dir().map(|dir| dir.join("mx").display().to_string());
        page.field("Data", data.as_deref().unwrap_or("none"), None);
    }

    fn examine_tools(&self, page: &mut Page) {
        page.heading("Tools");
        for (name, hint) in TOOLS {
            let label = format!("  {name:<LABEL_WIDTH$}");
            match (self.probe)(name) {
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
    }
}

fn examine_languages(page: &mut Page) {
    let names: Vec<String> = Language::ALL
        .iter()
        .map(|language| {
            let extensions: Vec<String> = language
                .extensions()
                .iter()
                .map(|extension| format!(".{extension}"))
                .collect();
            format!("{} ({})", language.name(), extensions.join(" "))
        })
        .collect();
    let name_width = names
        .iter()
        .map(|name| name.chars().count() + 2)
        .max()
        .unwrap_or(0)
        .max(LABEL_WIDTH);
    let mut header = format!("{:<width$}", "Languages", width = name_width + 2);
    for feature in Feature::ALL {
        let _ = write!(
            header,
            "{:<width$}",
            feature.label(),
            width = column_width(feature)
        );
    }
    page.line(&[(header.trim_end(), Some(Highlight::Heading))]);
    for (language, name) in Language::ALL.into_iter().zip(names) {
        let name = format!("  {name:<name_width$}");
        let mut parts: Vec<(String, Option<Highlight>)> = vec![(name, None)];
        for feature in Feature::ALL {
            let width = column_width(feature);
            parts.push(if language.supports(feature) {
                (format!("{:<width$}", "✓"), Some(Highlight::Success))
            } else {
                (format!("{:<width$}", "–"), None)
            });
        }
        let borrowed: Vec<(&str, Option<Highlight>)> = parts
            .iter()
            .map(|(text, highlight)| (text.as_str(), *highlight))
            .collect();
        page.line(&borrowed);
    }
}

fn column_width(feature: Feature) -> usize {
    feature.label().len() + 2
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

    use crate::editor::Editor;
    use crate::editor::test_support::{lines, press, type_text};
    use crate::theme::Highlight;
    use crate::tools::Probe;

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

    fn doctor() -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        editor.probe = fake_probe;
        editor.config_path = Some(dir.path().join("config.toml"));
        editor.set_font_note("JetBrains Mono (bundled)");
        press(&mut editor, "M-x");
        type_text(&mut editor, "mx-doctor");
        press(&mut editor, "RET");
        (dir, editor)
    }

    fn line_with(editor: &Editor, text: &str) -> String {
        lines(editor)
            .into_iter()
            .find(|line| line.contains(text))
            .unwrap_or_else(|| panic!("no line with {text}: {:?}", lines(editor)))
    }

    #[test]
    fn mx_doctor_opens_a_read_only_report() {
        let (_dir, editor) = doctor();

        assert_eq!(editor.buffer().name(), "*mx-doctor*");
        assert!(editor.buffer().is_read_only());
        assert!(line_with(&editor, "Version").contains(env!("CARGO_PKG_VERSION")));
        assert!(line_with(&editor, "Font").contains("JetBrains Mono (bundled)"));
        assert_eq!(editor.line_style(0).spans[0].1, Highlight::Heading);
    }

    #[test]
    fn languages_show_which_features_work() {
        let (_dir, editor) = doctor();

        let header = line_with(&editor, "Languages");
        let column = |label: &str| header.find(label).unwrap();
        let mark =
            |row: &str, label: &str| line_with(&editor, row).chars().nth(column(label)).unwrap();

        assert_eq!(mark("Rust (.rs)", "Highlight"), '✓');
        assert_eq!(mark("Rust (.rs)", "LSP"), '–');
        assert_eq!(mark("Markdown (", "Items"), '✓');
        assert_eq!(mark("Markdown (", "Comment"), '–');
    }

    #[test]
    fn tools_show_their_versions_or_how_to_get_them() {
        let (_dir, editor) = doctor();

        assert!(line_with(&editor, "rustfmt").contains("✓ rustfmt 1.0"));
        assert!(
            line_with(&editor, "  cargo")
                .contains("! no toolchain; install Rust from https://rustup.rs")
        );
        assert!(
            line_with(&editor, "rust-analyzer")
                .contains("✗ not found; rustup component add rust-analyzer")
        );
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
