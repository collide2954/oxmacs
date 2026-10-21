use std::ops::Range;
use std::path::Path;

use oxmacs_core::display::LineStyle;
use oxmacs_core::files::paths;
use oxmacs_core::theme::Highlight;

use crate::uv::facts::{Facts, Lock, Probe};

const LABEL: usize = 10;
const HELP: &str = "g refresh · q quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    Info,
    Blank,
    Heading,
    Required(usize),
    Locked(usize),
    Newer(usize),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Rendered {
    pub(crate) text: String,
    pub(crate) rows: Vec<Row>,
    pub(crate) styles: Vec<LineStyle>,
    pub(crate) pills: Vec<Vec<(Range<usize>, Highlight)>>,
}

#[derive(Debug, Default)]
struct Line {
    text: String,
    width: usize,
    style: LineStyle,
    pills: Vec<(Range<usize>, Highlight)>,
}

impl Line {
    fn push(&mut self, text: &str, face: Option<Highlight>) -> Range<usize> {
        let start = self.width;
        self.text.push_str(text);
        self.width += text.chars().count();
        let range = start..self.width;
        if let Some(face) = face
            && !range.is_empty()
        {
            self.style.spans.push((range.clone(), face));
        }
        range
    }

    fn pill(&mut self, text: &str, face: Highlight) {
        let range = self.push(text, Some(face));
        self.pills.push((range, face));
    }

    fn pad(&mut self, column: usize) {
        let spaces = column.saturating_sub(self.width);
        self.push(&" ".repeat(spaces), None);
    }
}

impl Rendered {
    fn add(&mut self, row: Row, line: Line) {
        if !self.rows.is_empty() {
            self.text.push('\n');
        }
        self.text.push_str(&line.text);
        self.rows.push(row);
        self.styles.push(line.style);
        self.pills.push(line.pills);
    }

    fn heading(&mut self, title: &str, count: Option<usize>) {
        self.add(Row::Blank, Line::default());
        let mut line = Line::default();
        line.push(title, Some(Highlight::Heading));
        if let Some(count) = count {
            line.push(&format!(" ({count})"), Some(Highlight::Comment));
        }
        self.add(Row::Heading, line);
    }

    fn note(&mut self, text: &str) {
        let mut line = Line::default();
        line.push(&format!("  {text}"), Some(Highlight::Comment));
        self.add(Row::Info, line);
    }
}

fn field(label: &str) -> Line {
    let mut line = Line::default();
    line.push(&format!("{label:<LABEL$}"), Some(Highlight::Comment));
    line
}

fn header(out: &mut Rendered, facts: &Facts, home: Option<&Path>) {
    let mut line = field("Project:");
    match &facts.project.name {
        Some(name) => line.pill(name, Highlight::Link),
        None => {
            line.push("unnamed", Some(Highlight::Comment));
        }
    }
    out.add(Row::Info, line);
    let mut line = field("Python:");
    match &facts.python {
        Some(version) => line.push(version, Some(Highlight::Constant)),
        None => line.push("no .venv", Some(Highlight::Comment)),
    };
    out.add(Row::Info, line);
    let mut line = field("Venv:");
    match &facts.venv {
        Some(venv) => line.push(&paths::abbreviate(venv, home), None),
        None => line.push("none", Some(Highlight::Comment)),
    };
    out.add(Row::Info, line);
    let mut line = field("Lock:");
    match &facts.lock {
        Lock::Fresh => line.pill("up to date", Highlight::Success),
        Lock::Stale => line.pill("out of date", Highlight::Warning),
        Lock::Missing => {
            line.push("no uv.lock", Some(Highlight::Comment));
        }
        Lock::Checking => {
            line.push("checking…", Some(Highlight::Comment));
        }
        Lock::Failed(error) => {
            line.push(error, Some(Highlight::Error));
        }
    }
    out.add(Row::Info, line);
    let mut line = Line::default();
    line.push(HELP, Some(Highlight::Comment));
    out.add(Row::Info, line);
}

fn widest<'a>(names: impl Iterator<Item = &'a String>) -> usize {
    names.map(|name| name.chars().count()).max().unwrap_or(0)
}

fn dependencies(out: &mut Rendered, facts: &Facts) {
    let required = &facts.project.required;
    out.heading("Required", Some(required.len()));
    if required.is_empty() {
        out.note("none");
    }
    for (index, requirement) in required.iter().enumerate() {
        let mut line = Line::default();
        line.push("  ", None);
        line.push(requirement, None);
        out.add(Row::Required(index), line);
    }
    out.heading("Locked", Some(facts.locked.len()));
    if facts.locked.is_empty() {
        out.note("none");
    }
    let width = widest(facts.locked.iter().map(|(name, _)| name));
    for (index, (name, version)) in facts.locked.iter().enumerate() {
        let mut line = Line::default();
        line.push("  ", None);
        line.push(name, None);
        line.pad(2 + width + 2);
        line.push(version, Some(Highlight::Constant));
        out.add(Row::Locked(index), line);
    }
}

