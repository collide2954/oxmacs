use jiff::ToSpan;
use jiff::civil::{Date, Weekday};

const WEEKDAYS: [(&str, Weekday); 7] = [
    ("monday", Weekday::Monday),
    ("tuesday", Weekday::Tuesday),
    ("wednesday", Weekday::Wednesday),
    ("thursday", Weekday::Thursday),
    ("friday", Weekday::Friday),
    ("saturday", Weekday::Saturday),
    ("sunday", Weekday::Sunday),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Every {
    Days(i32),
    Weeks(i32),
    Months(i32),
    Years(i32),
    Weekdays(u8),
}

fn weekday(word: &str) -> Option<Weekday> {
    (word.len() >= 3)
        .then(|| {
            WEEKDAYS
                .iter()
                .find(|(name, _)| name.starts_with(word))
                .map(|(_, day)| *day)
        })
        .flatten()
}

fn bit(day: Weekday) -> u8 {
    1 << day.to_monday_zero_offset()
}

fn amount(digits: &str) -> Option<i32> {
    if digits.is_empty() {
        Some(1)
    } else {
        digits.parse().ok().filter(|count| *count > 0)
    }
}

pub fn resolve(word: &str, today: Date) -> Option<Date> {
    let word = word.trim().to_lowercase();
    if let Ok(date) = word.parse::<Date>() {
        return Some(date);
    }
    match word.as_str() {
        "today" => return Some(today),
        "tomorrow" => return today.checked_add(1.day()).ok(),
        "yesterday" => return today.checked_sub(1.day()).ok(),
        _ => {}
    }
    if let Some(day) = weekday(&word) {
        let ahead =
            (day.to_monday_zero_offset() - today.weekday().to_monday_zero_offset()).rem_euclid(7);
        return today.checked_add(i64::from(ahead).days()).ok();
    }
    if let Some(sign) = word.chars().next().filter(|c| matches!(c, '+' | '-')) {
        let rest = &word[1..];
        let unit = rest.chars().last().filter(char::is_ascii_alphabetic);
        let digits = unit.map_or(rest, |_| &rest[..rest.len() - 1]);
        let count: i32 = digits.parse().ok()?;
        let count = if sign == '-' { -count } else { count };
        let span = match unit.unwrap_or('d') {
            'd' => count.days(),
            'w' => count.weeks(),
            'm' => count.months(),
            'y' => count.years(),
            _ => return None,
        };
        return today.checked_add(span).ok();
    }
    let (month, day) = word.split_once('-')?;
    let this_year = Date::new(today.year(), month.parse().ok()?, day.parse().ok()?).ok()?;
    if this_year >= today {
        Some(this_year)
    } else {
        Date::new(today.year() + 1, this_year.month(), this_year.day()).ok()
    }
}

pub fn parse_every(text: &str) -> Option<Every> {
    let text = text.trim().to_lowercase();
    match text.as_str() {
        "daily" | "day" => return Some(Every::Days(1)),
        "weekly" | "week" => return Some(Every::Weeks(1)),
        "monthly" | "month" => return Some(Every::Months(1)),
        "yearly" | "year" => return Some(Every::Years(1)),
        _ => {}
    }
    let unit = text.chars().last()?;
    let digits = &text[..text.len() - unit.len_utf8()];
    if digits.chars().all(|c| c.is_ascii_digit()) {
        let count = amount(digits)?;
        return match unit {
            'd' => Some(Every::Days(count)),
            'w' => Some(Every::Weeks(count)),
            'm' => Some(Every::Months(count)),
            'y' => Some(Every::Years(count)),
            _ => None,
        };
    }
    let mask = text
        .split(',')
        .map(|word| weekday(word.trim()).map(bit))
        .try_fold(0, |mask, day| day.map(|day| mask | day))?;
    (mask != 0).then_some(Every::Weekdays(mask))
}

impl Every {
    pub fn after(self, date: Date) -> Option<Date> {
        let span = match self {
            Self::Days(count) => count.days(),
            Self::Weeks(count) => count.weeks(),
            Self::Months(count) => count.months(),
            Self::Years(count) => count.years(),
            Self::Weekdays(mask) => {
                let next = (1..=7).find(|ahead| {
                    date.checked_add(ahead.days())
                        .is_ok_and(|day| mask & bit(day.weekday()) != 0)
                })?;
                next.days()
            }
        };
        date.checked_add(span).ok()
    }

    pub fn next(self, from: Date, today: Date) -> Option<Date> {
        let mut date = self.after(from)?;
        while date <= today {
            date = self.after(date)?;
        }
        Some(date)
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    const TODAY: Date = date(2026, 10, 19);

    fn on(word: &str) -> Option<Date> {
        resolve(word, TODAY)
    }

    #[test]
    fn exact_dates_stay_as_written() {
        assert_eq!(on("2026-10-24"), Some(date(2026, 10, 24)));
        assert_eq!(on("2026-13-01"), None);
    }

    #[test]
    fn words_count_from_today() {
        assert_eq!(on("today"), Some(TODAY));
        assert_eq!(on("Tomorrow"), Some(date(2026, 10, 20)));
        assert_eq!(on("yesterday"), Some(date(2026, 10, 18)));
        assert_eq!(on("someday"), None);
    }

    #[test]
    fn weekdays_mean_the_next_one_from_today_on() {
        assert_eq!(on("fri"), Some(date(2026, 10, 23)));
        assert_eq!(on("friday"), Some(date(2026, 10, 23)));
        assert_eq!(on("mon"), Some(TODAY));
        assert_eq!(on("Sun"), Some(date(2026, 10, 25)));
        assert_eq!(on("fr"), None);
    }

    #[test]
    fn offsets_step_by_days_weeks_months_and_years() {
        assert_eq!(on("+3d"), Some(date(2026, 10, 22)));
        assert_eq!(on("+3"), Some(date(2026, 10, 22)));
        assert_eq!(on("+2w"), Some(date(2026, 11, 2)));
        assert_eq!(on("+1m"), Some(date(2026, 11, 19)));
        assert_eq!(on("+1y"), Some(date(2027, 10, 19)));
        assert_eq!(on("-1d"), Some(date(2026, 10, 18)));
        assert_eq!(on("+x"), None);
    }

    #[test]
    fn month_and_day_pick_the_next_such_date() {
        assert_eq!(on("12-25"), Some(date(2026, 12, 25)));
        assert_eq!(on("01-02"), Some(date(2027, 1, 2)));
        assert_eq!(on("02-30"), None);
    }

    #[test]
    fn repeats_are_read_from_short_words() {
        assert_eq!(parse_every("3d"), Some(Every::Days(3)));
        assert_eq!(parse_every("week"), Some(Every::Weeks(1)));
        assert_eq!(parse_every("2w"), Some(Every::Weeks(2)));
        assert_eq!(parse_every("month"), Some(Every::Months(1)));
        assert_eq!(parse_every("y"), Some(Every::Years(1)));
        assert_eq!(
            parse_every("mon,thu"),
            Some(Every::Weekdays(
                bit(Weekday::Monday) | bit(Weekday::Thursday)
            ))
        );
        assert_eq!(parse_every("mon,xyz"), None);
        assert_eq!(parse_every("0d"), None);
        assert_eq!(parse_every("x"), None);
    }

    #[test]
    fn repeats_move_past_today() {
        assert_eq!(
            Every::Days(3).next(date(2026, 10, 21), TODAY),
            Some(date(2026, 10, 24))
        );
        assert_eq!(
            Every::Days(3).next(date(2026, 10, 10), TODAY),
            Some(date(2026, 10, 22))
        );
        let mondays_and_thursdays = parse_every("mon,thu").unwrap();
        assert_eq!(
            mondays_and_thursdays.next(TODAY, TODAY),
            Some(date(2026, 10, 22))
        );
        assert_eq!(
            Every::Months(1).after(date(2026, 1, 31)),
            Some(date(2026, 2, 28))
        );
    }
}
