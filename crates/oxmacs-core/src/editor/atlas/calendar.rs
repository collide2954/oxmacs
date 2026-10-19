use std::fmt::Write;
use std::path::PathBuf;

use jiff::ToSpan;
use jiff::civil::Date;

use crate::atlas::agenda::{Entry, Kind};
use crate::atlas::calendar;
use crate::editor::Editor;
use crate::editor::atlas::agenda::{color, fit, lead};
use crate::editor::atlas::list::{ListKind, Month};
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::theme::Highlight;

const HELP: &str = "arrows day · n/p month · w week · . today · v vault · RET visit · q quit";
const MIN_CELL: usize = 9;
const ENTRY_LINES: usize = 3;
const FIRST_HOUR: i8 = 7;
const LAST_HOUR: i8 = 21;
const ROOM: usize = 84;
const TIMES: usize = 7;
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

type Rows = Vec<(usize, PathBuf, usize)>;

fn short(entry: &Entry) -> String {
    match &entry.kind {
        Kind::Event {
            start: Some(start), ..
        } => format!("{} {}", start.strftime("%H:%M"), entry.title),
        Kind::Event { .. } => entry.title.clone(),
        Kind::Task { state, .. } => format!("[{}] {}", state.mark(), entry.title),
    }
}

fn entry_face(entry: &Entry) -> Highlight {
    match &entry.kind {
        Kind::Event { .. } => Highlight::Attribute,
        Kind::Task { .. } => lead(entry).1,
    }
}

struct Grid<'a> {
    days: &'a [(Date, Vec<Entry>)],
    cell: usize,
    today: Date,
    selected: Date,
}

impl Grid<'_> {
    fn entries(&self, day: Date) -> &[Entry] {
        self.days
            .iter()
            .find(|(known, _)| *known == day)
            .map_or(&[], |(_, entries)| entries.as_slice())
    }

    fn number(&self, day: Date, month: Date) -> (String, Option<Highlight>) {
        let text = if day == self.selected {
            format!("[{}]", day.day())
        } else {
            format!(" {}", day.day())
        };
        let face = if day == self.selected {
            Some(Highlight::Heading)
        } else if day == self.today {
            Some(Highlight::Link)
        } else if day.month() != month.month() {
            Some(Highlight::Comment)
        } else {
            None
        };
        (format!("{text:<width$}", width = self.cell), face)
    }

    fn line(&self, day: Date, row: usize) -> (String, Option<Highlight>) {
        let entries = self.entries(day);
        let width = self.cell.saturating_sub(1);
        let blank = || (" ".repeat(self.cell), None);
        if entries.len() > ENTRY_LINES && row == ENTRY_LINES - 1 {
            let more = format!(" +{} more", entries.len() - row);
            return (fit(&more, self.cell), Some(Highlight::Comment));
        }
        match entries.get(row) {
            Some(entry) => (
                format!(" {}", fit(&short(entry), width)),
                Some(entry_face(entry)),
            ),
            None => blank(),
        }
    }
}

impl Editor {
    pub(in crate::editor) fn atlas_calendar(&mut self) {
        if self.config.atlas_vault.is_empty() {
            self.show_message("No Atlas vault yet; C-c n f offers to make one");
            return;
        }
        let month = Month {
            selected: (self.today)(),
            week: false,
            filter: None,
        };
        self.show_atlas_list(&ListKind::Calendar(month));
    }

    fn calendar_month(&self) -> Option<Month> {
        match self.atlas_list_kind(0)? {
            ListKind::Calendar(month) => Some(month),
            _ => None,
        }
    }

    fn set_calendar(&mut self, month: Month) {
        if let Some(Mode::AtlasList(list)) = self.buffers.mode_mut(0) {
            list.kind = ListKind::Calendar(month);
        }
        self.render_calendar();
    }

    pub(in crate::editor) fn atlas_calendar_move(&mut self, days: i64) {
        if let Some(mut month) = self.calendar_month() {
            month.selected = month
                .selected
                .checked_add(days.days())
                .unwrap_or(month.selected);
            self.set_calendar(month);
        }
    }

    pub(in crate::editor) fn atlas_calendar_month(&mut self, forward: bool) {
        let Some(mut month) = self.calendar_month() else {
            return;
        };
        let span = match (month.week, forward) {
            (true, true) => 7.days(),
            (true, false) => (-7).days(),
            (false, true) => 1.month(),
            (false, false) => (-1).month(),
        };
        month.selected = month.selected.checked_add(span).unwrap_or(month.selected);
        self.set_calendar(month);
    }

    pub(in crate::editor) fn atlas_calendar_week(&mut self) {
        if let Some(mut month) = self.calendar_month() {
            month.week = !month.week;
            self.set_calendar(month);
        }
    }