fn newer(out: &mut Rendered, facts: &Facts) {
    let newer = match &facts.newer {
        Probe::Checking => {
            out.heading("Newer", None);
            out.note("checking…");
            return;
        }
        Probe::Failed(error) => {
            out.heading("Newer", None);
            out.note(error);
            return;
        }
        Probe::Known(newer) => newer,
    };
    out.heading("Newer", Some(newer.len()));
    if newer.is_empty() {
        out.note("everything is up to date");
    }
    let width = widest(newer.iter().map(|package| &package.name));
    for (index, package) in newer.iter().enumerate() {
        let mut line = Line::default();
        line.push("  ", None);
        line.push(&package.name, None);
        line.pad(2 + width + 2);
        line.push(&package.version, Some(Highlight::Comment));
        line.push(" → ", Some(Highlight::Comment));
        line.pill(&package.latest, Highlight::Success);
        out.add(Row::Newer(index), line);
    }
}

pub(crate) fn render(facts: &Facts, home: Option<&Path>) -> Rendered {
    let mut out = Rendered::default();
    header(&mut out, facts, home);
    dependencies(&mut out, facts);
    newer(&mut out, facts);
    out
}

#[cfg(test)]
mod tests {
    use std::path::{MAIN_SEPARATOR as SEPARATOR, Path, PathBuf};

    use oxmacs_core::theme::Highlight;

    use super::*;
    use crate::uv::facts::{Lock, Newer, Probe, Project};

    fn sample() -> Facts {
        Facts {
            project: Project {
                name: Some("demo".to_owned()),
                required: vec!["requests>=2.31".to_owned(), "Numpy".to_owned()],
            },
            locked: vec![
                ("certifi".to_owned(), "2024.8.30".to_owned()),
                ("numpy".to_owned(), "2.1.0".to_owned()),
            ],
            python: Some("3.12.4".to_owned()),
            venv: Some(PathBuf::from("/home/ada").join("demo").join(".venv")),
            lock: Lock::Fresh,
            newer: Probe::Known(vec![Newer {
                name: "requests".to_owned(),
                version: "2.32.3".to_owned(),
                latest: "2.33.0".to_owned(),
            }]),
        }
    }

    fn lines(rendered: &Rendered) -> Vec<&str> {
        rendered.text.split('\n').collect()
    }

    #[test]
    fn the_dashboard_shows_the_project_then_its_dependencies() {
        let rendered = render(&sample(), Some(Path::new("/home/ada")));
        let venv = format!("Venv:     ~{SEPARATOR}demo{SEPARATOR}.venv");

        assert_eq!(
            lines(&rendered),
            [
                "Project:  demo",
                "Python:   3.12.4",
                &venv,
                "Lock:     up to date",
                "g refresh · q quit",
                "",
                "Required (2)",
                "  requests>=2.31",
                "  Numpy",
                "",
                "Locked (2)",
                "  certifi  2024.8.30",
                "  numpy    2.1.0",
                "",
                "Newer (1)",
                "  requests  2.32.3 → 2.33.0",
            ]
        );
        assert_eq!(rendered.rows.len(), rendered.styles.len());
        assert_eq!(rendered.rows[7], Row::Required(0));
        assert_eq!(rendered.rows[12], Row::Locked(1));
        assert_eq!(rendered.rows[15], Row::Newer(0));
    }

    #[test]
    fn names_states_and_new_versions_are_pills() {
        let rendered = render(&sample(), None);

        assert_eq!(rendered.pills[0], [(10..14, Highlight::Link)]);
        assert_eq!(rendered.pills[3], [(10..20, Highlight::Success)]);
        assert_eq!(rendered.pills[15], [(21..27, Highlight::Success)]);
        assert!(
            rendered.styles[11]
                .spans
                .contains(&(11..20, Highlight::Constant))
        );
        assert!(
            rendered.styles[6]
                .spans
                .contains(&(0..8, Highlight::Heading))
        );
    }

    #[test]
    fn missing_and_pending_facts_say_so() {
        let facts = Facts {
            project: Project::default(),
            lock: Lock::Stale,
            newer: Probe::Checking,
            ..Facts::default()
        };

        let rendered = render(&facts, None);

        assert_eq!(
            lines(&rendered),
            [
                "Project:  unnamed",
                "Python:   no .venv",
                "Venv:     none",
                "Lock:     out of date",
                "g refresh · q quit",
                "",
                "Required (0)",
                "  none",
                "",
                "Locked (0)",
                "  none",
                "",
                "Newer",
                "  checking…",
            ]
        );
        assert_eq!(rendered.pills[3], [(10..21, Highlight::Warning)]);
    }

    #[test]
    fn checks_that_could_not_run_say_why() {
        let facts = Facts {
            lock: Lock::Failed("uv is not installed".to_owned()),
            newer: Probe::Failed("no .venv".to_owned()),
            ..Facts::default()
        };

        let rendered = render(&facts, None);

        let shown = lines(&rendered);
        assert_eq!(shown[3], "Lock:     uv is not installed");
        assert_eq!(shown.last(), Some(&"  no .venv"));
        assert!(
            rendered.styles[3]
                .spans
                .contains(&(10..29, Highlight::Error))
        );
    }
}
