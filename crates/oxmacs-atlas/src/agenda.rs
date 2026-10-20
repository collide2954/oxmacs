use std::path::Path;

use jiff::civil::Date;
use oxmacs_core::editor::{Page, clip};
use oxmacs_core::files;
use oxmacs_core::theme::Highlight;

use crate::lists::ListKind;
use crate::model::agenda::{self, Entry, Heading, Kind};
use crate::model::date;
use crate::model::edit;
use crate::model::note::{Priority, State, Style};
use crate::notes::{Notebook, cycled_message, face, notebook, settings};

const HELP: &str = "RET visit · t cycle · s plan · d due · v vault · g refresh · q quit";
const DAYS: i64 = 7;
const LEAD: usize = 12;
const ROOM: usize = 80;
const WHEN: usize = 14;

pub(crate) fn color(name: Option<&str>, position: usize) -> Highlight {
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

pub(crate) fn lead(entry: &Entry) -> (String, Highlight) {
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
                face(Style::Box(*state)),
            )
        }
    }
}

pub(crate) fn fit(text: &str, width: usize) -> String {
    format!("{:<width$}", clip(text, width))
}

pub(crate) fn vault_colors(book: &Notebook<'_>) -> Vec<(String, Highlight)> {
    settings(book)
        .vault
        .iter()
        .enumerate()
        .map(|(position, vault)| (vault.name.clone(), color(vault.color.as_deref(), position)))
        .collect()
}

impl Notebook<'_> {
    pub(crate) fn agenda(&mut self) {
        if self.vaults().is_empty() {
            self.show_message("No Atlas vault yet; C-c n f offers to make one");
            return;
        }
        let filter = self
            .list_id(&ListKind::Agenda(None))
            .and_then(|id| self.list_kind(id))
            .and_then(|kind| match kind {
                ListKind::Agenda(filter) => filter,
                ListKind::Backlinks(_) | ListKind::Calendar(_) => None,
            });
        self.show_list(&ListKind::Agenda(filter));
    }

    pub(crate) fn render_agenda(&mut self) {
        let Some(id) = self.list_id(&ListKind::Agenda(None)) else {
            return;
        };
        let Some(ListKind::Agenda(filter)) = self.list_kind(id) else {
            return;
        };
        let today = self.today();
        let sections = agenda::build(self.index(), today, DAYS, filter.as_deref());
        let room = self.buffer_width(id).unwrap_or(ROOM);
        let vaults = vault_colors(self);
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
                rows.push((page.line_count(), entry.path.clone(), entry.line));
                page.line(&[
                    (&format!("  {lead:<LEAD$}"), Some(lead_face)),
                    (
                        &format!("{} ", fit(&entry.title, title_width)),
                        waiting.then_some(Highlight::Comment),
                    ),
                    (&format!("{when:<WHEN$} "), when_face),
                    (&format!("{:>vault_width$}", entry.vault), Some(vault_face)),
                ]);
            }
        }
        self.fill_list(id, page, rows);
    }

    fn change_note_line(
        &mut self,
        path: &Path,
        line: usize,
        change: impl FnOnce(&str) -> Option<String>,
    ) -> Result<bool, String> {
        if let Some(id) = self.buffer_visiting(path) {
            let Some(buffer) = self.buffer_by_id(id) else {
                return Ok(false);
            };
            let Some(changed) = change(buffer.line_text(line).as_ref()) else {
                return Ok(false);
            };
            self.replace_line(id, line, &changed);
            return if self.save(id) {
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
        self.note_saved(path, &text);
        Ok(true)
    }

    pub(crate) fn agenda_cycle(&mut self) {
        let Some((path, line)) = self.row() else {
            self.show_message("No task on this line");
            return;
        };
        let today = self.today();
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
                self.refresh_list();
                if let Some(message) = message {
                    self.show_message(message);
                }
            }
            Ok(false) => self.show_message("No task on this line"),
            Err(error) => self.show_message(error),
        }
    }

    pub(crate) fn agenda_date(&mut self, due: bool) {
        if self.row().is_none() {
            self.show_message("No task on this line");
            return;
        }
        let prompt = if due { "Due: " } else { "Plan for: " };
        self.prompt(prompt, move |editor, input| {
            notebook(editor, |book| book.date_entered(due, &input));
        });
    }

    fn date_entered(&mut self, due: bool, input: &str) {
        let today = self.today();
        let Some(day) = date::resolve(input, today) else {
            self.show_message(format!("Not a date: {}", input.trim()));
            return;
        };
        let Some((path, line)) = self.row() else {
            return;
        };
        let key = if due { "due" } else { "on" };
        let changed = self.change_note_line(&path, line, |text| {
            let is_task = edit::cycle(text, today).is_some();
            is_task.then(|| edit::set_field(text, key, day))
        });
        self.after_note_change(changed, Some(format!("{key}:{day}")));
    }

    pub(crate) fn filter_vault(&mut self) {
        let id = self.buffer().id();
        let Some(kind) = self.list_kind(id) else {
            return;
        };
        let names: Vec<String> = self.vaults().into_iter().map(|(name, _)| name).collect();
        let next = |filter: Option<String>| match filter {
            None => names.first().cloned(),
            Some(name) => names
                .iter()
                .position(|known| *known == name)
                .and_then(|at| names.get(at + 1).cloned()),
        };
        let kind = match kind {
            ListKind::Agenda(filter) => ListKind::Agenda(next(filter)),
            ListKind::Calendar(mut month) => {
                month.filter = next(month.filter);
                ListKind::Calendar(month)
            }
            ListKind::Backlinks(_) => return,
        };
        self.set_kind(id, kind.clone());
        self.render_list(&kind);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use oxmacs_core::editor::Editor;

    use crate::testing::{lines, press, slashed, type_text, write};

    fn agenda() -> (tempfile::TempDir, PathBuf, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let personal = dir.path().join("personal");
        let work = dir.path().join("work");
        let garden = write(
            &personal.join("Garden.atlas"),
            "[ ] Buy seeds  due:2026-10-19  !high\n[ ] Rake  on:2026-10-21\n@2026-10-19 10:00-11:00 Visit nursery\n",
        );
        write(
            &work.join("Office.atlas"),
            "[ ] Send invoice  due:2026-10-15\n",
        );
        let text = format!(
            "[[atlas.vault]]\nname = \"personal\"\npath = \"{}\"\n[[atlas.vault]]\nname = \"work\"\npath = \"{}\"\ncolor = \"red\"\n",
            slashed(&personal),
            slashed(&work)
        );
        let mut editor = crate::testing::configured(&text);
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

        let id = editor.buffer_visiting(&garden).unwrap();
        let buffer = editor.buffer_by_id(id).unwrap();
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
