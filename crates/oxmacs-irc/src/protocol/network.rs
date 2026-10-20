use jiff::Timestamp;

use crate::protocol::message::Message;
use crate::protocol::plain;
use crate::protocol::session::Session;

const CTCP: char = '\u{1}';

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Server,
    Channel(String),
    Query(String),
}

impl Target {
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Server => None,
            Self::Channel(name) | Self::Query(name) => Some(name),
        }
    }

    pub fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Server, Self::Server) => true,
            (Self::Channel(a), Self::Channel(b)) | (Self::Query(a), Self::Query(b)) => same(a, b),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub nick: String,
    pub prefixes: String,
    pub away: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Message,
    Action,
    Notice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineEvent {
    Chat {
        nick: String,
        text: String,
        kind: Kind,
        own: bool,
        mention: bool,
    },
    Joined(String),
    Left {
        nick: String,
        reason: Option<String>,
    },
    Quit {
        nick: String,
        reason: Option<String>,
    },
    Kicked {
        nick: String,
        by: String,
        reason: Option<String>,
    },
    Renamed {
        old: String,
        new: String,
    },
    Topic {
        nick: String,
        text: String,
    },
    Mode {
        nick: String,
        change: String,
    },
    Info {
        text: String,
        error: bool,
    },
}

