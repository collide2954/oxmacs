use std::ops::Range;

use jiff::Unit;
use jiff::civil::{Date, DateTime, Time};

use crate::atlas::date::{self, Every};

pub const COMMENTS: &str = "--- comments";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum State {
    Todo,
    Doing,
    Waiting,
    Done,
    Dropped,
}

impl State {
    pub fn from_mark(mark: char) -> Option<Self> {
        match mark {
            ' ' => Some(Self::Todo),
            '/' => Some(Self::Doing),
            '?' => Some(Self::Waiting),
            'x' | 'X' => Some(Self::Done),
            '-' => Some(Self::Dropped),
            _ => None,
        }
    }

    pub fn mark(self) -> char {
        match self {
            Self::Todo => ' ',
            Self::Doing => '/',
            Self::Waiting => '?',
            Self::Done => 'x',
            Self::Dropped => '-',
        }
    }

    pub fn cycled(self) -> Self {
        match self {
            Self::Todo => Self::Doing,
            Self::Doing => Self::Done,
            Self::Waiting | Self::Done | Self::Dropped => Self::Todo,
        }
    }

    pub fn finished(self) -> bool {
        matches!(self, Self::Done | Self::Dropped)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    pub line: usize,
    pub date: Date,
    pub start: Time,
    pub end: Option<Time>,
}

impl Clock {
    pub fn minutes(&self, now: DateTime) -> i64 {
        let start = self.date.to_datetime(self.start);
        let end = match self.end {
            Some(end) if end >= self.start => self.date.to_datetime(end),
            Some(end) => self
                .date
                .tomorrow()
                .map_or(start, |next| next.to_datetime(end)),
            None => now,
        };
        start
            .until((Unit::Minute, end))
            .map_or(0, |span| span.get_minutes())
            .max(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub line: usize,
    pub depth: usize,
    pub state: State,
    pub title: String,
    pub due: Option<Date>,
    pub on: Option<Date>,
    pub at: Option<Time>,
    pub done: Option<Date>,
    pub every: Option<Every>,
    pub priority: Option<Priority>,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub clocks: Vec<Clock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub line: usize,
    pub level: usize,
    pub title: String,
    pub due: Option<Date>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub line: usize,
    pub date: Date,
    pub start: Option<Time>,
    pub end: Option<Time>,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub id: String,
    pub line: usize,
    pub entries: Vec<String>,
    pub resolved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Key,
    Separator,
    Heading(usize),
    Box(State),
    Finished,
    Field,
    Loose,
    Priority,
    Tag,
    Link,
    Anchor,
    When,
    Bullet,
    Comment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub columns: Range<usize>,
    pub style: Style,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Note {
    pub header: Vec<(String, String)>,
    pub headings: Vec<Heading>,
    pub tasks: Vec<Task>,
    pub events: Vec<Event>,
    pub links: Vec<String>,
    pub tags: Vec<String>,
    pub anchors: Vec<(usize, String)>,
    pub threads: Vec<Thread>,
    pub spans: Vec<Vec<Span>>,
}

impl Note {
    pub fn field(&self, key: &str) -> Option<&str> {
        self.header
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Default)]
struct Inline {
    links: Vec<String>,
    tags: Vec<String>,
}

fn header_field(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once(':')?;
    let valid = key.chars().next().is_some_and(char::is_alphabetic)
        && key
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'));
    valid.then(|| (key, value.trim()))
}

fn header_length(lines: &[&str]) -> usize {
    let fields = lines
        .iter()
        .take_while(|line| header_field(line).is_some())
        .count();
    if fields > 0
        && lines
            .get(fields)
            .is_some_and(|line| line.trim_end() == "---")
    {
        fields + 1
    } else {
        0
    }
}

fn words(text: &str) -> Vec<(Range<usize>, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut start = None;
    for (index, c) in chars.iter().enumerate() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(index),
            (true, Some(from)) => {
                found.push((from..index, chars[from..index].iter().collect()));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        found.push((from..chars.len(), chars[from..].iter().collect()));
    }
    found
}

fn time(word: &str) -> Option<Time> {
    let (hour, minute) = word.split_once(':')?;
    Time::new(hour.parse().ok()?, minute.parse().ok()?, 0, 0).ok()
}

fn indent(text: &str) -> usize {
    text.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

fn inline(text: &str, line: usize, spans: &mut Vec<Span>, note: &mut Note) -> Inline {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Inline::default();
    let mut index = 0;
    while index < chars.len() {
        let rest = &chars[index..];
        let after_space = index == 0 || chars[index - 1].is_whitespace();
        let taken = match rest[0] {
            '[' => link(rest).map(|(length, target)| {
                found.links.push(target);
                (length, Style::Link)
            }),
            '{' => anchor(rest).map(|(length, id)| {
                note.anchors.push((line, id));
                (length, Style::Anchor)
            }),
            '#' if after_space => tag(rest).map(|(length, name)| {
                found.tags.push(name);
                (length, Style::Tag)
            }),
            _ => None,
        };
        match taken {
            Some((length, style)) => {
                spans.push(Span {
                    columns: index..index + length,
                    style,
                });
                index += length;
            }
            None => index += 1,
        }
    }
    for link in &found.links {
        if !note.links.contains(link) {
            note.links.push(link.clone());
        }
    }
    for tag in &found.tags {
        if !note.tags.contains(tag) {
            note.tags.push(tag.clone());
        }
    }
    found
}

fn link(rest: &[char]) -> Option<(usize, String)> {
    let close = rest[1..].iter().position(|c| *c == ']')?;
    let target: String = rest[1..=close].iter().collect();
    let target = target.trim();
    (!target.is_empty()).then(|| (close + 2, target.to_owned()))
}

fn anchor(rest: &[char]) -> Option<(usize, String)> {
    if rest.get(1) != Some(&'c') {
        return None;
    }
    let digits = rest[2..].iter().take_while(|c| c.is_ascii_digit()).count();
    (digits > 0 && rest.get(2 + digits) == Some(&'}'))
        .then(|| (digits + 3, rest[1..2 + digits].iter().collect()))
}

fn tag(rest: &[char]) -> Option<(usize, String)> {
    let length = rest[1..]
        .iter()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/'))
        .count();
    (length > 0).then(|| (length + 1, rest[1..=length].iter().collect()))
}

fn field(word: &str, today: Date, task: &mut Task) -> Option<Style> {
    let lower = word.to_lowercase();
    let priority = match lower.as_str() {
        "!high" | "!h" => Some(Priority::High),
        "!medium" | "!med" | "!m" => Some(Priority::Medium),
        "!low" | "!l" => Some(Priority::Low),
        _ => None,
    };
    if priority.is_some() {
        task.priority = priority;
        return Some(Style::Priority);
    }
    let (key, value) = lower.split_once(':')?;
    if value.is_empty() {
        return None;
    }
    if key == "every" {
        task.every = date::parse_every(value);
        return Some(if task.every.is_some() {
            Style::Field
        } else {
            Style::Loose
        });
    }
    let slot = match key {
        "due" => &mut task.due,
        "on" => &mut task.on,
        "done" => &mut task.done,
        _ => return None,
    };
    let (found, style) = dated(value, today);
    *slot = found;
    Some(style)
}

fn dated(value: &str, today: Date) -> (Option<Date>, Style) {
    let exact = value.parse::<Date>().ok();
    let style = if exact.is_some() {
        Style::Field
    } else {
        Style::Loose
    };
    (exact.or_else(|| date::resolve(value, today)), style)
}

fn task(line: usize, text: &str, today: Date, note: &mut Note) -> Option<(Task, Vec<Span>)> {
    let depth = indent(text);
    let rest: Vec<char> = text.chars().skip(depth).collect();
    let boxed = rest.len() >= 3 && rest[0] == '[' && rest[2] == ']';
    if !boxed || rest.get(3).is_some_and(|c| *c != ' ') {
        return None;
    }
    let state = State::from_mark(rest[1])?;
    let mut spans = vec![Span {
        columns: depth..depth + 3,
        style: Style::Box(state),
    }];
    let start = depth + 3;
    let body: String = rest[3..].iter().collect();
    let mut found = Task {
        line,
        depth,
        state,
        title: String::new(),
        due: None,
        on: None,
        at: None,
        done: None,
        every: None,
        priority: None,
        tags: Vec::new(),
        links: Vec::new(),
        clocks: Vec::new(),
    };
    if state.finished() {
        spans.push(Span {
            columns: start..start + body.chars().count(),
            style: Style::Finished,
        });
    }
    let mut title = Vec::new();
    let mut after_on = false;
    for (columns, word) in words(&body) {
        let columns = start + columns.start..start + columns.end;
        if std::mem::take(&mut after_on)
            && let Some(at) = time(&word)
        {
            found.at = Some(at);
            spans.push(Span {
                columns,
                style: Style::Field,
            });
            continue;
        }
        if let Some(style) = field(&word, today, &mut found) {
            after_on = word.to_lowercase().starts_with("on:");
            spans.push(Span { columns, style });
            continue;
        }
        title.push(word);
    }
    found.title = title.join(" ");
    let mut body_spans = Vec::new();
    let Inline { links, tags } = inline(&body, line, &mut body_spans, note);
    found.links = links;
    found.tags = tags;
    spans.extend(body_spans.into_iter().map(|span| Span {
        columns: start + span.columns.start..start + span.columns.end,
        style: span.style,
    }));
    Some((found, spans))
}

fn event(line: usize, text: &str) -> Option<(Event, Span)> {
    let depth = indent(text);
    let rest: String = text.chars().skip(depth).collect();
    let rest = rest.strip_prefix('@')?;
    let all = words(rest);
    let (first, word) = all.first()?;
    let mut found = Event {
        line,
        date: word.parse().ok()?,
        start: None,
        end: None,
        title: String::new(),
    };
    let mut end = first.end;
    let mut skip = 1;
    if let Some((columns, times)) = all.get(1) {
        let (start, until) = times.split_once('-').unwrap_or((times, ""));
        if let Some(start) = time(start) {
            found.start = Some(start);
            found.end = time(until);
            end = columns.end;
            skip = 2;
        }
    }
    found.title = all
        .iter()
        .skip(skip)
        .map(|(_, word)| word.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let span = Span {
        columns: depth..depth + 1 + end,
        style: Style::When,
    };
    Some((found, span))
}

fn heading_level(text: &str) -> Option<usize> {
    let level = text.chars().take_while(|c| *c == '=').count();
    let rest = &text[level..];
    (level > 0 && (rest.is_empty() || rest.starts_with(' '))).then_some(level)
}

fn heading(number: usize, line: &str, level: usize, today: Date, note: &mut Note) -> Vec<Span> {
    inline(line, number, &mut Vec::new(), note);
    let mut spans = whole(line, Style::Heading(level));
    let mut due = None;
    let mut title = Vec::new();
    for (columns, word) in words(line).into_iter().skip(1) {
        let lower = word.to_lowercase();
        match lower.strip_prefix("due:").filter(|value| !value.is_empty()) {
            Some(value) if due.is_none() => {
                let (found, style) = dated(value, today);
                due = found;
                spans.push(Span { columns, style });
            }
            _ => title.push(word),
        }
    }
    note.headings.push(Heading {
        line: number,
        level,
        title: title.join(" "),
        due,
    });
    spans
}

fn clock(number: usize, line: &str, note: &mut Note) -> Option<Vec<Span>> {
    let depth = indent(line);
    let rest: String = line.chars().skip(depth).collect();
    let body = rest.strip_prefix("clock:").filter(|_| depth > 0)?;
    let all = words(body);
    let [(day_columns, day), (range_columns, range)] = all.as_slice() else {
        return None;
    };
    let (start, end) = range.split_once('-').unwrap_or((range, ""));
    let found = Clock {
        line: number,
        date: day.parse().ok()?,
        start: time(start)?,
        end: if end.is_empty() {
            None
        } else {
            Some(time(end)?)
        },
    };
    let after = note.headings.last().map(|heading| heading.line);
    let owner = note
        .tasks
        .iter_mut()
        .rev()
        .take_while(|task| after.is_none_or(|after| task.line > after))
        .find(|task| task.depth < depth)?;
    owner.clocks.push(found);
    let key = depth + "clock:".len();
    Some(vec![
        Span {
            columns: depth..key,
            style: Style::Key,
        },
        Span {
            columns: key + day_columns.start..key + range_columns.end,
            style: Style::Field,
        },
    ])
}

fn thread(line: usize, text: &str, threads: &mut Vec<Thread>) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    match (text.starts_with(char::is_whitespace), threads.last_mut()) {
        (true, Some(last)) if trimmed == "resolved" => last.resolved = true,
        (true, Some(last)) => last.entries.push(trimmed.to_owned()),
        _ => {
            let (id, rest) = trimmed
                .split_once(char::is_whitespace)
                .unwrap_or((trimmed, ""));
            threads.push(Thread {
                id: id.to_owned(),
                line,
                entries: vec![rest.trim().to_owned()],
                resolved: false,
            });
        }
    }
}

fn whole(text: &str, style: Style) -> Vec<Span> {
    vec![Span {
        columns: 0..text.chars().count(),
        style,
    }]
}

fn body_line(number: usize, line: &str, today: Date, note: &mut Note) -> Vec<Span> {
    if line.trim_end() == "---" {
        return whole(line, Style::Separator);
    }
    if let Some(level) = heading_level(line) {
        return heading(number, line, level, today, note);
    }
    if let Some((found, spans)) = task(number, line, today, note) {
        note.tasks.push(found);
        return spans;
    }
    if let Some(spans) = clock(number, line, note) {
        return spans;
    }
    let mut spans = Vec::new();
    if let Some((found, span)) = event(number, line) {
        note.events.push(found);
        spans.push(span);
    } else {
        let depth = indent(line);
        if line.chars().skip(depth).take(2).eq("- ".chars()) {
            spans.push(Span {
                columns: depth..depth + 1,
                style: Style::Bullet,
            });
        }
    }
    inline(line, number, &mut spans, note);
    spans
}

pub fn parse(text: &str, today: Date) -> Note {
    let lines: Vec<&str> = text.lines().collect();
    let header = header_length(&lines);
    let mut note = Note::default();
    let mut comments = false;
    for (number, line) in lines.iter().enumerate() {
        let mut spans = if number < header {
            match header_field(line) {
                Some((key, value)) => {
                    note.header.push((key.to_owned(), value.to_owned()));
                    vec![Span {
                        columns: 0..key.chars().count() + 1,
                        style: Style::Key,
                    }]
                }
                None => whole(line, Style::Separator),
            }
        } else if comments {
            thread(number, line, &mut note.threads);
            whole(line, Style::Comment)
        } else if line.trim_end() == COMMENTS {
            comments = true;
            whole(line, Style::Separator)
        } else {
            body_line(number, line, today, &mut note)
        };
        spans.sort_by_key(|span| span.columns.start);
        note.spans.push(spans);
    }
    note
}

#[cfg(test)]
mod tests {
    use jiff::civil::{date, time};

    use super::*;

    const TODAY: Date = date(2026, 10, 19);
    const GARDEN: &str = "title: Garden plan
tags: home spring
status: active
---
= Garden plan

[ ] Buy seeds  due:fri  !high  #home
[x] Measure beds  done:2026-10-18
    [/] Order soil  due:2026-10-30  on:2026-10-21 14:00
[?] Feedback from [Ana]  every:2w
[-] Old idea

Met with Ana about the beds{c1}; see [Seed catalog].

== Log
@2026-10-19 10:00-11:00 Visit nursery
- bring the #seeds list

--- comments
c1 2026-10-19 14:02  Check the soil depth first
   2026-10-20 09:10  Done, 30cm is fine
   resolved
";

    fn garden() -> Note {
        parse(GARDEN, TODAY)
    }

    fn styles(note: &Note, line: usize) -> Vec<(Range<usize>, Style)> {
        note.spans[line]
            .iter()
            .map(|span| (span.columns.clone(), span.style))
            .collect()
    }

    #[test]
    fn the_header_holds_fields_until_its_rule() {
        let note = garden();

        assert_eq!(note.field("title"), Some("Garden plan"));
        assert_eq!(note.field("Tags"), Some("home spring"));
        assert_eq!(note.field("status"), Some("active"));
        assert_eq!(styles(&note, 0), [(0..6, Style::Key)]);
        assert_eq!(styles(&note, 3), [(0..3, Style::Separator)]);
        assert_eq!(parse("note: no rule\nmore text\n", TODAY).header, []);
    }

    fn heading_at(line: usize, level: usize, title: &str, due: Option<Date>) -> Heading {
        Heading {
            line,
            level,
            title: title.to_owned(),
            due,
        }
    }

    #[test]
    fn headings_have_levels() {
        let note = garden();

        assert_eq!(
            note.headings,
            [
                heading_at(4, 1, "Garden plan", None),
                heading_at(14, 2, "Log", None)
            ]
        );
        assert_eq!(styles(&note, 14), [(0..6, Style::Heading(2))]);
    }

    #[test]
    fn headings_with_a_due_date_are_milestones() {
        let note = parse(
            "== Beds ready  due:2026-11-01\n== Later  due:fri\n= Plain\n",
            TODAY,
        );

        assert_eq!(
            note.headings,
            [
                heading_at(0, 2, "Beds ready", Some(date(2026, 11, 1))),
                heading_at(1, 2, "Later", Some(date(2026, 10, 23))),
                heading_at(2, 1, "Plain", None)
            ]
        );
        assert_eq!(
            styles(&note, 0),
            [(0..29, Style::Heading(2)), (15..29, Style::Field)]
        );
        assert_eq!(
            styles(&note, 1),
            [(0..17, Style::Heading(2)), (10..17, Style::Loose)]
        );
    }

    #[test]
    fn clock_lines_belong_to_the_task_above_them() {
        let note = parse(
            "[/] Order soil\n    clock: 2026-10-19 14:02-15:30\n    [ ] Child\n    clock: 2026-10-20 23:10-\n== Next\n    clock: 2026-10-21 08:00-09:00\n",
            TODAY,
        );

        assert_eq!(
            note.tasks[0].clocks,
            [
                Clock {
                    line: 1,
                    date: date(2026, 10, 19),
                    start: time(14, 2, 0, 0),
                    end: Some(time(15, 30, 0, 0)),
                },
                Clock {
                    line: 3,
                    date: date(2026, 10, 20),
                    start: time(23, 10, 0, 0),
                    end: None,
                }
            ]
        );
        assert_eq!(note.tasks[1].clocks, []);
        assert_eq!(
            styles(&note, 1),
            [(4..10, Style::Key), (11..33, Style::Field)]
        );
        assert_eq!(styles(&note, 5), []);
    }

    #[test]
    fn clocks_count_minutes_across_midnight_and_while_running() {
        let clock = |start, end| Clock {
            line: 0,
            date: TODAY,
            start,
            end,
        };
        let now = TODAY.tomorrow().unwrap().at(0, 30, 0, 0);

        assert_eq!(
            clock(time(14, 2, 0, 0), Some(time(15, 30, 0, 0))).minutes(now),
            88
        );
        assert_eq!(
            clock(time(23, 10, 0, 0), Some(time(0, 20, 0, 0))).minutes(now),
            70
        );
        assert_eq!(clock(time(23, 10, 0, 0), None).minutes(now), 80);
    }

    #[test]
    fn tasks_read_their_state_fields_tags_and_links() {
        let note = garden();
        let [seeds, beds, soil, feedback, idea] = &note.tasks[..] else {
            panic!("{:?}", note.tasks);
        };

        assert_eq!(seeds.state, State::Todo);
        assert_eq!(seeds.title, "Buy seeds #home");
        assert_eq!(seeds.due, Some(date(2026, 10, 23)));
        assert_eq!(seeds.priority, Some(Priority::High));
        assert_eq!(seeds.tags, ["home"]);
        assert_eq!(beds.state, State::Done);
        assert_eq!(beds.done, Some(date(2026, 10, 18)));
        assert_eq!(soil.state, State::Doing);
        assert_eq!(soil.depth, 4);
        assert_eq!(soil.on, Some(date(2026, 10, 21)));
        assert_eq!(soil.at, Some(time(14, 0, 0, 0)));
        assert_eq!(soil.title, "Order soil");
        assert_eq!(feedback.state, State::Waiting);
        assert_eq!(feedback.links, ["Ana"]);
        assert_eq!(feedback.every, Some(Every::Weeks(2)));
        assert_eq!(idea.state, State::Dropped);
    }

    #[test]
    fn task_spans_mark_boxes_fields_and_loose_dates() {
        let note = garden();

        assert_eq!(
            styles(&note, 6),
            [
                (0..3, Style::Box(State::Todo)),
                (15..22, Style::Loose),
                (24..29, Style::Priority),
                (31..36, Style::Tag),
            ]
        );
        assert_eq!(
            styles(&note, 7),
            [
                (0..3, Style::Box(State::Done)),
                (3..33, Style::Finished),
                (18..33, Style::Field),
            ]
        );
    }

    #[test]
    fn events_have_a_date_times_and_a_title() {
        let note = garden();

        assert_eq!(
            note.events,
            [Event {
                line: 15,
                date: date(2026, 10, 19),
                start: Some(time(10, 0, 0, 0)),
                end: Some(time(11, 0, 0, 0)),
                title: "Visit nursery".to_owned(),
            }]
        );
        assert_eq!(styles(&note, 15), [(0..23, Style::When)]);
    }

    #[test]
    fn text_carries_links_tags_anchors_and_bullets() {
        let note = garden();

        assert_eq!(note.links, ["Ana", "Seed catalog"]);
        assert_eq!(note.tags, ["home", "seeds"]);
        assert_eq!(note.anchors, [(12, "c1".to_owned())]);
        assert_eq!(
            styles(&note, 12),
            [(27..31, Style::Anchor), (37..51, Style::Link)]
        );
        assert_eq!(
            styles(&note, 16),
            [(0..1, Style::Bullet), (12..18, Style::Tag)]
        );
    }

    #[test]
    fn comment_threads_sit_after_their_rule() {
        let note = garden();

        assert_eq!(
            note.threads,
            [Thread {
                id: "c1".to_owned(),
                line: 19,
                entries: vec![
                    "2026-10-19 14:02  Check the soil depth first".to_owned(),
                    "2026-10-20 09:10  Done, 30cm is fine".to_owned(),
                ],
                resolved: true,
            }]
        );
        assert_eq!(styles(&note, 18), [(0..12, Style::Separator)]);
        assert_eq!(styles(&note, 19)[0].1, Style::Comment);
    }

    #[test]
    fn states_cycle_from_todo_through_doing_to_done() {
        assert_eq!(State::Todo.cycled(), State::Doing);
        assert_eq!(State::Doing.cycled(), State::Done);
        assert_eq!(State::Done.cycled(), State::Todo);
        assert_eq!(State::from_mark('X'), Some(State::Done));
        assert_eq!(State::Waiting.mark(), '?');
        assert_eq!(State::from_mark('*'), None);
    }

    #[test]
    fn brackets_that_are_not_tasks_are_text() {
        let note = parse("[x]done\n[ab] text\n#notag\nsee#nope\n", TODAY);

        assert_eq!(note.tasks, []);
        assert_eq!(note.links, ["x", "ab"]);
        assert_eq!(note.tags, ["notag"]);
    }
}
