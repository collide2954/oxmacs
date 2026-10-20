use jiff::civil::Date;
use jiff::tz::TimeZone;

use crate::protocol::network::{Chat, Kind, Line, LineEvent, Target};
use crate::protocol::nick_color;
use oxmacs_core::editor::{Page, fill};
use oxmacs_core::theme::Highlight;

const TIME: usize = 5;
const MIN_NICK: usize = 4;
const MAX_NICK: usize = 12;
const MIN_TEXT: usize = 10;
const BAR: &str = " │ ";

fn nick_style(nick: &str, own: bool) -> Highlight {
    if own {
        Highlight::Heading
    } else {
        Highlight::Nick(nick_color(nick))
    }
}

fn shorten(nick: &str, width: usize) -> String {
    if nick.chars().count() <= width {
        return nick.to_owned();
    }
    let mut short: String = nick.chars().take(width.saturating_sub(1)).collect();
    short.push('…');
    short
}

fn separator(date: Date) -> String {
    format!("── {} ──", date.strftime("%A %-d %B"))
}

fn fold(lines: &[Line]) -> String {
    let mut joined = Vec::new();
    let mut left = Vec::new();
    let mut quit = Vec::new();
    let mut renamed = Vec::new();
    for line in lines {
        match &line.event {
            LineEvent::Joined(nick) => joined.push(nick.as_str()),
            LineEvent::Left { nick, .. } => left.push(nick.as_str()),
            LineEvent::Quit { nick, .. } => quit.push(nick.as_str()),
            LineEvent::Renamed { old, new } => renamed.push(format!("{old} is now {new}")),
            _ => {}
        }
    }
    let mut parts = Vec::new();
    for (names, arrow, verb) in [
        (joined, "→", "joined"),
        (left, "←", "left"),
        (quit, "←", "quit"),
    ] {
        if !names.is_empty() {
            parts.push(format!("{arrow} {} {verb}", names.join(", ")));
        }
    }
    parts.extend(renamed);
    parts.join(" · ")
}

struct Row<'a> {
    time: String,
    mark: (&'a str, Option<Highlight>),
    text: String,
    style: Option<Highlight>,
    background: Option<Highlight>,
}

struct Columns {
    nick: usize,
    text: usize,
}

impl Columns {
    fn emit(&self, page: &mut Page, row: &Row<'_>) {
        for (index, part) in fill(&row.text, self.text).iter().enumerate() {
            let first = index == 0;
            let time = if first {
                row.time.clone()
            } else {
                " ".repeat(TIME)
            };
            let (mark, mark_style) = if first { row.mark } else { ("", None) };
            let nick = format!("{:>width$}", shorten(mark, self.nick), width = self.nick);
            page.line(&[
                (&time, first.then_some(Highlight::Comment)),
                (" ", None),
                (&nick, mark_style),
                (BAR, Some(Highlight::Comment)),
                (part, row.style),
            ]);
            page.set_background(row.background);
        }
    }
}

fn row(line: &Line, time: String) -> Row<'_> {
    let note = |text: String| Row {
        time: time.clone(),
        mark: ("*", Some(Highlight::Comment)),
        text,
        style: Some(Highlight::Comment),
        background: None,
    };
    match &line.event {
        LineEvent::Chat {
            nick,
            text,
            kind: Kind::Action,
            own,
            mention,
        } => Row {
            time,
            mark: ("*", Some(Highlight::Comment)),
            text: format!("{nick} {text}"),
            style: Some(nick_style(nick, *own)),
            background: mention.then_some(Highlight::Marked),
        },
        LineEvent::Chat {
            nick,
            text,
            own,
            mention,
            ..
        } => Row {
            time,
            mark: (nick, Some(nick_style(nick, *own))),
            text: text.clone(),
            style: None,
            background: mention.then_some(Highlight::Marked),
        },
        LineEvent::Kicked { nick, by, reason } => note(match reason {
            Some(reason) => format!("{by} kicked {nick} ({reason})"),
            None => format!("{by} kicked {nick}"),
        }),
        LineEvent::Topic { nick, text } => note(format!("{nick} set the topic: {text}")),
        LineEvent::Mode { nick, change } => note(format!("{nick} sets mode {change}")),
        LineEvent::Info { text, error: true } => Row {
            time,
            mark: ("!", Some(Highlight::Error)),
            text: text.clone(),
            style: Some(Highlight::Error),
            background: None,
        },
        LineEvent::Info { text, .. } => Row {
            time,
            mark: ("*", Some(Highlight::Comment)),
            text: text.clone(),
            style: None,
            background: None,
        },
        LineEvent::Joined(_)
        | LineEvent::Left { .. }
        | LineEvent::Quit { .. }
        | LineEvent::Renamed { .. } => Row {
            mark: ("", None),
            ..note(fold(std::slice::from_ref(line)))
        },
    }
}

