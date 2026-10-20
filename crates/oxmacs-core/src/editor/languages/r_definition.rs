use std::fs;
use std::path::{Path, PathBuf};

use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::languages::r_help::symbol_at;
use crate::external::r;
use crate::syntax::{Language, language_for};
use crate::text::{Buffer, BufferId};

const MARKERS: [&str; 2] = [".git", "DESCRIPTION"];
const SKIPPED: [&str; 3] = ["renv", "node_modules", "packrat"];
const MAX_FILES: usize = 2000;
const MAX_DEPTH: usize = 8;

fn assigns(line: &str, name: &str, equals: bool) -> bool {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix('`')
        .and_then(|rest| rest.strip_prefix(name))
        .and_then(|rest| rest.strip_prefix('`'))
        .or_else(|| {
            trimmed.strip_prefix(name).filter(|rest| {
                !rest.starts_with(|c: char| c.is_alphanumeric() || matches!(c, '.' | '_'))
            })
        });
    let Some(rest) = rest.map(str::trim_start) else {
        return false;
    };
    rest.starts_with("<-")
        || rest.starts_with("<<-")
        || (equals && rest.starts_with('=') && !rest.starts_with("=="))
}

fn definition_line(text: &str, name: &str) -> Option<usize> {
    let lines: Vec<&str> = text.lines().collect();
    lines
        .iter()
        .position(|line| !line.starts_with(char::is_whitespace) && assigns(line, name, true))
        .or_else(|| lines.iter().position(|line| assigns(line, name, false)))
}

fn project_root(directory: &Path) -> Option<PathBuf> {
    directory
        .ancestors()
        .find(|ancestor| {
            MARKERS.iter().any(|marker| ancestor.join(marker).exists())
                || fs::read_dir(ancestor).is_ok_and(|entries| {
                    entries.flatten().any(|entry| {
                        entry
                            .path()
                            .extension()
                            .is_some_and(|extension| extension == "Rproj")
                    })
                })
        })
        .map(Path::to_path_buf)
}

