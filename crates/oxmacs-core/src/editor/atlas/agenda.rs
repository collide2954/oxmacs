use std::path::Path;

use jiff::civil::Date;

use crate::atlas::agenda::{self, Entry, Heading, Kind};
use crate::atlas::date;
use crate::atlas::edit;
use crate::atlas::note::{Priority, State};
use crate::editor::Editor;
use crate::editor::atlas::list::ListKind;
use crate::editor::atlas::{cycled_message, face};
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::editor::prompts::minibuffer::Purpose;
use crate::files;
use crate::theme::Highlight;

const HELP: &str = "RET visit · t cycle · s plan · d due · v vault · g refresh · q quit";
const DAYS: i64 = 7;
const LEAD: usize = 12;
const ROOM: usize = 80;
const WHEN: usize = 14;

fn color(name: Option<&str>, position: usize) -> Highlight {
    match name.map(str::to_lowercase).as_deref() {
        Some("red") => Highlight::Error,
        Some("orange" | "yellow") => Highlight::Warning,
        Some("green") => Highlight::Success,
        Some("blue") => Highlight::Link,
        Some("purple" | "magenta") => Highlight::Heading,
        Some("cyan" | "teal") => Highlight::Constant,
        Some("gray" | "grey") => Highlight::Comment,
        _ => Highlight::Nick(u8::try_from(position).unwrap_or(0)),
    }
}

fn heading(heading: Heading, today: Date) -> String {
    match heading {
        Heading::Overdue => "Overdue".to_owned(),
        Heading::Waiting => "Waiting".to_owned(),
        Heading::Day(day) if day == today => "Today".to_owned(),
        Heading::Day(day) => day.strftime("%A %-d %B").to_string(),
    }
}

fn when(entry: &Entry, section: Heading, today: Date) -> (String, Option<Highlight>) {
    let short = |day: Date| day.strftime("%b %-d").to_string();
    match &entry.kind {
        Kind::Event { .. } => (String::new(), None),
        Kind::Task { due, on, at, .. } => match (section, due) {
            (Heading::Overdue, Some(due)) if *due < today => {
                (format!("due {}", short(*due)), Some(Highlight::Error))
            }
            (Heading::Overdue, _) => (
                on.map(|on| format!("planned {}", short(on)))
                    .unwrap_or_default(),
                Some(Highlight::Warning),
            ),
            (Heading::Day(day), Some(due)) if *due == day => (
                at.map_or_else(
                    || "due".to_owned(),
                    |at| format!("due {}", at.strftime("%H:%M")),
                ),
                Some(Highlight::Warning),
            ),
            (Heading::Waiting, Some(due)) => (format!("due {}", short(*due)), None),
            _ => (
                at.map(|at| at.strftime("%H:%M").to_string())
                    .unwrap_or_default(),
                Some(Highlight::Comment),
            ),
        },
    }
}

fn lead(entry: &Entry) -> (String, Highlight) {
    match &entry.kind {
        Kind::Event { start, end } => {
            let time = |time: Option<jiff::civil::Time>| {
                time.map(|time| time.strftime("%H:%M").to_string())
            };
            let text = match (time(*start), time(*end)) {
                (Some(start), Some(end)) => format!("{start}-{end}"),
                (Some(start), None) => start,
                _ => "all day".to_owned(),
            };
            (text, Highlight::Attribute)
        }
        Kind::Task {
            state, priority, ..
        } => {
            let flag = if *priority == Some(Priority::High) {
                " !"
            } else {
                ""
            };
            (
                format!("[{}]{flag}", state.mark()),
                face(crate::atlas::note::Style::Box(*state)),
            )
        }
    }
}

fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return format!("{text:<width$}");
    }
    let mut clipped: String = text.chars().take(width.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

impl Editor {
    pub(in crate::editor) fn atlas_agenda(&mut self) {
        if self.config.atlas_vault.is_empty() {
            self.show_message("No Atlas vault yet; C-c n f offers to make one");
            return;
        }
        let filter = self
            .atlas_list_index(&ListKind::Agenda(None))
            .and_then(|index| self.atlas_list_kind(index))
            .and_then(|kind| match kind {
                ListKind::Agenda(filter) => filter,
                ListKind::Backlinks(_) => None,
            });
        self.show_atlas_list(&ListKind::Agenda(filter));
    }

    pub(in crate::editor) fn render_agenda(&mut self) {
        let Some(index) = self.atlas_list_index(&ListKind::Agenda(None)) else {
            return;
        };
        let Some(ListKind::Agenda(filter)) = self.atlas_list_kind(index) else {
            return;
        };
        let today = (self.today)();
        let sections = agenda::build(self.atlas_index(), today, DAYS, filter.as_deref());
        let room = self
            .buffers
            .get(index)
            .and_then(|buffer| self.width_showing(buffer.id()))
            .filter(|width| *width > 0)
            .unwrap_or(ROOM);
        let vaults: Vec<(String, Highlight)> = self
            .config
            .atlas_vault
            .iter()
            .enumerate()
            .map(|(position, vault)| (vault.name.clone(), color(vault.color.as_deref(), position)))
            .collect();
        let vault_width = vaults
            .iter()
            .map(|(name, _)| name.chars().count())
            .max()
            .unwrap_or(0);
        let title_width = room
            .saturating_sub(2 + LEAD + WHEN + 2 + vault_width)
            .max(12);
        let mut page = Page::default();
        let shown = filter.as_deref().unwrap_or("all");
        page.line(&[
            ("Agenda", Some(Highlight::Heading)),
            (&format!(" · {}", today.strftime("%A %-d %B")), None),
            (&format!("   vault: {shown}"), Some(Highlight::Comment)),
        ]);
        page.line(&[(HELP, Some(Highlight::Comment))]);
        let mut rows = Vec::new();
        for section in sections {
            page.line(&[]);
            page.heading(&heading(section.heading, today));
            if section.entries.is_empty() {
                page.line(&[("  Nothing planned", Some(Highlight::Comment))]);
            }
            for entry in &section.entries {
                let (lead, lead_face) = lead(entry);
                let (when, when_face) = when(entry, section.heading, today);
                let vault_face = vaults
                    .iter()
                    .find(|(name, _)| *name == entry.vault)
                    .map_or(Highlight::Comment, |(_, face)| *face);
                let waiting =
                    matches!(entry.kind, Kind::Task { state, .. } if state == State::Waiting);
                rows.push((page.styles.len(), entry.path.clone(), entry.line));
                page.line(&[
                    (&format!("  {lead:<LEAD$}"), Some(lead_face)),
                    (
                        &format!("{} ", clip(&entry.title, title_width)),
                        waiting.then_some(Highlight::Comment),
                    ),
                    (&format!("{when:<WHEN$} "), when_face),
                    (&format!("{:>vault_width$}", entry.vault), Some(vault_face)),
                ]);
            }
        }
        if let Some((buffer, Mode::AtlasList(list))) = self.buffers.entry_mut(index) {
            page.fill(buffer);
            list.rows = rows;
            list.styles = page.styles;
        }
    }

    pub(super) fn change_note_line(
        &mut self,
        path: &Path,
        line: usize,
        change: impl FnOnce(&str) -> Option<String>,
    ) -> Result<bool, String> {
        let open = self
            .buffers
            .position(|buffer| buffer.file().is_some_and(|file| file.path == path));
        if let Some(index) = open {
            let Some(buffer) = self.buffers.get(index) else {
                return Ok(false);
            };
            let Some(changed) = change(buffer.line_text(line).as_ref()) else {
                return Ok(false);
            };
            self.replace_line(index, line, &changed);
            return if self.save_buffer_at(index) {
                Ok(true)
            } else {
                Err(format!("Could not save {}", path.display()))
            };
        }
        let loaded = files::load(path).map_err(|error| error.to_string())?;
        let mut lines: Vec<String> = loaded.text.split('\n').map(str::to_owned).collect();
        let Some(old) = lines.get(line) else {
            return Ok(false);
        };
        let Some(changed) = change(old) else {
            return Ok(false);
        };
        lines[line] = changed;
        let text = lines.join("\n");
        files::save(path, &text, loaded.format).map_err(|error| error.to_string())?;
        self.note_atlas_saved(path, &text);
        Ok(true)
    }

    pub(in crate::editor) fn atlas_agenda_cycle(&mut self) {
        let Some((path, line)) = self.atlas_row() else {
            self.show_message("No task on this line");
            return;
        };
        let today = (self.today)();
        let mut message = None;
        let changed = self.change_note_line(&path, line, |text| {
            edit::cycle(text, today).map(|cycled| {
                message = cycled_message(&cycled);
                cycled.text
            })
        });
        self.after_note_change(changed, message);
    }

    fn after_note_change(&mut self, changed: Result<bool, String>, message: Option<String>) {
        match changed {
            Ok(true) => {
                self.refresh_atlas_list();
                if let Some(message) = message {
                    self.show_message(message);
                }
            }
            Ok(false) => self.show_message("No task on this line"),
            Err(error) => self.show_message(error),
        }
    }

    pub(in crate::editor) fn atlas_agenda_date(&mut self, due: bool) {
        if self.atlas_row().is_none() {
            self.show_message("No task on this line");
            return;
        }
        let prompt = if due { "Due: " } else { "Plan for: " };
        self.open_minibuffer(prompt, Purpose::AtlasDate(due));
    }

    pub(in crate::editor) fn atlas_date_entered(&mut self, due: bool, input: &str) {
        let today = (self.today)();
        let Some(day) = date::resolve(input, today) else {
            self.show_message(format!("Not a date: {}", input.trim()));
            return;
        };
        let Some((path, line)) = self.atlas_row() else {
            return;
        };
        let key = if due { "due" } else { "on" };
        let changed = self.change_note_line(&path, line, |text| {
            let is_task = edit::cycle(text, today).is_some();
            is_task.then(|| edit::set_field(text, key, day))
        });
        self.after_note_change(changed, Some(format!("{key}:{day}")));
    }

    pub(in crate::editor) fn atlas_agenda_vault(&mut self) {
        let Some(index) = self.atlas_list_index(&ListKind::Agenda(None)) else {
            return;
        };
        let Some(ListKind::Agenda(filter)) = self.atlas_list_kind(index) else {
            return;
        };
        let names: Vec<String> = self
            .config
            .atlas_vault
            .iter()
            .map(|vault| vault.name.clone())
            .collect();
        let next = match filter {
            None => names.first().cloned(),
            Some(name) => names
                .iter()
                .position(|known| *known == name)
                .and_then(|at| names.get(at + 1).cloned()),
        };
        if let Some(Mode::AtlasList(list)) = self.buffers.mode_mut(index) {
            list.kind = ListKind::Agenda(next);
        }
        self.render_agenda();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use jiff::civil::date;

    use crate::config::Config;
    use crate::editor::Editor;
    use crate::editor::test_support::{lines, press, type_text};

    fn agenda() -> (tempfile::TempDir, PathBuf, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let personal = dir.path().join("personal");
        let work = dir.path().join("work");
        fs::create_dir_all(&personal).unwrap();
        fs::create_dir_all(&work).unwrap();
        let garden = personal.join("Garden.atlas");
        fs::write(
            &garden,
            "[ ] Buy seeds  due:2026-10-19  !high\n[ ] Rake  on:2026-10-21\n@2026-10-19 10:00-11:00 Visit nursery\n",
        )
        .unwrap();
        fs::write(
            work.join("Office.atlas"),
            "[ ] Send invoice  due:2026-10-15\n",
        )
        .unwrap();
        let path = |dir: &PathBuf| dir.display().to_string().replace('\\', "/");
        let text = format!(
            "[[atlas-vault]]\nname = \"personal\"\npath = \"{}\"\n[[atlas-vault]]\nname = \"work\"\npath = \"{}\"\ncolor = \"red\"\n",
            path(&personal),
            path(&work)
        );
        let mut editor = Editor::new();
        editor.apply_config(&Config::parse(&text).unwrap());
        editor.today = || date(2026, 10, 19);
        press(&mut editor, "C-c n a");
        (dir, garden, editor)
    }

    fn row(editor: &mut Editor, text: &str) {
        let at = lines(editor)
            .iter()
            .position(|line| line.contains(text))
            .unwrap_or_else(|| panic!("no {text} in {:?}", lines(editor)));
        press(editor, "M-<");
        for _ in 0..at {
            press(editor, "C-n");
        }
    }

    #[test]
    fn c_c_n_a_shows_the_agenda_by_section() {
        let (_dir, _garden, editor) = agenda();

        assert_eq!(editor.buffer().name(), "*Agenda*");
        let shown = lines(&editor);
        assert!(
            shown[0].starts_with("Agenda · Monday 19 October"),
            "{shown:?}"
        );
        assert!(shown[0].ends_with("vault: all"));
        let at = |text: &str| shown.iter().position(|line| line.contains(text)).unwrap();
        assert!(at("Overdue") < at("Send invoice"));
        assert!(at("Send invoice") < at("Today"));
        assert!(at("Today") < at("Visit nursery"));
        assert!(at("Visit nursery") < at("Buy seeds"));
        assert!(at("Wednesday 21 October") < at("Rake"));
        assert!(shown[at("Send invoice")].contains("due Oct 15"));
        assert!(shown[at("Send invoice")].trim_end().ends_with("work"));
        assert!(shown[at("Visit nursery")].starts_with("  10:00-11:00"));
        assert!(shown[at("Buy seeds")].starts_with("  [ ] !"));
    }

    #[test]
    fn rows_fit_inside_the_window() {
        let (_dir, _garden, mut editor) = agenda();
        let window = editor.selected_window();

        editor.resize_pane(window, 30, 60);

        let widest = lines(&editor)
            .iter()
            .skip(2)
            .map(|line| line.chars().count())
            .max()
            .unwrap();
        assert_eq!(widest, 59);
    }

    #[test]
    fn t_cycles_the_task_in_its_note() {
        let (_dir, garden, mut editor) = agenda();
        row(&mut editor, "Buy seeds");

        press(&mut editor, "t");

        assert!(
            fs::read_to_string(&garden)
                .unwrap()
                .starts_with("[/] Buy seeds  due:2026-10-19  !high\n")
        );
        assert!(lines(&editor).iter().any(|line| line.contains("[/]")));
    }

    #[test]
    fn s_and_d_set_dates_from_loose_words() {
        let (_dir, garden, mut editor) = agenda();
        row(&mut editor, "Rake");

        press(&mut editor, "s");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Plan for: ");
        type_text(&mut editor, "fri");
        press(&mut editor, "RET");

        let text = fs::read_to_string(&garden).unwrap();
        assert!(text.contains("[ ] Rake  on:2026-10-23\n"), "{text}");
        assert!(
            lines(&editor)
                .iter()
                .any(|line| line.contains("Friday 23 October"))
        );

        row(&mut editor, "Rake");
        press(&mut editor, "d");
        type_text(&mut editor, "someday");
        press(&mut editor, "RET");
        assert_eq!(editor.message(), Some("Not a date: someday"));
    }

    #[test]
    fn edits_go_through_an_open_buffer_and_save_it() {
        let (_dir, garden, mut editor) = agenda();
        editor.open_file(&garden);
        press(&mut editor, "C-c n a");
        row(&mut editor, "Buy seeds");

        press(&mut editor, "t");

        let index = editor
            .buffers
            .position(|buffer| buffer.name() == "Garden.atlas")
            .unwrap();
        let buffer = editor.buffers.get(index).unwrap();
        assert!(buffer.contents().starts_with("[/] Buy seeds"));
        assert!(!buffer.is_modified());
    }

    #[test]
    fn v_cycles_the_vault_filter() {
        let (_dir, _garden, mut editor) = agenda();

        press(&mut editor, "v");
        assert!(lines(&editor)[0].ends_with("vault: personal"));
        assert!(
            !lines(&editor)
                .iter()
                .any(|line| line.contains("Send invoice"))
        );

        press(&mut editor, "v v");
        assert!(lines(&editor)[0].ends_with("vault: all"));
    }
}