    pub(in crate::editor) fn atlas_calendar_today(&mut self) {
        if let Some(mut month) = self.calendar_month() {
            month.selected = (self.today)();
            self.set_calendar(month);
        }
    }

    pub(in crate::editor) fn atlas_calendar_select(&mut self) {
        if self.atlas_row().is_some() {
            self.atlas_visit();
            return;
        }
        let first = match self.buffers.mode(0) {
            Some(Mode::AtlasList(list)) => list.rows.first().map(|(row, ..)| *row),
            _ => None,
        };
        match first {
            Some(row) => {
                let buffer = self.buffers.current_mut();
                let start = buffer.line_start(row);
                buffer.set_point(start);
            }
            None => self.show_message("Nothing planned that day"),
        }
    }

    pub(in crate::editor) fn render_calendar(&mut self) {
        let Some(index) = self.atlas_list_index(&ListKind::Calendar(Month {
            selected: (self.today)(),
            week: false,
            filter: None,
        })) else {
            return;
        };
        let Some(ListKind::Calendar(month)) = self.atlas_list_kind(index) else {
            return;
        };
        let room = self
            .buffers
            .get(index)
            .and_then(|buffer| self.width_showing(buffer.id()))
            .filter(|width| *width > 0)
            .unwrap_or(ROOM);
        let today = (self.today)();
        let weeks = if month.week {
            let monday = month
                .selected
                .checked_sub(i64::from(month.selected.weekday().to_monday_zero_offset()).days());
            let monday = monday.unwrap_or(month.selected);
            calendar::weeks(monday)
                .into_iter()
                .filter(|week| week.contains(&monday))
                .collect()
        } else {
            calendar::weeks(month.selected)
        };
        let from = weeks.first().map_or(month.selected, |week| week[0]);
        let to = weeks.last().map_or(month.selected, |week| week[6]);
        let days = calendar::days(self.atlas_index(), from, to, month.filter.as_deref());
        let mut page = Page::default();
        let title = if month.week {
            format!("Week of {}", from.strftime("%-d %B %Y"))
        } else {
            month.selected.strftime("%B %Y").to_string()
        };
        let shown = month.filter.as_deref().unwrap_or("all");
        page.line(&[
            (&title, Some(Highlight::Heading)),
            (&format!("   vault: {shown}"), Some(Highlight::Comment)),
        ]);
        page.line(&[(HELP, Some(Highlight::Comment))]);
        page.line(&[]);
        let grid_room = if month.week {
            room.saturating_sub(TIMES)
        } else {
            room
        };
        let grid = Grid {
            days: &days,
            cell: (grid_room / 7).max(MIN_CELL),
            today,
            selected: month.selected,
        };
        let point = if month.week {
            week_view(&mut page, &grid, &weeks[0])
        } else {
            month_view(&mut page, &grid, &weeks, month.selected)
        };
        let rows = self.calendar_details(&mut page, &grid, grid_room);
        if let Some((buffer, Mode::AtlasList(list))) = self.buffers.entry_mut(index) {
            page.fill(buffer);
            let (line, column) = point;
            let start = buffer.line_start(line);
            buffer.set_point((start + column).min(buffer.line_end(line)));
            list.rows = rows;
            list.styles = page.styles;
        }
    }

    fn calendar_details(&self, page: &mut Page, grid: &Grid<'_>, room: usize) -> Rows {
        let vaults: Vec<(String, Highlight)> = self
            .config
            .atlas_vault
            .iter()
            .enumerate()
            .map(|(position, vault)| (vault.name.clone(), color(vault.color.as_deref(), position)))
            .collect();
        page.line(&[]);
        page.heading(&grid.selected.strftime("%A %-d %B").to_string());
        let entries = grid.entries(grid.selected);
        let mut rows = Vec::new();
        if entries.is_empty() {
            page.line(&[("  Nothing planned", Some(Highlight::Comment))]);
        }
        for entry in entries {
            let (lead, face) = lead(entry);
            let vault_face = vaults
                .iter()
                .find(|(name, _)| *name == entry.vault)
                .map_or(Highlight::Comment, |(_, face)| *face);
            rows.push((page.styles.len(), entry.path.clone(), entry.line));
            let width = room
                .saturating_sub(16 + entry.vault.chars().count())
                .max(10);
            page.line(&[
                (&format!("  {lead:<12}"), Some(face)),
                (&format!("{} ", fit(&entry.title, width)), None),
                (&entry.vault, Some(vault_face)),
            ]);
        }
        rows
    }
}

