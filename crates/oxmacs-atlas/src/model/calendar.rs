use jiff::ToSpan;
use jiff::civil::Date;

use crate::model::agenda::{self, Entry, Kind};
use crate::model::vault::Index;

pub fn weeks(day: Date) -> Vec<[Date; 7]> {
    let first = day.first_of_month();
    let last = day.last_of_month();
    let back = i64::from(first.weekday().to_monday_zero_offset());
    let mut start = first.checked_sub(back.days()).unwrap_or(first);
    let mut found = Vec::new();
    while start <= last {
        let week: [Date; 7] = std::array::from_fn(|offset| {
            let offset = i32::try_from(offset).unwrap_or(0);
            start.checked_add(offset.days()).unwrap_or(start)
        });
        found.push(week);
        start = start.checked_add(7.days()).unwrap_or(last);
        if found.len() > 6 {
            break;
        }
    }
    found
}

pub fn days(index: &Index, from: Date, to: Date, vault: Option<&str>) -> Vec<(Date, Vec<Entry>)> {
    let mut found: Vec<(Date, Vec<Entry>)> = Vec::new();
    let mut add = |date: Date, entry: Entry| {
        if !(from..=to).contains(&date) {
            return;
        }
        match found.iter_mut().find(|(known, _)| *known == date) {
            Some((_, entries)) => entries.push(entry),
            None => found.push((date, vec![entry])),
        }
    };
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
            let kind = Kind::Event {
                start: event.start,
                end: event.end,
            };
            add(event.date, entry(event.line, &event.title, kind));
        }
        for task in page.note.tasks.iter().filter(|task| !task.state.finished()) {
            let when = match (task.on, task.due) {
                (Some(on), Some(due)) => Some(on.min(due)),
                (on, due) => on.or(due),
            };
            let Some(when) = when else {
                continue;
            };
            let kind = Kind::Task {
                state: task.state,
                due: task.due,
                on: task.on,
                at: task.at,
                priority: task.priority,
            };
            add(when, entry(task.line, &task.title, kind));
        }
    }
    found.sort_by_key(|(date, _)| *date);
    for (_, entries) in &mut found {
        entries.sort_by_key(agenda::order);
    }
    found
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use jiff::civil::date;

    use super::*;
    use crate::model::vault;

    #[test]
    fn months_are_whole_weeks_from_monday() {
        let october = weeks(date(2026, 10, 19));

        assert_eq!(october.len(), 5);
        assert_eq!(october[0][0], date(2026, 9, 28));
        assert_eq!(october[0][3], date(2026, 10, 1));
        assert_eq!(october[4][6], date(2026, 11, 1));
        assert_eq!(weeks(date(2026, 2, 10)).len(), 5);
        assert_eq!(weeks(date(2027, 2, 10)).len(), 4);
        assert_eq!(weeks(date(2026, 3, 10)).len(), 6);
    }

    #[test]
    fn days_gather_events_and_open_tasks_in_a_range() {
        let today = date(2026, 10, 19);
        let index = Index {
            pages: vec![vault::page(
                "personal",
                Path::new("Garden.atlas"),
                "[ ] Rake  on:2026-10-21\n[ ] Seeds  on:2026-10-25 due:2026-10-22\n[x] Done  due:2026-10-21\n[ ] Undated\n@2026-10-21 09:00 Dentist\n@2026-12-01 Later\n",
                today,
            )],
        };

        let found = days(&index, date(2026, 10, 1), date(2026, 10, 31), None);

        let titles: Vec<(Date, Vec<&str>)> = found
            .iter()
            .map(|(day, entries)| {
                (
                    *day,
                    entries.iter().map(|entry| entry.title.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(
            titles,
            [
                (date(2026, 10, 21), vec!["Dentist", "Rake"]),
                (date(2026, 10, 22), vec!["Seeds"]),
            ]
        );
        assert_eq!(
            days(&index, date(2026, 10, 1), date(2026, 10, 31), Some("work")),
            []
        );
    }
}