fn r_files(directory: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for path in entries {
        if found.len() >= MAX_FILES {
            return;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.is_dir() {
            if depth < MAX_DEPTH && !name.starts_with('.') && !SKIPPED.contains(&name.as_str()) {
                r_files(&path, depth + 1, found);
            }
        } else if language_for(&Language::BUILT_IN, &path) == Some(Language::R) {
            found.push(path);
        }
    }
}

impl Editor {
    pub(in crate::editor) fn r_find_definition(&mut self) {
        let buffer = self.buffers.current();
        let Some(name) = symbol_at(buffer, buffer.point()) else {
            self.show_message("No symbol at point");
            return;
        };
        let origin = self.current_location();
        if self.definition_in_buffers(&name, origin) || self.definition_in_project(&name, origin) {
            return;
        }
        if self.r_running() && self.r_waiting() {
            self.r_sessions.definition = Some((name.clone(), origin));
            self.r_request(&format!(
                "getOption(\"oxmacs\")$definition({})",
                r::string(&name)
            ));
            return;
        }
        self.show_message(format!(
            "No definition of {name} in open R buffers or the project"
        ));
    }

    fn definition_in_buffers(&mut self, name: &str, origin: (BufferId, usize)) -> bool {
        let found = (0..self.buffers.len()).find_map(|index| {
            if self.buffers.language(index) != Some(Language::R) {
                return None;
            }
            let buffer = self.buffers.get(index)?;
            let line = definition_line(&buffer.contents(), name)?;
            Some((buffer.id(), buffer.line_start(line)))
        });
        let Some((id, position)) = found else {
            return false;
        };
        self.remember_location(origin);
        if let Some(index) = self.buffers.position(|buffer| buffer.id() == id) {
            self.select_buffer(index);
        }
        self.buffers.current_mut().set_point(position);
        true
    }

    fn definition_in_project(&mut self, name: &str, origin: (BufferId, usize)) -> bool {
        let Some(root) = project_root(&self.default_directory()) else {
            return false;
        };
        let open: Vec<PathBuf> = self
            .buffers
            .iter()
            .filter_map(|buffer| buffer.file().map(|file| file.path.clone()))
            .collect();
        let mut files = Vec::new();
        r_files(&root, 0, &mut files);
        let found = files
            .into_iter()
            .filter(|path| !open.contains(path))
            .find_map(|path| {
                let text = fs::read_to_string(&path).ok()?;
                Some((path, definition_line(&text, name)?))
            });
        let Some((path, line)) = found else {
            return false;
        };
        self.visit_definition(&path, line + 1, origin)
    }

    fn visit_definition(&mut self, path: &Path, line: usize, origin: (BufferId, usize)) -> bool {
        if !self.open_file(path) {
            return false;
        }
        self.remember_location(origin);
        self.go_to_line(line, 1);
        true
    }

    pub(in crate::editor) fn r_definition_arrived(&mut self, payload: &str) {
        let Some((asked, origin)) = self.r_sessions.definition.take() else {
            return;
        };
        let mut fields = payload.splitn(3, '\t');
        let name = fields.next().unwrap_or_default();
        if name != asked {
            return;
        }
        match (fields.next(), fields.next()) {
            (Some("file"), Some(place)) => {
                let (path, line) = place.rsplit_once('\t').unwrap_or((place, "1"));
                let line = line.trim().parse().unwrap_or(1);
                if !self.visit_definition(Path::new(path), line, origin) {
                    self.show_message(format!("Cannot open {path}"));
                }
            }
            (Some("source"), Some(code)) => self.show_r_source(name, code, origin),
            _ => self.show_message(format!("No definition of {name} found")),
        }
    }

    fn show_r_source(&mut self, name: &str, code: &str, origin: (BufferId, usize)) {
        let title = format!("*R source: {name}*");
        self.remember_location(origin);
        if let Some(index) = self.buffers.position(|buffer| buffer.name() == title) {
            self.select_buffer(index);
        } else {
            self.add_buffer_with(Buffer::with_text(&title, ""), Mode::Text(None));
            self.set_major_mode(Some(Language::R));
        }
        let buffer = self.buffers.current_mut();
        buffer.set_read_only(false);
        buffer.set_text(code);
        buffer.mark_saved();
        buffer.set_read_only(true);
        buffer.set_point(0);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::definition_line;
    use crate::editor::test_support::{R_PROMPT, lines, press, r_running, r_script, r_sent};
    use crate::editor::{Editor, JobId};
    use crate::syntax::Language;

    #[test]
    fn assignments_at_the_top_level_come_first() {
        let text = "f <- function(x) {\n  helper <- 2\n}\nhelper <- function() 1\n";
        assert_eq!(definition_line(text, "helper"), Some(3));
        assert_eq!(definition_line("  helper <<- 3\n", "helper"), Some(0));
        assert_eq!(definition_line("x = 1\n", "x"), Some(0));
        assert_eq!(definition_line("`odd name` <- 1\n", "odd name"), Some(0));
    }

    #[test]
    fn named_arguments_and_comparisons_are_not_definitions() {
        assert_eq!(definition_line("plot(\n  data = x\n)\n", "data"), None);
        assert_eq!(definition_line("x == 1\n", "x"), None);
        assert_eq!(definition_line("xy <- 1\n", "x"), None);
        assert_eq!(definition_line("x$y <- 1\n", "x"), None);
    }

    #[test]
    fn m_dot_jumps_to_a_definition_in_the_buffer_and_m_comma_returns() {
        let mut editor = r_script("add <- function(a, b) a + b\nx <- add(1, 2)\n", 33);

        press(&mut editor, "M-.");
        assert_eq!(editor.buffer().point(), 0);

        press(&mut editor, "M-,");
        assert_eq!(editor.buffer().point(), 33);
    }

    #[test]
    fn definitions_are_found_in_the_projects_other_files() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".git")).unwrap();
        fs::create_dir_all(root.path().join("R")).unwrap();
        fs::write(
            root.path().join("R").join("utils.R"),
            "# helpers\n\nscale_to <- function(x, to) x / max(x) * to\n",
        )
        .unwrap();
        let script = root.path().join("analysis.R");
        fs::write(&script, "y <- scale_to(1:3, 10)\n").unwrap();
        let mut editor = Editor::new();
        editor.open_file(&script);
        press(&mut editor, "C-f C-f C-f C-f C-f C-f");

        press(&mut editor, "M-.");

        assert_eq!(editor.buffer().name(), "utils.R");
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 2);
        press(&mut editor, "M-,");
        assert_eq!(editor.buffer().name(), "analysis.R");
        assert_eq!(editor.buffer().point(), 6);
    }

    fn asking() -> (Editor, JobId) {
        let mut editor = r_script("fit <- lm(y ~ x)\n", 8);
        let job = r_running(&mut editor);
        press(&mut editor, "M-.");
        (editor, job)
    }

    fn definition(editor: &mut Editor, job: JobId, payload: &str) {
        editor.session_output(
            job,
            &format!("\u{1}definition\u{2}{payload}\u{3}{R_PROMPT}"),
        );
    }

    #[test]
    fn the_session_shows_the_source_of_other_functions() {
        let (mut editor, job) = asking();
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$definition(\"lm\")\n"]
        );

        definition(
            &mut editor,
            job,
            "lm\tsource\tfunction (formula, data)\n{\n    NULL\n}",
        );

        assert_eq!(editor.buffer().name(), "*R source: lm*");
        assert_eq!(lines(&editor)[0], "function (formula, data)");
        assert!(editor.buffer().is_read_only());
        assert_eq!(editor.buffers.language(0), Some(Language::R));
        press(&mut editor, "M-,");
        assert_eq!(editor.buffer().point(), 8);
    }

    #[test]
    fn the_session_can_point_at_a_sourced_file() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("model.R");
        fs::write(&file, "\n\nfit_model <- function(d) lm(y ~ x, d)\n").unwrap();
        let (mut editor, job) = asking();

        definition(
            &mut editor,
            job,
            &format!("lm\tfile\t{}\t3", file.display()),
        );

        assert_eq!(editor.buffer().name(), "model.R");
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 2);
    }

    #[test]
    fn unknown_names_say_so() {
        let (mut editor, job) = asking();

        definition(&mut editor, job, "lm\t");

        assert_eq!(editor.message(), Some("No definition of lm found"));

        let mut editor = r_script("zzz(1)\n", 0);
        press(&mut editor, "M-.");
        assert_eq!(
            editor.message(),
            Some("No definition of zzz in open R buffers or the project")
        );
    }
}