fn month_view(
    page: &mut Page,
    grid: &Grid<'_>,
    weeks: &[[Date; 7]],
    month: Date,
) -> (usize, usize) {
    let header = WEEKDAYS.iter().fold(String::new(), |mut header, day| {
        let _ = write!(header, " {day:<width$}", width = grid.cell - 1);
        header
    });
    page.line(&[(header.trim_end(), Some(Highlight::Comment))]);
    let mut point = (0, 0);
    for week in weeks {
        let numbers: Vec<(String, Option<Highlight>)> =
            week.iter().map(|day| grid.number(*day, month)).collect();
        if let Some(column) = week.iter().position(|day| *day == grid.selected) {
            point = (page.styles.len(), column * grid.cell);
        }
        let parts: Vec<(&str, Option<Highlight>)> = numbers
            .iter()
            .map(|(text, face)| (text.as_str(), *face))
            .collect();
        page.line(&parts);
        for row in 0..ENTRY_LINES {
            let cells: Vec<(String, Option<Highlight>)> =
                week.iter().map(|day| grid.line(*day, row)).collect();
            if cells.iter().all(|(text, _)| text.trim().is_empty()) {
                continue;
            }
            let parts: Vec<(&str, Option<Highlight>)> = cells
                .iter()
                .map(|(text, face)| (text.as_str(), *face))
                .collect();
            page.line(&parts);
        }
    }
    point
}

fn week_view(page: &mut Page, grid: &Grid<'_>, week: &[Date; 7]) -> (usize, usize) {
    let header: Vec<(String, Option<Highlight>)> = week
        .iter()
        .map(|day| {
            let weekday = usize::try_from(day.weekday().to_monday_zero_offset()).unwrap_or(0);
            let label = format!("{} {}", WEEKDAYS[weekday], day.day());
            let (marked, face) = if *day == grid.selected {
                (format!("[{label}]"), Some(Highlight::Heading))
            } else if *day == grid.today {
                (format!(" {label}"), Some(Highlight::Link))
            } else {
                (format!(" {label}"), None)
            };
            (format!("{marked:<width$}", width = grid.cell), face)
        })
        .collect();
    let point_column = week
        .iter()
        .position(|day| *day == grid.selected)
        .map_or(TIMES, |column| TIMES + column * grid.cell);
    let point = (page.styles.len(), point_column);
    let mut parts: Vec<(&str, Option<Highlight>)> = vec![("       ", None)];
    parts.extend(header.iter().map(|(text, face)| (text.as_str(), *face)));
    page.line(&parts);
    let untimed = |day: Date| -> Vec<&Entry> {
        grid.entries(day)
            .iter()
            .filter(|entry| {
                !matches!(
                    entry.kind,
                    Kind::Event { start: Some(_), .. } | Kind::Task { at: Some(_), .. }
                )
            })
            .collect()
    };
    for row in 0..ENTRY_LINES {
        let cells: Vec<(String, Option<Highlight>)> = week
            .iter()
            .map(|day| match untimed(*day).get(row) {
                Some(entry) => (
                    format!(" {}", fit(&short(entry), grid.cell - 1)),
                    Some(entry_face(entry)),
                ),
                None => (" ".repeat(grid.cell), None),
            })
            .collect();
        if row > 0 && cells.iter().all(|(text, _)| text.trim().is_empty()) {
            continue;
        }
        let label = if row == 0 { "all day" } else { "       " };
        let mut parts: Vec<(&str, Option<Highlight>)> = vec![(label, Some(Highlight::Comment))];
        parts.extend(cells.iter().map(|(text, face)| (text.as_str(), *face)));
        page.line(&parts);
    }
    for hour in FIRST_HOUR..=LAST_HOUR {
        let cells: Vec<(String, Option<Highlight>)> =
            week.iter().map(|day| hour_cell(grid, *day, hour)).collect();
        let label = format!("{hour:02}:00  ");
        let mut parts: Vec<(&str, Option<Highlight>)> = vec![(&label, Some(Highlight::Comment))];
        parts.extend(cells.iter().map(|(text, face)| (text.as_str(), *face)));
        page.line(&parts);
    }
    point
}