impl LineEvent {
    pub fn is_membership(&self) -> bool {
        matches!(
            self,
            Self::Joined(_) | Self::Left { .. } | Self::Quit { .. } | Self::Renamed { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub time: Timestamp,
    pub event: LineEvent,
    pub history: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    Created(usize),
    Line(usize),
    Members(usize),
    Topic(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chat {
    pub target: Target,
    pub topic: Option<String>,
    pub lines: Vec<Line>,
    pub unread: usize,
    pub mentions: usize,
    pub joined: bool,
    members: Vec<Member>,
    naming: bool,
}

impl Chat {
    fn new(target: Target) -> Self {
        Self {
            target,
            topic: None,
            lines: Vec::new(),
            unread: 0,
            mentions: 0,
            joined: false,
            members: Vec::new(),
            naming: false,
        }
    }

    pub fn members(&self, session: &Session) -> Vec<Member> {
        let mut members = self.members.clone();
        members.sort_by_key(|member| {
            let rank = member
                .prefixes
                .chars()
                .next()
                .and_then(|symbol| session.prefix_rank(symbol))
                .unwrap_or(usize::MAX);
            (rank, member.nick.to_ascii_lowercase())
        });
        members
    }

    fn member_mut(&mut self, nick: &str) -> Option<&mut Member> {
        self.members
            .iter_mut()
            .find(|member| same(&member.nick, nick))
    }

    fn remove(&mut self, nick: &str) -> bool {
        let before = self.members.len();
        self.members.retain(|member| !same(&member.nick, nick));
        self.members.len() != before
    }

    fn add(&mut self, nick: &str, prefixes: &str) {
        if let Some(member) = self.member_mut(nick) {
            prefixes.clone_into(&mut member.prefixes);
            return;
        }
        self.members.push(Member {
            nick: nick.to_owned(),
            prefixes: prefixes.to_owned(),
            away: false,
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    name: String,
    chats: Vec<Chat>,
    batches: Vec<(String, String)>,
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn nick_char(c: char) -> bool {
    c.is_alphanumeric() || "_-[]\\`^{}|".contains(c)
}

pub fn mentions(text: &str, nick: &str) -> bool {
    if nick.is_empty() {
        return false;
    }
    let text = text.to_lowercase();
    let nick = nick.to_lowercase();
    text.match_indices(&nick).any(|(at, found)| {
        let before = text[..at].chars().next_back();
        let after = text[at + found.len()..].chars().next();
        !before.is_some_and(nick_char) && !after.is_some_and(nick_char)
    })
}

fn reason(message: &Message, index: usize) -> Option<String> {
    message
        .params
        .get(index)
        .filter(|reason| !reason.is_empty())
        .cloned()
}

impl Network {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            chats: vec![Chat::new(Target::Server)],
            batches: Vec::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn chats(&self) -> &[Chat] {
        &self.chats
    }

    pub fn find(&self, target: &Target) -> Option<usize> {
        self.chats.iter().position(|chat| chat.target.same(target))
    }

    pub fn note(&mut self, index: usize, text: &str, error: bool, time: Timestamp) -> Update {
        let line = Line {
            time,
            event: LineEvent::Info {
                text: text.to_owned(),
                error,
            },
            history: false,
        };
        if let Some(chat) = self.chats.get_mut(index) {
            chat.lines.push(line);
        }
        Update::Line(index)
    }

    pub fn disconnected(&mut self) {
        for chat in &mut self.chats {
            chat.joined = false;
            chat.members.clear();
        }
    }

    pub fn mark_read(&mut self, index: usize) {
        if let Some(chat) = self.chats.get_mut(index) {
            chat.unread = 0;
            chat.mentions = 0;
        }
    }

    pub fn ensure(&mut self, target: Target, updates: &mut Vec<Update>) -> usize {
        if let Some(index) = self.find(&target) {
            return index;
        }
        self.chats.push(Chat::new(target));
        let index = self.chats.len() - 1;
        updates.push(Update::Created(index));
        index
    }

    fn push(&mut self, index: usize, line: Line, updates: &mut Vec<Update>) {
        if let Some(chat) = self.chats.get_mut(index) {
            chat.lines.push(line);
            updates.push(Update::Line(index));
        }
    }

    pub fn apply(&mut self, session: &Session, message: &Message, now: Timestamp) -> Vec<Update> {
        let stamp = Stamp {
            time: message
                .tag("time")
                .and_then(|time| time.parse().ok())
                .unwrap_or(now),
            history: message.tag("batch").is_some_and(|id| self.in_history(id)),
        };
        let mut updates = Vec::new();
        match message.command.as_str() {
            "BATCH" => self.batch(message),
            "JOIN" => self.join(session, message, stamp, &mut updates),
            "PART" | "KICK" => self.leave(session, message, stamp, &mut updates),
            "QUIT" => self.quit(message, stamp, &mut updates),
            "NICK" => self.rename(message, stamp, &mut updates),
            "PRIVMSG" | "NOTICE" => self.chat_message(session, message, stamp, &mut updates),
            "TOPIC" | "332" => self.topic(message, stamp, &mut updates),
            "353" | "366" => self.names(session, message, &mut updates),
            "MODE" => self.mode(session, message, stamp, &mut updates),
            "AWAY" => self.away(message, &mut updates),
            "INVITE" | "ERROR" | "FAIL" | "WARN" | "NOTE" => {
                self.server_notice(message, stamp, &mut updates);
            }
            "004" | "005" | "333" => {}
            command if command.len() == 3 && command.chars().all(|c| c.is_ascii_digit()) => {
                self.numeric(message, stamp, &mut updates);
            }
            _ => {}
        }
        updates
    }

    fn in_history(&self, id: &str) -> bool {
        self.batches
            .iter()
            .any(|(batch, kind)| batch == id && kind.ends_with("chathistory"))
    }

    fn channel(&self, name: &str) -> Option<usize> {
        self.find(&Target::Channel(name.to_owned()))
    }

    fn batch(&mut self, message: &Message) {
        let id = message.param(0);
        if let Some(id) = id.strip_prefix('+') {
            self.batches
                .push((id.to_owned(), message.param(1).to_owned()));
        } else if let Some(id) = id.strip_prefix('-') {
            self.batches.retain(|(batch, _)| batch != id);
        }
    }

    fn join(
        &mut self,
        session: &Session,
        message: &Message,
        stamp: Stamp,
        updates: &mut Vec<Update>,
    ) {
        let nick = message.nick().unwrap_or_default();
        let index = self.ensure(Target::Channel(message.param(0).to_owned()), updates);
        let chat = &mut self.chats[index];
        if same(nick, session.nick()) {
            chat.joined = true;
            chat.members.clear();
            return;
        }
        chat.add(nick, "");
        self.push(
            index,
            stamp.line(LineEvent::Joined(nick.to_owned())),
            updates,
        );
    }

    fn leave(
        &mut self,
        session: &Session,
        message: &Message,
        stamp: Stamp,
        updates: &mut Vec<Update>,
    ) {
        let Some(index) = self.channel(message.param(0)) else {
            return;
        };
        let by = message.nick().unwrap_or_default();
        let kicked = message.command == "KICK";
        let nick = if kicked { message.param(1) } else { by };
        let chat = &mut self.chats[index];
        chat.remove(nick);
        if same(nick, session.nick()) {
            chat.joined = false;
            chat.members.clear();
        }
        let event = if kicked {
            LineEvent::Kicked {
                nick: nick.to_owned(),
                by: by.to_owned(),
                reason: reason(message, 2),
            }
        } else {
            LineEvent::Left {
                nick: nick.to_owned(),
                reason: reason(message, 1),
            }
        };
        self.push(index, stamp.line(event), updates);
    }

    fn quit(&mut self, message: &Message, stamp: Stamp, updates: &mut Vec<Update>) {
        let nick = message.nick().unwrap_or_default();
        for index in 0..self.chats.len() {
            let chat = &mut self.chats[index];
            let query = chat.target.same(&Target::Query(nick.to_owned()));
            if chat.remove(nick) || query {
                let event = LineEvent::Quit {
                    nick: nick.to_owned(),
                    reason: reason(message, 0),
                };
                self.push(index, stamp.line(event), updates);
            }
        }
    }

    fn rename(&mut self, message: &Message, stamp: Stamp, updates: &mut Vec<Update>) {
        let old = message.nick().unwrap_or_default();
        let new = message.param(0);
        for index in 0..self.chats.len() {
            let chat = &mut self.chats[index];
            let query = chat.target.same(&Target::Query(old.to_owned()));
            if query {
                chat.target = Target::Query(new.to_owned());
            }
            let member = chat.member_mut(old);
            let renamed = member.is_some();
            if let Some(member) = member {
                new.clone_into(&mut member.nick);
            }
            if renamed || query {
                let event = LineEvent::Renamed {
                    old: old.to_owned(),
                    new: new.to_owned(),
                };
                self.push(index, stamp.line(event), updates);
            }
        }
    }

    fn topic(&mut self, message: &Message, stamp: Stamp, updates: &mut Vec<Update>) {
        let reply = message.command == "332";
        let (channel, text) = if reply {
            (message.param(1), message.param(2))
        } else {
            (message.param(0), message.param(1))
        };
        let Some(index) = self.channel(channel) else {
            return;
        };
        self.chats[index].topic = Some(plain(text));
        if !reply {
            let event = LineEvent::Topic {
                nick: message.nick().unwrap_or_default().to_owned(),
                text: text.to_owned(),
            };
            self.push(index, stamp.line(event), updates);
        }
        updates.push(Update::Topic(index));
    }

    fn names(&mut self, session: &Session, message: &Message, updates: &mut Vec<Update>) {
        if message.command == "366" {
            if let Some(index) = self.channel(message.param(1)) {
                self.chats[index].naming = false;
                updates.push(Update::Members(index));
            }
            return;
        }
        let Some(index) = self.channel(message.param(2)) else {
            return;
        };
        let chat = &mut self.chats[index];
        if !chat.naming {
            chat.members.clear();
            chat.naming = true;
        }
        for name in message.param(3).split_whitespace() {
            let split = name
                .find(|c| !session.prefix_symbols().contains(c))
                .unwrap_or(name.len());
            let (prefixes, rest) = name.split_at(split);
            let nick = rest.split('!').next().unwrap_or(rest);
            chat.add(nick, prefixes);
        }
    }

    fn away(&mut self, message: &Message, updates: &mut Vec<Update>) {
        let nick = message.nick().unwrap_or_default();
        let away = !message.params.is_empty();
        for index in 0..self.chats.len() {
            if let Some(member) = self.chats[index].member_mut(nick) {
                member.away = away;
                updates.push(Update::Members(index));
            }
        }
    }

    fn server_notice(&mut self, message: &Message, stamp: Stamp, updates: &mut Vec<Update>) {
        let text = if message.command == "INVITE" {
            format!(
                "{} invited you to {}",
                message.nick().unwrap_or_default(),
                message.param(1)
            )
        } else {
            message.params.last().cloned().unwrap_or_default()
        };
        let error = matches!(message.command.as_str(), "ERROR" | "FAIL");
        self.push(0, stamp.line(LineEvent::Info { text, error }), updates);
    }

    fn chat_message(
        &mut self,
        session: &Session,
        message: &Message,
        stamp: Stamp,
        updates: &mut Vec<Update>,
    ) {
        let nick = message.nick().unwrap_or_default();
        let own = same(nick, session.nick());
        let target = message.param(0);
        let mut text = message.param(1).to_owned();
        let mut kind = if message.command == "NOTICE" {
            Kind::Notice
        } else {
            Kind::Message
        };
        if let Some(inner) = text
            .strip_prefix(CTCP)
            .map(|inner| inner.trim_end_matches(CTCP).to_owned())
        {
            if let Some(action) = inner.strip_prefix("ACTION") {
                kind = Kind::Action;
                action.trim_start().clone_into(&mut text);
            } else {
                let event = LineEvent::Info {
                    text: format!("{nick} sent CTCP {inner}"),
                    error: false,
                };
                self.push(0, stamp.line(event), updates);
                return;
            }
        }
        let from_server = !message
            .source
            .as_deref()
            .is_some_and(|source| source.contains('!'));
        let index = if session.is_channel(target) {
            self.ensure(Target::Channel(target.to_owned()), updates)
        } else if own {
            self.ensure(Target::Query(target.to_owned()), updates)
        } else if from_server {
            0
        } else if kind == Kind::Notice {
            self.notice_home(nick, &text)
        } else {
            self.ensure(Target::Query(nick.to_owned()), updates)
        };
        let mention = !own && mentions(&text, session.nick());
        let event = if index == 0 && from_server {
            LineEvent::Info { text, error: false }
        } else {
            LineEvent::Chat {
                nick: nick.to_owned(),
                text,
                kind,
                own,
                mention,
            }
        };
        let private = matches!(self.chats[index].target, Target::Query(_));
        if !own && !stamp.history && index != 0 && (kind != Kind::Notice || private) {
            let chat = &mut self.chats[index];
            chat.unread += 1;
            chat.mentions += usize::from(mention);
        }
        self.push(index, stamp.line(event), updates);
    }

    fn notice_home(&self, nick: &str, text: &str) -> usize {
        let query = self.find(&Target::Query(nick.to_owned()));
        let channel = text
            .strip_prefix('[')
            .and_then(|rest| rest.split_once(']'))
            .and_then(|(channel, _)| self.channel(channel));
        query.or(channel).unwrap_or(0)
    }
    fn mode(
        &mut self,
        session: &Session,
        message: &Message,
        stamp: Stamp,
        updates: &mut Vec<Update>,
    ) {
        let Some(index) = self.channel(message.param(0)) else {
            return;
        };
        let mut args = message.params.iter().skip(2);
        let mut adding = true;
        let chat = &mut self.chats[index];
        for mode in message.param(1).chars() {
            match mode {
                '+' => adding = true,
                '-' => adding = false,
                mode if session.mode_takes_param(mode, adding) => {
                    let Some(arg) = args.next() else {
                        break;
                    };
                    let (Some(symbol), Some(member)) =
                        (session.prefix_symbol(mode), chat.member_mut(arg))
                    else {
                        continue;
                    };
                    member.prefixes.retain(|known| known != symbol);
                    if adding {
                        member.prefixes.push(symbol);
                        let mut symbols: Vec<char> = member.prefixes.chars().collect();
                        symbols.sort_by_key(|symbol| session.prefix_rank(*symbol));
                        member.prefixes = symbols.into_iter().collect();
                    }
                }
                _ => {}
            }
        }
        updates.push(Update::Members(index));
        let event = LineEvent::Mode {
            nick: message.nick().unwrap_or_default().to_owned(),
            change: message.params[1..].join(" "),
        };
        self.push(index, stamp.line(event), updates);
    }

    fn numeric(&mut self, message: &Message, stamp: Stamp, updates: &mut Vec<Update>) {
        let error = message.command.starts_with('4') || message.command.starts_with('5');
        let channel = self.channel(message.param(1));
        let skip = if channel.is_some() { 2 } else { 1 };
        let last = message.params.len().saturating_sub(1);
        let middle = message
            .params
            .get(skip..last)
            .map(|middle| middle.join(" "))
            .unwrap_or_default();
        let tail = message
            .params
            .get(last)
            .filter(|_| last >= 1)
            .cloned()
            .unwrap_or_default();
        let counts = !middle.is_empty()
            && middle
                .split(' ')
                .all(|token| token.chars().all(|c| c.is_ascii_digit()));
        let text = if middle.is_empty()
            || (counts && tail.contains(middle.split(' ').next().unwrap_or_default()))
        {
            tail
        } else if counts {
            format!("{middle} {tail}")
        } else {
            format!("{middle}: {tail}")
        };
        self.push(
            channel.unwrap_or(0),
            stamp.line(LineEvent::Info { text, error }),
            updates,
        );
    }
}

#[derive(Debug, Clone, Copy)]
struct Stamp {
    time: Timestamp,
    history: bool,
}

impl Stamp {
    fn line(self, event: LineEvent) -> Line {
        Line {
            time: self.time,
            event: event.plain(),
            history: self.history,
        }
    }
}

impl LineEvent {
    fn plain(self) -> Self {
        let clean = |text: String| plain(&text);
        let reason = |reason: Option<String>| reason.map(clean);
        match self {
            Self::Chat {
                nick,
                text,
                kind,
                own,
                mention,
            } => Self::Chat {
                nick,
                text: clean(text),
                kind,
                own,
                mention,
            },
            Self::Left { nick, reason: why } => Self::Left {
                nick,
                reason: reason(why),
            },
            Self::Quit { nick, reason: why } => Self::Quit {
                nick,
                reason: reason(why),
            },
            Self::Kicked {
                nick,
                by,
                reason: why,
            } => Self::Kicked {
                nick,
                by,
                reason: reason(why),
            },
            Self::Topic { nick, text } => Self::Topic {
                nick,
                text: clean(text),
            },
            Self::Info { text, error } => Self::Info {
                text: clean(text),
                error,
            },
            other => other,
        }
    }
}
#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use super::*;
    use crate::protocol::session::{Identity, Session};

    const NOW: &str = "2026-10-19T12:00:00Z";

    fn session() -> Session {
        let mut session = Session::new(Identity {
            nick: "oxtest".to_owned(),
            username: "oxtest".to_owned(),
            realname: "oxmacs".to_owned(),
            password: None,
            sasl: None,
        });
        session.receive(":srv 001 oxtest :Welcome");
        session.receive(":srv 005 oxtest PREFIX=(ov)@+ CHANTYPES=# :are supported");
        session.take_events();
        session
    }

    fn now() -> Timestamp {
        NOW.parse().unwrap()
    }

    fn apply(network: &mut Network, session: &Session, line: &str) -> Vec<Update> {
        network.apply(session, &Message::parse(line).unwrap(), now())
    }

    fn joined() -> (Network, Session) {
        let session = session();
        let mut network = Network::new("libera");
        for line in [
            ":oxtest!o@h JOIN #rust",
            ":srv 332 oxtest #rust :Rust talk",
            ":srv 353 oxtest = #rust :@alice +bob carol oxtest",
            ":srv 366 oxtest #rust :End of /NAMES list.",
        ] {
            apply(&mut network, &session, line);
        }
        (network, session)
    }

    fn rust(network: &Network) -> &Chat {
        let index = network.find(&Target::Channel("#rust".to_owned())).unwrap();
        &network.chats()[index]
    }

    fn nicks(network: &Network, session: &Session) -> Vec<String> {
        rust(network)
            .members(session)
            .iter()
            .map(|member| format!("{}{}", member.prefixes, member.nick))
            .collect()
    }

    fn last(network: &Network) -> &LineEvent {
        &rust(network).lines.last().unwrap().event
    }

    #[test]
    fn the_server_chat_comes_first() {
        let network = Network::new("libera");

        assert_eq!(network.chats()[0].target, Target::Server);
        assert_eq!(network.name(), "libera");
    }

    #[test]
    fn joining_creates_a_channel_whose_names_fill_its_members() {
        let session = session();
        let mut network = Network::new("libera");

        let updates = apply(&mut network, &session, ":oxtest!o@h JOIN #rust");
        assert_eq!(updates, [Update::Created(1)]);

        let (network, session) = joined();
        assert_eq!(rust(&network).topic.as_deref(), Some("Rust talk"));
        assert!(rust(&network).joined);
        assert_eq!(
            nicks(&network, &session),
            ["@alice", "+bob", "carol", "oxtest"]
        );
    }

    #[test]
    fn messages_land_in_their_channel_at_the_server_time() {
        let (mut network, session) = joined();

        let updates = apply(
            &mut network,
            &session,
            "@time=2026-10-19T11:59:30.000Z :alice!a@h PRIVMSG #rust :hello",
        );

        assert_eq!(updates, [Update::Line(1)]);
        let line = rust(&network).lines.last().unwrap();
        assert_eq!(
            line.time,
            "2026-10-19T11:59:30Z".parse::<Timestamp>().unwrap()
        );
        assert_eq!(
            line.event,
            LineEvent::Chat {
                nick: "alice".to_owned(),
                text: "hello".to_owned(),
                kind: Kind::Message,
                own: false,
                mention: false,
            }
        );
        assert_eq!(rust(&network).unread, 1);
    }

    #[test]
    fn private_messages_open_a_query_with_the_sender() {
        let session = session();
        let mut network = Network::new("libera");

        let updates = apply(&mut network, &session, ":alice!a@h PRIVMSG oxtest :hi");

        assert_eq!(updates, [Update::Created(1), Update::Line(1)]);
        assert_eq!(network.chats()[1].target, Target::Query("alice".to_owned()));
    }

    #[test]
    fn echoed_messages_go_to_their_target_and_are_already_read() {
        let session = session();
        let mut network = Network::new("libera");

        apply(&mut network, &session, ":oxtest!o@h PRIVMSG alice :hey");

        let chat = &network.chats()[1];
        assert_eq!(chat.target, Target::Query("alice".to_owned()));
        assert_eq!(chat.unread, 0);
        assert!(matches!(
            chat.lines[0].event,
            LineEvent::Chat { own: true, .. }
        ));
    }

    #[test]
    fn mentions_of_your_nick_are_counted() {
        let (mut network, session) = joined();

        apply(
            &mut network,
            &session,
            ":alice!a@h PRIVMSG #rust :hey OXtest, look",
        );
        apply(
            &mut network,
            &session,
            ":alice!a@h PRIVMSG #rust :oxtester is someone else",
        );

        assert_eq!(rust(&network).mentions, 1);
        assert_eq!(rust(&network).unread, 2);
    }

    #[test]
    fn private_notices_do_not_open_chats() {
        let (mut network, session) = joined();

        let updates = apply(
            &mut network,
            &session,
            ":ChanServ!C@services NOTICE oxtest :[#rust] Welcome to #rust!",
        );
        assert!(
            !updates
                .iter()
                .any(|update| matches!(update, Update::Created(_)))
        );
        assert!(matches!(
            last(&network),
            LineEvent::Chat { kind: Kind::Notice, nick, .. } if nick == "ChanServ"
        ));
        assert_eq!(rust(&network).unread, 0);

        apply(
            &mut network,
            &session,
            ":NickServ!N@services NOTICE oxtest :Hello",
        );
        assert!(matches!(
            &network.chats()[0].lines.last().unwrap().event,
            LineEvent::Chat { kind: Kind::Notice, text, .. } if text == "Hello"
        ));

        apply(&mut network, &session, ":alice!a@h PRIVMSG oxtest :hi");
        apply(
            &mut network,
            &session,
            ":alice!a@h NOTICE oxtest :also this",
        );
        let query = network.find(&Target::Query("alice".to_owned())).unwrap();
        assert_eq!(network.chats()[query].lines.len(), 2);
    }

    #[test]
    fn formatting_codes_and_invisible_characters_are_dropped() {
        let (mut network, session) = joined();

        apply(
            &mut network,
            &session,
            ":alice!a@h PRIVMSG #rust :\u{2}bold\u{2} \u{3}04,12red\u{3} \u{1d}it\u{f}\u{200b}\u{feff}",
        );

        assert!(matches!(
            last(&network),
            LineEvent::Chat { text, .. } if text == "bold red it"
        ));
    }

    #[test]
    fn actions_and_notices_are_kept_apart() {
        let (mut network, session) = joined();

        apply(
            &mut network,
            &session,
            ":alice!a@h PRIVMSG #rust :\u{1}ACTION waves\u{1}",
        );
        assert!(matches!(
            last(&network),
            LineEvent::Chat { kind: Kind::Action, text, .. } if text == "waves"
        ));

        apply(&mut network, &session, ":alice!a@h NOTICE #rust :psst");
        assert!(matches!(
            last(&network),
            LineEvent::Chat {
                kind: Kind::Notice,
                ..
            }
        ));
    }

    #[test]
    fn joins_parts_nicks_and_quits_update_members() {
        let (mut network, session) = joined();

        apply(&mut network, &session, ":dave!d@h JOIN #rust");
        assert_eq!(*last(&network), LineEvent::Joined("dave".to_owned()));
        assert!(nicks(&network, &session).contains(&"dave".to_owned()));

        apply(&mut network, &session, ":dave!d@h PART #rust :bye");
        assert_eq!(
            *last(&network),
            LineEvent::Left {
                nick: "dave".to_owned(),
                reason: Some("bye".to_owned())
            }
        );
        assert!(!nicks(&network, &session).contains(&"dave".to_owned()));

        apply(&mut network, &session, ":carol!c@h NICK :caroline");
        assert_eq!(
            *last(&network),
            LineEvent::Renamed {
                old: "carol".to_owned(),
                new: "caroline".to_owned()
            }
        );
        assert!(nicks(&network, &session).contains(&"caroline".to_owned()));

        apply(&mut network, &session, ":caroline!c@h QUIT :Quit: sleep");
        assert_eq!(
            *last(&network),
            LineEvent::Quit {
                nick: "caroline".to_owned(),
                reason: Some("Quit: sleep".to_owned())
            }
        );
        assert_eq!(rust(&network).unread, 0);
    }

    #[test]
    fn kicks_remove_the_member() {
        let (mut network, session) = joined();

        apply(&mut network, &session, ":alice!a@h KICK #rust bob :spam");

        assert_eq!(
            *last(&network),
            LineEvent::Kicked {
                nick: "bob".to_owned(),
                by: "alice".to_owned(),
                reason: Some("spam".to_owned())
            }
        );
        assert!(!nicks(&network, &session).contains(&"+bob".to_owned()));
    }

    #[test]
    fn modes_change_prefixes() {
        let (mut network, session) = joined();

        apply(
            &mut network,
            &session,
            ":alice!a@h MODE #rust +o-v+l carol bob 10",
        );

        assert_eq!(
            nicks(&network, &session),
            ["@alice", "@carol", "bob", "oxtest"]
        );
        assert_eq!(
            *last(&network),
            LineEvent::Mode {
                nick: "alice".to_owned(),
                change: "+o-v+l carol bob 10".to_owned()
            }
        );
    }

    #[test]
    fn topic_changes_are_shown_and_kept() {
        let (mut network, session) = joined();

        apply(&mut network, &session, ":alice!a@h TOPIC #rust :Rust 2027");

        assert_eq!(rust(&network).topic.as_deref(), Some("Rust 2027"));
        assert_eq!(
            *last(&network),
            LineEvent::Topic {
                nick: "alice".to_owned(),
                text: "Rust 2027".to_owned()
            }
        );
    }

    #[test]
    fn leaving_marks_the_channel_parted() {
        let (mut network, session) = joined();

        apply(&mut network, &session, ":oxtest!o@h PART #rust");

        assert!(!rust(&network).joined);
        assert_eq!(rust(&network).members(&session), Vec::<Member>::new());
    }

    #[test]
    fn server_replies_go_to_the_server_chat() {
        let session = session();
        let mut network = Network::new("libera");

        apply(
            &mut network,
            &session,
            ":srv 001 oxtest :Welcome to Libera.Chat",
        );
        apply(&mut network, &session, ":srv 372 oxtest :- Be nice");

        let lines: Vec<&LineEvent> = network.chats()[0]
            .lines
            .iter()
            .map(|line| &line.event)
            .collect();
        assert_eq!(
            lines,
            [
                &LineEvent::Info {
                    text: "Welcome to Libera.Chat".to_owned(),
                    error: false
                },
                &LineEvent::Info {
                    text: "- Be nice".to_owned(),
                    error: false
                }
            ]
        );
    }

    #[test]
    fn numeric_counts_read_naturally() {
        let session = session();
        let mut network = Network::new("libera");
        for line in [
            ":srv 004 oxtest srv solanum-1.0 DGIM CFIL bklov",
            ":srv 252 oxtest 32 :IRC Operators online",
            ":srv 265 oxtest 1795 1845 :Current local users 1795, max 1845",
            ":srv 433 * oxtest :Nickname is already in use",
        ] {
            apply(&mut network, &session, line);
        }

        let texts: Vec<&str> = network.chats()[0]
            .lines
            .iter()
            .filter_map(|line| match &line.event {
                LineEvent::Info { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            [
                "32 IRC Operators online",
                "Current local users 1795, max 1845",
                "oxtest: Nickname is already in use"
            ]
        );
    }

    #[test]
    fn errors_about_a_channel_show_in_it() {
        let (mut network, session) = joined();

        apply(
            &mut network,
            &session,
            ":srv 404 oxtest #rust :Cannot send to channel",
        );

        assert_eq!(
            *last(&network),
            LineEvent::Info {
                text: "Cannot send to channel".to_owned(),
                error: true
            }
        );
    }

    #[test]
    fn history_batches_are_not_unread() {
        let (mut network, session) = joined();

        for line in [
            ":srv BATCH +h1 chathistory #rust",
            "@batch=h1;time=2026-10-19T10:00:00.000Z :alice!a@h PRIVMSG #rust :earlier",
            ":srv BATCH -h1",
        ] {
            apply(&mut network, &session, line);
        }

        let line = rust(&network).lines.last().unwrap();
        assert!(line.history);
        assert_eq!(rust(&network).unread, 0);
    }

    #[test]
    fn away_members_are_marked() {
        let (mut network, session) = joined();

        apply(&mut network, &session, ":bob!b@h AWAY :lunch");
        let away: Vec<bool> = rust(&network)
            .members(&session)
            .iter()
            .map(|member| member.away)
            .collect();
        assert_eq!(away, [false, true, false, false]);

        apply(&mut network, &session, ":bob!b@h AWAY");
        assert!(
            rust(&network)
                .members(&session)
                .iter()
                .all(|member| !member.away)
        );
    }

    #[test]
    fn notes_and_disconnection_are_recorded() {
        let (mut network, session) = joined();
        let index = network.find(&Target::Channel("#rust".to_owned())).unwrap();

        assert_eq!(
            network.note(index, "Connected", false, now()),
            Update::Line(index)
        );
        network.disconnected();

        assert!(!rust(&network).joined);
        assert_eq!(rust(&network).members(&session), Vec::<Member>::new());
        assert_eq!(
            *last(&network),
            LineEvent::Info {
                text: "Connected".to_owned(),
                error: false
            }
        );
    }

    #[test]
    fn reading_clears_the_counts() {
        let (mut network, session) = joined();
        apply(
            &mut network,
            &session,
            ":alice!a@h PRIVMSG #rust :oxtest: hi",
        );
        let index = network.find(&Target::Channel("#RUST".to_owned())).unwrap();

        network.mark_read(index);

        assert_eq!((rust(&network).unread, rust(&network).mentions), (0, 0));
    }
}
