use std::path::PathBuf;

use jiff::ToSpan;
use jiff::civil::{Date, Time};

use crate::atlas::note::{Priority, State};
use crate::atlas::vault::Index;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Event {
        start: Option<Time>,
        end: Option<Time>,
    },
    Task {
        state: State,
        due: Option<Date>,
        on: Option<Date>,
        at: Option<Time>,
        priority: Option<Priority>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub vault: String,
    pub path: PathBuf,
    pub line: usize,
    pub title: String,
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Heading {
    Overdue,
    Day(Date),
    Waiting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub heading: Heading,
    pub entries: Vec<Entry>,
}

pub(crate) fn order(entry: &Entry) -> (u8, Option<Time>, u8, String) {
    match &entry.kind {
        Kind::Event { start, .. } => (0, *start, 0, entry.title.to_lowercase()),
        Kind::Task {
            state,
            at,
            priority,
            ..
        } => (
            if *state == State::Doing { 1 } else { 2 },
            *at,
            priority.map_or(3, |priority| priority as u8),
            entry.title.to_lowercase(),
        ),
    }
}

fn place(sections: &mut Vec<Section>, heading: Heading, entry: Entry) {
    match sections
        .iter_mut()
        .find(|section| section.heading == heading)
    {
        Some(section) => section.entries.push(entry),
        None => sections.push(Section {
            heading,
            entries: vec![entry],
        }),
    }
}

pub fn build(index: &Index, today: Date, days: i64, vault: Option<&str>) -> Vec<Section> {
    let last = today.checked_add(days.days()).unwrap_or(today);
    let mut sections = vec![Section {
        heading: Heading::Day(today),
        entries: Vec::new(),
    }];
    let pages = index
        .pages
        .iter()
        .filter(|page| vault.is_none_or(|vault| page.vault == vault));
    for page in pages {
        let entry = |line: usize, title: &str, kind: Kind| Entry {
            vault: page.vault.clone(),
            path: page.path.clone(),
            line,
            title: title.to_owned(),
            kind,
        };
        for event in &page.note.events {
            if (today..=last).contains(&event.date) {
                let kind = Kind::Event {
                    start: event.start,
                    end: event.end,
                };
                place(
                    &mut sections,
                    Heading::Day(event.date),
                    entry(event.line, &event.title, kind),
                );
            }
        }
        for task in page.note.tasks.iter().filter(|task| !task.state.finished()) {
            let when = match (task.on, task.due) {
                (Some(on), Some(due)) => Some(on.min(due)),
                (on, due) => on.or(due),
            };
            let heading = match when {
                Some(when) if when < today => Heading::Overdue,
                Some(when) if when <= last => Heading::Day(when),
                _ if task.state == State::Waiting => Heading::Waiting,
                _ => continue,
            };
            let kind = Kind::Task {
                state: task.state,
                due: task.due,
                on: task.on,
                at: task.at,
                priority: task.priority,
            };
            place(&mut sections, heading, entry(task.line, &task.title, kind));
        }
    }
    sections.sort_by_key(|section| section.heading);
    for section in &mut sections {
        if section.heading == Heading::Overdue {
            section.entries.sort_by_key(|entry| match &entry.kind {
                Kind::Task { on, due, .. } => (on.min(due).or(*on).or(*due), order(entry)),
                Kind::Event { .. } => (None, order(entry)),
            });
        } else {
            section.entries.sort_by_key(order);
        }
    }
    sections
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use jiff::civil::{date, time};

    use super::*;
    use crate::atlas::vault;

    const TODAY: Date = date(2026, 10, 19);

    fn index() -> Index {
        let page = |vault_name: &str, name: &str, text: &str| {
            vault::page(vault_name, Path::new(name), text, TODAY)
        };
        Index {
            pages: vec![
                page(
                    "personal",
                    "Garden.atlas",
                    "[ ] Buy seeds  due:2026-10-19  !high\n[/] Order soil  on:2026-10-19 14:00\n[ ] Rake  on:2026-10-21\n[x] Measure  due:2026-10-19\n[ ] Someday\n[?] Feedback from Ana\n[?] Quote  due:2026-10-22\n@2026-10-19 10:00-11:00 Visit nursery\n@2026-10-18 Old event\n@2026-11-30 Far event\n",
                ),
                page(
                    "work",
                    "Office.atlas",
                    "[ ] Send invoice  due:2026-10-15\n[ ] Plan  on:2026-10-17 due:2026-10-30\n[ ] Review  due:2026-10-26\n[ ] Later  due:2026-10-27\n",
                ),
            ],
        }
    }

    fn titles(section: &Section) -> Vec<&str> {
        section
            .entries
            .iter()
            .map(|entry| entry.title.as_str())
            .collect()
    }

    #[test]
    fn tasks_and_events_fall_into_overdue_days_and_waiting() {
        let sections = build(&index(), TODAY, 7, None);

        let headings: Vec<Heading> = sections.iter().map(|section| section.heading).collect();
        assert_eq!(
            headings,
            [
                Heading::Overdue,
                Heading::Day(TODAY),
                Heading::Day(date(2026, 10, 21)),
                Heading::Day(date(2026, 10, 22)),
                Heading::Day(date(2026, 10, 26)),
                Heading::Waiting,
            ]
        );
        assert_eq!(titles(&sections[0]), ["Send invoice", "Plan"]);
        assert_eq!(
            titles(&sections[1]),
            ["Visit nursery", "Order soil", "Buy seeds"]
        );
        assert_eq!(titles(&sections[2]), ["Rake"]);
        assert_eq!(titles(&sections[3]), ["Quote"]);
        assert_eq!(titles(&sections[4]), ["Review"]);
        assert_eq!(titles(&sections[5]), ["Feedback from Ana"]);
    }

    #[test]
    fn entries_know_where_they_came_from() {
        let sections = build(&index(), TODAY, 7, None);
        let soil = &sections[1].entries[1];

        assert_eq!(soil.vault, "personal");
        assert_eq!(soil.path, Path::new("Garden.atlas"));
        assert_eq!(soil.line, 1);
        assert_eq!(
            soil.kind,
            Kind::Task {
                state: State::Doing,
                due: None,
                on: Some(TODAY),
                at: Some(time(14, 0, 0, 0)),
                priority: None,
            }
        );
    }

    #[test]
    fn a_vault_filter_keeps_only_its_pages() {
        let sections = build(&index(), TODAY, 7, Some("work"));

        assert_eq!(titles(&sections[0]), ["Send invoice", "Plan"]);
        assert_eq!(sections[1].heading, Heading::Day(TODAY));
        assert_eq!(titles(&sections[1]), Vec::<&str>::new());
        assert_eq!(sections.len(), 3);
    }
}