pub(crate) fn page(chat: &Chat, width: usize, zone: &TimeZone) -> Page {
    let mut page = Page::default();
    if let (Target::Channel(_), Some(topic)) = (&chat.target, &chat.topic) {
        for line in fill(&format!("Topic: {topic}"), width.max(MIN_TEXT)) {
            page.line(&[(&line, Some(Highlight::Comment))]);
        }
        page.line(&[]);
    }
    let nick = chat
        .lines
        .iter()
        .filter_map(|line| match &line.event {
            LineEvent::Chat { nick, .. } => Some(nick.chars().count()),
            _ => None,
        })
        .max()
        .unwrap_or(0)
        .clamp(MIN_NICK, MAX_NICK);
    let columns = Columns {
        nick,
        text: width
            .saturating_sub(TIME + 1 + nick + BAR.chars().count())
            .max(MIN_TEXT),
    };
    let mut day = None;
    let mut index = 0;
    while let Some(line) = chat.lines.get(index) {
        let zoned = line.time.to_zoned(zone.clone());
        let date = zoned.date();
        if day != Some(date) {
            day = Some(date);
            page.line(&[(&separator(date), Some(Highlight::Comment))]);
        }
        let time = zoned.strftime("%H:%M").to_string();
        let end = if line.event.is_membership() {
            chat.lines[index..]
                .iter()
                .position(|next| {
                    !next.event.is_membership() || next.time.to_zoned(zone.clone()).date() != date
                })
                .map_or(chat.lines.len(), |offset| index + offset)
        } else {
            index + 1
        };
        let mut shown = row(line, time);
        if end > index + 1 {
            shown.text = fold(&chat.lines[index..end]);
        }
        columns.emit(&mut page, &shown);
        index = end;
    }
    page
}
#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use jiff::tz::TimeZone;
    use oxmacs_core::display::LineStyle;

    use super::*;
    use crate::protocol::message::Message;
    use crate::protocol::network::{Network, Target};
    use crate::protocol::session::{Identity, Session};

    fn session() -> Session {
        let mut session = Session::new(Identity {
            nick: "oxtest".to_owned(),
            username: "oxtest".to_owned(),
            realname: "oxmacs".to_owned(),
            password: None,
            sasl: None,
        });
        session.receive(":srv 001 oxtest :Welcome");
        session.take_events();
        session
    }

    fn chat(lines: &[(&str, &str)]) -> (Network, Session) {
        let session = session();
        let mut network = Network::new("libera");
        network.apply(
            &session,
            &Message::parse(":oxtest!o@h JOIN #rust").unwrap(),
            "2026-10-19T08:00:00Z".parse().unwrap(),
        );
        for (time, line) in lines {
            let now: Timestamp = format!("2026-10-{time}Z").parse().unwrap();
            network.apply(&session, &Message::parse(line).unwrap(), now);
        }
        (network, session)
    }

    fn shown(lines: &[(&str, &str)], width: usize) -> (Vec<String>, Vec<LineStyle>) {
        let (network, _) = chat(lines);
        let index = network.find(&Target::Channel("#rust".to_owned())).unwrap();
        let (text, styles) = page(&network.chats()[index], width, &TimeZone::UTC).into_parts();
        (text.lines().map(str::to_owned).collect(), styles)
    }

    #[test]
    fn messages_line_up_under_a_nick_column() {
        let (lines, _) = shown(
            &[
                ("19T12:00:00", ":alice!a@h PRIVMSG #rust :hello"),
                ("19T12:01:00", ":bob!b@h PRIVMSG #rust :hi"),
            ],
            80,
        );

        assert_eq!(
            lines,
            [
                "── Monday 19 October ──",
                "12:00 alice │ hello",
                "12:01   bob │ hi",
            ]
        );
    }

    #[test]
    fn long_messages_wrap_under_their_text() {
        let (lines, _) = shown(
            &[(
                "19T12:00:00",
                ":alice!a@h PRIVMSG #rust :one two three four five six",
            )],
            28,
        );

        assert_eq!(
            lines[1..],
            [
                "12:00 alice │ one two three".to_owned(),
                format!("{}│ four five six", " ".repeat(12)),
            ]
        );
    }

    #[test]
    fn joins_parts_and_quits_fold_into_one_line() {
        let (lines, _) = shown(
            &[
                ("19T12:00:00", ":bob!b@h JOIN #rust"),
                ("19T12:00:01", ":carol!c@h JOIN #rust"),
                ("19T12:00:02", ":dave!d@h PART #rust :bye"),
                ("19T12:00:03", ":carol!c@h NICK caroline"),
                ("19T12:05:00", ":alice!a@h PRIVMSG #rust :hey"),
            ],
            80,
        );

        assert_eq!(
            lines[1..],
            [
                format!(
                    "12:00{}│ → bob, carol joined · ← dave left · carol is now caroline",
                    " ".repeat(7)
                ),
                "12:05 alice │ hey".to_owned(),
            ]
        );
    }

    #[test]
    fn a_new_day_gets_a_separator() {
        let (lines, _) = shown(
            &[
                ("19T23:59:00", ":alice!a@h PRIVMSG #rust :late"),
                ("20T00:01:00", ":alice!a@h PRIVMSG #rust :early"),
            ],
            80,
        );

        assert_eq!(
            lines,
            [
                "── Monday 19 October ──",
                "23:59 alice │ late",
                "── Tuesday 20 October ──",
                "00:01 alice │ early",
            ]
        );
    }

    #[test]
    fn actions_notices_and_server_lines_have_their_own_marks() {
        let (lines, _) = shown(
            &[
                (
                    "19T12:00:00",
                    ":alice!a@h PRIVMSG #rust :\u{1}ACTION waves\u{1}",
                ),
                ("19T12:00:01", ":alice!a@h NOTICE #rust :psst"),
                ("19T12:00:02", ":alice!a@h TOPIC #rust :New topic"),
                (
                    "19T12:00:03",
                    ":srv 404 oxtest #rust :Cannot send to channel",
                ),
            ],
            80,
        );

        assert_eq!(
            lines,
            [
                "Topic: New topic",
                "",
                "── Monday 19 October ──",
                "12:00     * │ alice waves",
                "12:00 alice │ psst",
                "12:00     * │ alice set the topic: New topic",
                "12:00     ! │ Cannot send to channel",
            ]
        );
    }

    #[test]
    fn long_nicks_are_shortened_to_the_column() {
        let (lines, _) = shown(
            &[(
                "19T12:00:00",
                ":averyveryverylongnick!a@h PRIVMSG #rust :hi",
            )],
            80,
        );

        assert_eq!(lines[1], "12:00 averyveryve… │ hi");
    }

    #[test]
    fn colors_mark_times_nicks_and_mentions() {
        let (_, page) = shown(
            &[
                ("19T12:00:00", ":alice!a@h PRIVMSG #rust :oxtest: ping"),
                ("19T12:00:01", ":oxtest!o@h PRIVMSG #rust :pong"),
            ],
            80,
        );

        let mention = &page[1];
        assert_eq!(mention.background, Some(Highlight::Marked));
        assert_eq!(mention.spans[0], (0..5, Highlight::Comment));
        assert_eq!(
            mention.spans[1],
            (6..12, Highlight::Nick(nick_color("alice")))
        );
        assert_eq!(page[2].background, None);
        assert_eq!(page[2].spans[1].1, Highlight::Heading);
    }
}
