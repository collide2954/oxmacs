use std::sync::LazyLock;

use jiff::civil::Date;
use regex::Regex;

use crate::atlas::date;
use crate::atlas::note::{self, State, Style};

static DONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s+done:\S+").unwrap_or_else(|_| unreachable!("a valid pattern"))
});
static DATED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(on|due):(\d{4}-\d{2}-\d{2})")
        .unwrap_or_else(|_| unreachable!("a valid pattern"))
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cycled {
    pub text: String,
    pub state: State,
    pub repeated: Option<Option<Date>>,
}

fn marked(text: &str, depth: usize, state: State) -> String {
    text.chars()
        .enumerate()
        .map(|(column, c)| if column == depth + 1 { state.mark() } else { c })
        .collect()
}

pub fn exact(text: &str, today: Date) -> Option<String> {
    let parsed = note::parse(text, today);
    let spans = parsed.spans.first()?;
    let chars: Vec<char> = text.chars().collect();
    let mut changed = chars.clone();
    let mut any = false;
    for span in spans.iter().rev().filter(|span| span.style == Style::Loose) {
        let word: String = chars[span.columns.clone()].iter().collect();
        let Some((key, value)) = word.split_once(':') else {
            continue;
        };
        if key.eq_ignore_ascii_case("every") {
            continue;
        }
        if let Some(date) = date::resolve(value, today) {
            let replacement: Vec<char> = format!("{key}:{date}").chars().collect();
            changed.splice(span.columns.clone(), replacement);
            any = true;
        }
    }
    any.then(|| changed.into_iter().collect())
}

pub fn cycle(text: &str, today: Date) -> Option<Cycled> {
    let parsed = note::parse(text, today);
    let task = parsed.tasks.first()?;
    let next = task.state.cycled();
    if next == State::Done
        && let Some(every) = task.every
    {
        let mut first = None;
        let moved = DATED.replace_all(text, |captures: &regex::Captures<'_>| {
            let next = captures[2]
                .parse::<Date>()
                .ok()
                .and_then(|date| every.next(date, today));
            match next {
                Some(next) => {
                    first.get_or_insert(next);
                    format!("{}:{next}", &captures[1])
                }
                None => captures[0].to_owned(),
            }
        });
        let changed = marked(&moved, task.depth, State::Todo);
        let log = format!("\n{}- done {today}", " ".repeat(task.depth + 4));
        return Some(Cycled {
            text: format!("{changed}{log}"),
            state: State::Todo,
            repeated: Some(first),
        });
    }
    let mut changed = marked(text, task.depth, next);
    if task.state == State::Done {
        changed = DONE.replace_all(&changed, "").into_owned();
    }
    if next == State::Done && task.done.is_none() {
        changed = format!("{}  done:{today}", changed.trim_end());
    }
    Some(Cycled {
        text: changed,
        state: next,
        repeated: None,
    })
}

pub fn set_field(text: &str, key: &str, value: Date) -> String {
    let pattern = Regex::new(&format!(r"\b{}:\S+", regex::escape(key)));
    match pattern {
        Ok(pattern) if pattern.is_match(text) => pattern
            .replace(text, format!("{key}:{value}").as_str())
            .into_owned(),
        _ => format!("{}  {key}:{value}", text.trim_end()),
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    const TODAY: Date = date(2026, 10, 19);

    fn cycled(text: &str) -> String {
        cycle(text, TODAY).unwrap().text
    }

    #[test]
    fn tasks_cycle_and_stamp_their_completion() {
        assert_eq!(cycled("[ ] Buy seeds  #home"), "[/] Buy seeds  #home");
        assert_eq!(
            cycled("[/] Buy seeds  #home"),
            "[x] Buy seeds  #home  done:2026-10-19"
        );
        assert_eq!(
            cycled("[x] Buy seeds  #home  done:2026-10-19"),
            "[ ] Buy seeds  #home"
        );
        assert_eq!(cycle("plain text", TODAY), None);
    }

    #[test]
    fn repeating_tasks_move_on_and_log() {
        let result = cycle("  [/] Water plants  on:2026-10-21  every:3d", TODAY).unwrap();

        assert_eq!(
            result.text,
            "  [ ] Water plants  on:2026-10-24  every:3d\n      - done 2026-10-19"
        );
        assert_eq!(result.state, State::Todo);
        assert_eq!(result.repeated, Some(Some(date(2026, 10, 24))));
    }

    #[test]
    fn loose_dates_become_exact() {
        assert_eq!(
            exact("[ ] Call  due:fri on:tomorrow every:week", TODAY).as_deref(),
            Some("[ ] Call  due:2026-10-23 on:2026-10-20 every:week")
        );
        assert_eq!(exact("[ ] Call  due:2026-10-23", TODAY), None);
    }

    #[test]
    fn fields_are_replaced_or_added() {
        assert_eq!(
            set_field("[ ] Call  due:2026-10-23", "due", date(2026, 10, 30)),
            "[ ] Call  due:2026-10-30"
        );
        assert_eq!(
            set_field("[ ] Call #ana", "on", date(2026, 10, 21)),
            "[ ] Call #ana  on:2026-10-21"
        );
        assert_eq!(
            set_field("[ ] Call  done:2026-10-01", "on", date(2026, 10, 21)),
            "[ ] Call  done:2026-10-01  on:2026-10-21"
        );
    }
}