fn hour_cell(grid: &Grid<'_>, day: Date, hour: i8) -> (String, Option<Highlight>) {
    for entry in grid.entries(day) {
        let (start, end) = match &entry.kind {
            Kind::Event {
                start: Some(start),
                end,
            } => (*start, *end),
            Kind::Task { at: Some(at), .. } => (*at, None),
            _ => continue,
        };
        if start.hour() == hour {
            return (
                format!(" {}", fit(&short(entry), grid.cell - 1)),
                Some(entry_face(entry)),
            );
        }
        let last = end.map_or(start.hour(), |end| {
            if end.minute() == 0 {
                end.hour() - 1
            } else {
                end.hour()
            }
        });
        if start.hour() < hour && hour <= last {
            return (
                format!("{:<width$}", " │", width = grid.cell),
                Some(entry_face(entry)),
            );
        }
    }
    (" ".repeat(grid.cell), None)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use jiff::civil::date;

    use crate::config::Config;
    use crate::editor::Editor;
    use crate::editor::test_support::{lines, press};

    fn calendar(extra: &str) -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Garden.atlas"),
            format!("[ ] Rake  on:2026-10-21\n@2026-10-19 10:00-12:00 Visit nursery\n[ ] Seeds  due:2026-10-19\n{extra}"),
        )
        .unwrap();
        let text = format!(
            "[[atlas-vault]]\nname = \"personal\"\npath = \"{}\"\n",
            dir.path().display().to_string().replace('\\', "/")
        );
        let mut editor = Editor::new();
        editor.apply_config(&Config::parse(&text).unwrap());
        editor.today = || date(2026, 10, 19);
        press(&mut editor, "C-c n m");
        let window = editor.selected_window();
        editor.resize_pane(window, 40, 85);
        (dir, editor)
    }

    fn line_with(editor: &Editor, text: &str) -> String {
        lines(editor)
            .into_iter()
            .find(|line| line.contains(text))
            .unwrap_or_else(|| panic!("no {text} in {:?}", lines(editor)))
    }

    #[test]
    fn c_c_n_m_draws_the_month_with_the_day_selected() {
        let (_dir, editor) = calendar("");

        assert_eq!(editor.buffer().name(), "*Calendar*");
        assert!(lines(&editor)[0].starts_with("October 2026"));
        assert!(line_with(&editor, "Mon").contains("Tue"));
        let week = line_with(&editor, "[19]");
        assert!(week.starts_with(" 19") || week.contains("[19]"));
        let cell = week.find("[19]").unwrap();
        assert!(line_with(&editor, "10:00 Visi")[cell..].starts_with(" 10:00 Visi"));
        assert!(line_with(&editor, "[ ] Rake").contains("[ ] Rake"));
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 7);
    }

    #[test]
    fn the_selected_day_is_listed_below() {
        let (_dir, editor) = calendar("");

        assert!(lines(&editor).contains(&"Monday 19 October".to_owned()));
        assert!(line_with(&editor, "10:00-12:00").contains("Visit nursery"));
        assert!(
            lines(&editor)
                .iter()
                .any(|line| line.contains("Seeds") && line.trim_end().ends_with("personal"))
        );
    }

    #[test]
    fn arrows_move_the_day_and_n_p_the_month() {
        let (_dir, mut editor) = calendar("");

        press(&mut editor, "<right> <right>");
        assert!(lines(&editor).contains(&"Wednesday 21 October".to_owned()));
        press(&mut editor, "<down>");
        assert!(lines(&editor).contains(&"Wednesday 28 October".to_owned()));

        press(&mut editor, "n");
        assert!(lines(&editor)[0].starts_with("November 2026"));
        press(&mut editor, "p p");
        assert!(lines(&editor)[0].starts_with("September 2026"));
        press(&mut editor, ".");
        assert!(lines(&editor)[0].starts_with("October 2026"));
    }

    #[test]
    fn busy_days_show_how_many_more_there_are() {
        let (_dir, editor) = calendar("[ ] One  on:2026-10-19\n[ ] Two  on:2026-10-19\n");

        assert!(
            lines(&editor).iter().any(|line| line.contains("+2 more")),
            "{:?}",
            lines(&editor)
        );
    }

    #[test]
    fn return_steps_into_the_day_and_visits_an_entry() {
        let (_dir, mut editor) = calendar("");

        press(&mut editor, "RET");
        assert!(
            lines(&editor)[editor.buffer().line_of(editor.buffer().point())]
                .contains("Visit nursery")
        );
        press(&mut editor, "RET");

        assert_eq!(editor.buffer().name(), "Garden.atlas");
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 1);
    }

    #[test]
    fn w_shows_the_week_by_hour() {
        let (_dir, mut editor) = calendar("");

        press(&mut editor, "w");

        assert!(lines(&editor)[0].starts_with("Week of 19 October 2026"));
        let header = line_with(&editor, "Tue 20");
        let column = header.find("[Mon 19]").unwrap();
        assert!(line_with(&editor, "10:00  ")[column..].starts_with(" 10:00 Vi"));
        assert!(line_with(&editor, "11:00  ")[column..].starts_with(" │"));
        assert!(!line_with(&editor, "12:00  ")[column..].starts_with(" │"));
        assert!(line_with(&editor, "all day").contains("[ ] Seeds"));
        press(&mut editor, "n");
        assert!(lines(&editor)[0].starts_with("Week of 26 October 2026"));
    }
}
