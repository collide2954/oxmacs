use std::collections::BTreeSet;

use crate::irc::message::Message;

const WANTED: &[&str] = &[
    "message-tags",
    "server-time",
    "batch",
    "echo-message",
    "account-notify",
    "account-tag",
    "away-notify",
    "chghost",
    "extended-join",
    "multi-prefix",
    "userhost-in-names",
    "invite-notify",
    "setname",
    "cap-notify",
    "sasl",
];
const SASL_CHUNK: usize = 400;
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sasl {
    pub account: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub nick: String,
    pub username: String,
    pub realname: String,
    pub sasl: Option<Sasl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Registered(String),
    Authenticated(String),
    SaslFailed(String),
    Message(Message),
}

#[derive(Debug)]
pub struct Session {
    identity: Identity,
    nick: String,
    registered: bool,
    negotiating: bool,
    listed: Vec<(String, Option<String>)>,
    enabled: BTreeSet<String>,
    authenticating: bool,
    prefix_modes: String,
    prefix_symbols: String,
    channel_types: String,
    channel_modes: [String; 4],
    network: Option<String>,
    outgoing: Vec<String>,
    events: Vec<Event>,
}

impl Session {
    pub fn new(identity: Identity) -> Self {
        Self {
            nick: identity.nick.clone(),
            identity,
            registered: false,
            negotiating: false,
            listed: Vec::new(),
            enabled: BTreeSet::new(),
            authenticating: false,
            prefix_modes: "ov".to_owned(),
            prefix_symbols: "@+".to_owned(),
            channel_types: "#&".to_owned(),
            channel_modes: [
                "beI".to_owned(),
                "k".to_owned(),
                "l".to_owned(),
                "imnpst".to_owned(),
            ],
            network: None,
            outgoing: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn start(&mut self) {
        self.negotiating = true;
        self.send(&Message::new("CAP", ["LS", "302"]));
        self.send(&Message::new("NICK", [self.nick.clone()]));
        self.send(&Message::new(
            "USER",
            [
                self.identity.username.clone(),
                "0".to_owned(),
                "*".to_owned(),
                self.identity.realname.clone(),
            ],
        ));
    }

    pub fn send(&mut self, message: &Message) {
        self.outgoing.push(message.to_line());
    }

    pub fn take_outgoing(&mut self) -> Vec<String> {
        std::mem::take(&mut self.outgoing)
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn nick(&self) -> &str {
        &self.nick
    }

    pub fn is_registered(&self) -> bool {
        self.registered
    }

    pub fn has_cap(&self, name: &str) -> bool {
        self.enabled.contains(name)
    }

    pub fn network_name(&self) -> Option<&str> {
        self.network.as_deref()
    }

    pub fn prefix_symbol(&self, mode: char) -> Option<char> {
        let index = self.prefix_modes.chars().position(|known| known == mode)?;
        self.prefix_symbols.chars().nth(index)
    }

    pub fn prefix_rank(&self, symbol: char) -> Option<usize> {
        self.prefix_symbols
            .chars()
            .position(|known| known == symbol)
    }

    pub fn prefix_symbols(&self) -> &str {
        &self.prefix_symbols
    }

    pub fn mode_takes_param(&self, mode: char, adding: bool) -> bool {
        let [list, always, when_set, _] = &self.channel_modes;
        self.prefix_modes.contains(mode)
            || list.contains(mode)
            || always.contains(mode)
            || (adding && when_set.contains(mode))
    }

    pub fn is_channel(&self, target: &str) -> bool {
        target
            .chars()
            .next()
            .is_some_and(|first| self.channel_types.contains(first))
    }

    pub fn receive(&mut self, line: &str) {
        let Some(message) = Message::parse(line) else {
            return;
        };
        match message.command.as_str() {
            "PING" => {
                self.send(&Message::new("PONG", message.params));
                return;
            }
            "CAP" => {
                self.capability(&message);
                return;
            }
            "AUTHENTICATE" => {
                self.authenticate(&message);
                return;
            }
            "001" => {
                self.registered = true;
                self.negotiating = false;
                message.param(0).clone_into(&mut self.nick);
                self.events.push(Event::Registered(self.nick.clone()));
            }
            "005" => self.isupport(&message),
            "433" if !self.registered => {
                let nick = format!("{}_", message.param(1));
                self.nick.clone_from(&nick);
                self.send(&Message::new("NICK", [nick]));
            }
            "NICK" if message.nick() == Some(self.nick.as_str()) => {
                message.param(0).clone_into(&mut self.nick);
            }
            "900" => {
                self.events
                    .push(Event::Authenticated(message.param(2).to_owned()));
            }
            "903" => self.finish_sasl(),
            "902" | "904" | "905" | "906" | "907" | "908" => {
                let text = message.params.last().cloned().unwrap_or_default();
                self.events.push(Event::SaslFailed(text));
                self.finish_sasl();
            }
            _ => {}
        }
        self.events.push(Event::Message(message));
    }

    fn capability(&mut self, message: &Message) {
        let subcommand = message.param(1).to_ascii_uppercase();
        let more = message.params.len() > 3 && message.param(2) == "*";
        let list = message.params.last().map_or("", String::as_str);
        let names = list.split_whitespace();
        match subcommand.as_str() {
            "LS" => {
                self.listed.extend(names.map(listed_cap));
                if !more {
                    let wanted = self.wanted(&self.listed);
                    self.request(&wanted);
                }
            }
            "NEW" => {
                let offered: Vec<(String, Option<String>)> = names.map(listed_cap).collect();
                let wanted = self.wanted(&offered);
                self.listed.extend(offered);
                self.request(&wanted);
            }
            "DEL" => {
                for name in names {
                    self.enabled.remove(name);
                }
            }
            "ACK" => {
                for name in names {
                    match name.strip_prefix('-') {
                        Some(removed) => {
                            self.enabled.remove(removed);
                        }
                        None => {
                            self.enabled.insert(name.to_owned());
                        }
                    }
                }
                if self.negotiating {
                    if self.has_cap("sasl") && self.identity.sasl.is_some() {
                        self.authenticating = true;
                        self.send(&Message::new("AUTHENTICATE", ["PLAIN"]));
                    } else {
                        self.end_negotiation();
                    }
                }
            }
            "NAK" if self.negotiating => self.end_negotiation(),
            _ => {}
        }
    }

    fn wanted(&self, offered: &[(String, Option<String>)]) -> Vec<&'static str> {
        WANTED
            .iter()
            .copied()
            .filter(|wanted| !self.enabled.contains(*wanted))
            .filter(|wanted| {
                offered.iter().any(|(name, value)| {
                    name == wanted && (*wanted != "sasl" || self.sasl_usable(value.as_deref()))
                })
            })
            .collect()
    }

    fn sasl_usable(&self, mechanisms: Option<&str>) -> bool {
        self.identity.sasl.is_some()
            && mechanisms.is_none_or(|mechanisms| mechanisms.split(',').any(|m| m == "PLAIN"))
    }

    fn request(&mut self, wanted: &[&'static str]) {
        if wanted.is_empty() {
            if self.negotiating {
                self.end_negotiation();
            }
            return;
        }
        self.send(&Message::new("CAP", ["REQ".to_owned(), wanted.join(" ")]));
    }

    fn end_negotiation(&mut self) {
        self.negotiating = false;
        self.send(&Message::new("CAP", ["END"]));
    }

    fn authenticate(&mut self, message: &Message) {
        if message.param(0) != "+" || !self.authenticating {
            return;
        }
        let Some(sasl) = &self.identity.sasl else {
            return;
        };
        let payload = format!("{0}\0{0}\0{1}", sasl.account, sasl.password);
        for chunk in authenticate_chunks(&base64(payload.as_bytes())) {
            self.send(&Message::new("AUTHENTICATE", [chunk]));
        }
    }

    fn finish_sasl(&mut self) {
        if self.authenticating {
            self.authenticating = false;
            self.end_negotiation();
        }
    }

    fn isupport(&mut self, message: &Message) {
        let tokens =
            &message.params[1.min(message.params.len())..message.params.len().saturating_sub(1)];
        for token in tokens {
            let (key, value) = token.split_once('=').unwrap_or((token, ""));
            match key {
                "PREFIX" => {
                    if let Some((modes, symbols)) = value
                        .strip_prefix('(')
                        .and_then(|rest| rest.split_once(')'))
                    {
                        modes.clone_into(&mut self.prefix_modes);
                        symbols.clone_into(&mut self.prefix_symbols);
                    }
                }
                "CHANTYPES" => value.clone_into(&mut self.channel_types),
                "CHANMODES" => {
                    for (kind, modes) in self.channel_modes.iter_mut().zip(value.split(',')) {
                        modes.clone_into(kind);
                    }
                }
                "NETWORK" => self.network = Some(value.to_owned()),
                _ => {}
            }
        }
    }
}

fn listed_cap(entry: &str) -> (String, Option<String>) {
    match entry.split_once('=') {
        Some((name, value)) => (name.to_owned(), Some(value.to_owned())),
        None => (entry.to_owned(), None),
    }
}

fn base64(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = chunk.iter().enumerate().fold(0u32, |acc, (index, byte)| {
            acc | (u32::from(*byte) << (16 - 8 * index))
        });
        for index in 0..4 {
            if index <= chunk.len() {
                let sextet = (triple >> (18 - 6 * index)) & 0x3F;
                encoded.push(char::from(BASE64[sextet as usize]));
            } else {
                encoded.push('=');
            }
        }
    }
    encoded
}

fn authenticate_chunks(encoded: &str) -> Vec<String> {
    let mut chunks: Vec<String> = encoded
        .as_bytes()
        .chunks(SASL_CHUNK)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect();
    if encoded.len().is_multiple_of(SASL_CHUNK) {
        chunks.push("+".to_owned());
    }
    chunks
}
#[cfg(test)]
mod tests {
    use super::*;

    fn identity(sasl: bool) -> Identity {
        Identity {
            nick: "oxtest".to_owned(),
            username: "oxtest".to_owned(),
            realname: "oxmacs".to_owned(),
            sasl: sasl.then(|| Sasl {
                account: "oxtest".to_owned(),
                password: "hunter2".to_owned(),
            }),
        }
    }

    fn started(sasl: bool) -> Session {
        let mut session = Session::new(identity(sasl));
        session.start();
        session.take_outgoing();
        session
    }

    fn feed(session: &mut Session, lines: &[&str]) -> Vec<String> {
        for line in lines {
            session.receive(line);
        }
        session.take_outgoing()
    }

    #[test]
    fn registration_starts_with_capability_negotiation() {
        let mut session = Session::new(identity(false));

        session.start();

        assert_eq!(
            session.take_outgoing(),
            ["CAP LS 302", "NICK oxtest", "USER oxtest 0 * oxmacs"]
        );
    }

    #[test]
    fn wanted_capabilities_are_requested_and_negotiation_ends() {
        let mut session = started(false);

        let sent = feed(
            &mut session,
            &[":srv CAP * LS :multi-prefix sasl=PLAIN server-time foo message-tags batch"],
        );
        assert_eq!(
            sent,
            ["CAP REQ :message-tags server-time batch multi-prefix"]
        );

        let sent = feed(
            &mut session,
            &[":srv CAP oxtest ACK :message-tags server-time batch multi-prefix"],
        );
        assert_eq!(sent, ["CAP END"]);
        assert!(session.has_cap("server-time"));
        assert!(!session.has_cap("sasl"));
    }

    #[test]
    fn multiline_capability_lists_are_joined() {
        let mut session = started(false);

        let first = feed(&mut session, &[":srv CAP * LS * :server-time"]);
        assert_eq!(first, Vec::<String>::new());

        let sent = feed(&mut session, &[":srv CAP * LS :echo-message"]);
        assert_eq!(sent, ["CAP REQ :server-time echo-message"]);
    }

    #[test]
    fn sasl_plain_authenticates_before_ending_negotiation() {
        let mut session = started(true);
        feed(
            &mut session,
            &[":srv CAP * LS :sasl=PLAIN,EXTERNAL server-time"],
        );

        let sent = feed(&mut session, &[":srv CAP oxtest ACK :server-time sasl"]);
        assert_eq!(sent, ["AUTHENTICATE PLAIN"]);

        let sent = feed(&mut session, &["AUTHENTICATE +"]);
        assert_eq!(sent, ["AUTHENTICATE b3h0ZXN0AG94dGVzdABodW50ZXIy"]);

        let sent = feed(
            &mut session,
            &[
                ":srv 900 oxtest oxtest!o@h oxtest :You are now logged in as oxtest",
                ":srv 903 oxtest :SASL authentication successful",
            ],
        );
        assert_eq!(sent, ["CAP END"]);
        assert!(
            session
                .take_events()
                .contains(&Event::Authenticated("oxtest".to_owned()))
        );
    }

    #[test]
    fn sasl_is_not_requested_without_plain() {
        let mut session = started(true);

        let sent = feed(&mut session, &[":srv CAP * LS :sasl=EXTERNAL server-time"]);

        assert_eq!(sent, ["CAP REQ server-time"]);
    }

    #[test]
    fn failed_sasl_is_reported_and_registration_continues() {
        let mut session = started(true);
        feed(
            &mut session,
            &[
                ":srv CAP * LS :sasl",
                ":srv CAP oxtest ACK :sasl",
                "AUTHENTICATE +",
            ],
        );

        let sent = feed(
            &mut session,
            &[":srv 904 oxtest :SASL authentication failed"],
        );

        assert_eq!(sent, ["CAP END"]);
        assert!(
            session
                .take_events()
                .contains(&Event::SaslFailed("SASL authentication failed".to_owned()))
        );
    }

    #[test]
    fn refused_capabilities_end_negotiation() {
        let mut session = started(false);
        feed(&mut session, &[":srv CAP * LS :server-time"]);

        let sent = feed(&mut session, &[":srv CAP * NAK :server-time"]);

        assert_eq!(sent, ["CAP END"]);
        assert!(!session.has_cap("server-time"));
    }

    #[test]
    fn welcome_completes_registration_with_the_given_nick() {
        let mut session = started(false);

        feed(&mut session, &[":srv 001 oxtest_ :Welcome to the network"]);

        assert_eq!(session.nick(), "oxtest_");
        assert!(session.is_registered());
        assert!(
            session
                .take_events()
                .contains(&Event::Registered("oxtest_".to_owned()))
        );
    }

    #[test]
    fn taken_nicks_get_an_underscore_while_registering() {
        let mut session = started(false);

        let sent = feed(
            &mut session,
            &[":srv 433 * oxtest :Nickname is already in use"],
        );
        assert_eq!(sent, ["NICK oxtest_"]);

        let sent = feed(
            &mut session,
            &[":srv 433 * oxtest_ :Nickname is already in use"],
        );
        assert_eq!(sent, ["NICK oxtest__"]);
    }

    #[test]
    fn pings_are_answered() {
        let mut session = started(false);

        assert_eq!(feed(&mut session, &["PING :abc"]), ["PONG abc"]);
        assert_eq!(
            feed(&mut session, &["PING :irc.example.net"]),
            ["PONG irc.example.net"]
        );
    }

    #[test]
    fn new_capabilities_are_requested_and_removed_ones_dropped() {
        let mut session = started(false);
        feed(
            &mut session,
            &[
                ":srv CAP * LS :cap-notify server-time",
                ":srv CAP oxtest ACK :cap-notify server-time",
                ":srv 001 oxtest :Welcome",
            ],
        );

        let sent = feed(&mut session, &[":srv CAP oxtest NEW :away-notify unknown"]);
        assert_eq!(sent, ["CAP REQ away-notify"]);

        feed(&mut session, &[":srv CAP oxtest DEL :server-time"]);
        assert!(!session.has_cap("server-time"));
    }

    #[test]
    fn server_support_is_remembered() {
        let mut session = started(false);

        feed(
            &mut session,
            &[
                ":srv 005 oxtest PREFIX=(qaohv)~&@%+ CHANTYPES=# NETWORK=Libera.Chat :are supported by this server",
            ],
        );

        assert_eq!(session.network_name(), Some("Libera.Chat"));
        assert_eq!(session.prefix_symbol('o'), Some('@'));
        assert_eq!(session.prefix_rank('~'), Some(0));
        assert_eq!(session.prefix_rank('+'), Some(4));
        assert!(session.is_channel("#rust"));
        assert!(!session.is_channel("alice"));
    }

    #[test]
    fn mode_parameters_follow_chanmodes() {
        let mut session = started(false);
        feed(
            &mut session,
            &[":srv 005 oxtest CHANMODES=beI,k,l,imnst PREFIX=(ov)@+ :are supported"],
        );

        assert!(session.mode_takes_param('o', true));
        assert!(session.mode_takes_param('b', false));
        assert!(session.mode_takes_param('k', false));
        assert!(session.mode_takes_param('l', true));
        assert!(!session.mode_takes_param('l', false));
        assert!(!session.mode_takes_param('m', true));
    }

    #[test]
    fn other_messages_are_passed_on() {
        let mut session = started(false);

        session.receive(":alice!a@h PRIVMSG #rust :hi");

        assert_eq!(
            session.take_events(),
            [Event::Message(
                Message::parse(":alice!a@h PRIVMSG #rust :hi").unwrap()
            )]
        );
    }

    #[test]
    fn base64_matches_the_rfc_vectors() {
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), encoded);
        }
    }

    #[test]
    fn long_sasl_payloads_are_split_into_chunks() {
        let chunks = authenticate_chunks(&"a".repeat(400));
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[1], "+");

        let chunks = authenticate_chunks(&"a".repeat(450));
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[1].len(), 50);
    }
}
