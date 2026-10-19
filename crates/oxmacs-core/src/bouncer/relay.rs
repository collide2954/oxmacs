use std::time::Duration;

use jiff::Timestamp;

use crate::bouncer::history::History;
use crate::config::IrcServer;
use crate::external::net::{Endpoint, Event};
use crate::irc::message::Message;
use crate::irc::network::{Network, Target};
use crate::irc::session::{Event as SessionEvent, Identity, Sasl, Session};

const NAME: &str = "oxmacs";
const OFFERED: &[&str] = &[
    "server-time",
    "message-tags",
    "batch",
    "echo-message",
    "multi-prefix",
];
const REPLAY: usize = 300;
const FIRST_WAIT: Duration = Duration::from_secs(10);
const LONGEST_WAIT: Duration = Duration::from_secs(300);
const QUIET: &[&str] = &[
    "001", "002", "003", "004", "005", "250", "251", "252", "253", "254", "255", "265", "266",
    "372", "375", "376",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Connect { network: usize, endpoint: Endpoint },
    Reconnect { network: usize, after: Duration },
    Upstream { network: usize, line: String },
    Client { client: ClientId, line: String },
    Drop(ClientId),
    Stop,
}

#[derive(Debug)]
struct Upstream {
    server: IrcServer,
    password: Option<String>,
    session: Session,
    network: Network,
    support: Vec<String>,
    rejoin: Vec<String>,
    wait: Duration,
    detached: Option<Timestamp>,
}

impl Upstream {
    fn session(server: &IrcServer, password: Option<&String>) -> Session {
        Session::new(Identity {
            nick: server.nick.clone(),
            username: server.username().to_owned(),
            realname: server.realname().to_owned(),
            sasl: server
                .sasl
                .clone()
                .zip(password.cloned())
                .map(|(account, password)| Sasl { account, password }),
        })
    }

    fn target_of(&self, message: &Message) -> Option<String> {
        let sender = message.nick().unwrap_or_default();
        let target = message.param(0);
        match message.command.as_str() {
            "PRIVMSG" | "NOTICE" if self.session.is_channel(target) => Some(target.to_owned()),
            "PRIVMSG" | "NOTICE" if sender.eq_ignore_ascii_case(self.session.nick()) => {
                Some(target.to_owned())
            }
            "PRIVMSG" | "NOTICE" => message
                .source
                .as_deref()
                .filter(|source| source.contains('!'))
                .map(|_| sender.to_owned()),
            "JOIN" | "PART" | "KICK" | "TOPIC" | "MODE" if self.session.is_channel(target) => {
                Some(target.to_owned())
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Client {
    id: ClientId,
    password: Option<String>,
    nick: Option<String>,
    user: Option<String>,
    negotiating: bool,
    caps: Vec<String>,
    network: Option<usize>,
}

impl Client {
    fn has(&self, cap: &str) -> bool {
        self.caps.iter().any(|known| known == cap)
    }

    fn shape(&self, message: &Message) -> String {
        let mut message = message.clone();
        if !self.has("message-tags") {
            message.tags.retain(|(key, _)| {
                (key == "time" && self.has("server-time")) || (key == "batch" && self.has("batch"))
            });
        }
        message.to_line()
    }
}

#[derive(Debug)]
pub struct Bouncer<H> {
    password: String,
    upstreams: Vec<Upstream>,
    clients: Vec<Client>,
    history: H,
    actions: Vec<Action>,
    batches: u64,
}

impl<H: History> Bouncer<H> {
    pub fn new(
        servers: Vec<IrcServer>,
        passwords: Vec<Option<String>>,
        password: &str,
        history: H,
    ) -> Self {
        let upstreams = servers
            .into_iter()
            .zip(passwords.into_iter().chain(std::iter::repeat(None)))
            .map(|(server, password)| Upstream {
                session: Upstream::session(&server, password.as_ref()),
                network: Network::new(&server.name),
                server,
                password,
                support: Vec::new(),
                rejoin: Vec::new(),
                wait: FIRST_WAIT,
                detached: None,
            })
            .collect();
        Self {
            password: password.to_owned(),
            upstreams,
            clients: Vec::new(),
            history,
            actions: Vec::new(),
            batches: 0,
        }
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    pub fn start(&mut self) {
        for network in 0..self.upstreams.len() {
            self.reconnect(network);
        }
    }

    pub fn reconnect(&mut self, network: usize) {
        let Some(upstream) = self.upstreams.get(network) else {
            return;
        };
        self.actions.push(Action::Connect {
            network,
            endpoint: Endpoint {
                host: upstream.server.host.clone(),
                port: upstream.server.port,
                tls: upstream.server.tls,
            },
        });
    }

    fn flush(&mut self, network: usize) {
        for line in self.upstreams[network].session.take_outgoing() {
            self.actions.push(Action::Upstream { network, line });
        }
    }

    fn send(&mut self, client: ClientId, line: String) {
        self.actions.push(Action::Client { client, line });
    }

    pub fn upstream_event(&mut self, network: usize, event: Event, now: Timestamp) {
        let Some(upstream) = self.upstreams.get_mut(network) else {
            return;
        };
        match event {
            Event::Connected => {
                upstream.session = Upstream::session(&upstream.server, upstream.password.as_ref());
                upstream.support.clear();
                upstream.session.start();
            }
            Event::Line(line) => {
                upstream.session.receive(&line);
                for event in upstream.session.take_events() {
                    self.session_event(network, event, &line, now);
                }
            }
            Event::Closed(_) => {
                upstream.rejoin = upstream
                    .network
                    .chats()
                    .iter()
                    .filter(|chat| chat.joined)
                    .filter_map(|chat| chat.target.name().map(str::to_owned))
                    .collect();
                upstream.network.disconnected();
                let after = upstream.wait;
                upstream.wait = (upstream.wait * 2).min(LONGEST_WAIT);
                self.actions.push(Action::Reconnect { network, after });
            }
        }
        self.flush(network);
    }

    fn session_event(&mut self, network: usize, event: SessionEvent, line: &str, now: Timestamp) {
        let upstream = &mut self.upstreams[network];
        match event {
            SessionEvent::Registered(_) => {
                upstream.wait = FIRST_WAIT;
                let mut channels = upstream.server.channels.clone();
                for channel in upstream.rejoin.drain(..) {
                    if !channels
                        .iter()
                        .any(|known| known.eq_ignore_ascii_case(&channel))
                    {
                        channels.push(channel);
                    }
                }
                if !channels.is_empty() {
                    upstream
                        .session
                        .send(&Message::new("JOIN", [channels.join(",")]));
                }
            }
            SessionEvent::Message(message) => {
                if message.command == "005" {
                    upstream.support.push(line.to_owned());
                }
                upstream.network.apply(&upstream.session, &message, now);
                if !QUIET.contains(&message.command.as_str()) {
                    self.relay(network, message, now);
                }
            }
            SessionEvent::Authenticated(_) | SessionEvent::SaslFailed(_) => {}
        }
    }

    fn relay(&mut self, network: usize, mut message: Message, now: Timestamp) {
        if !message.has_tag("time") {
            message = message.with_tag("time", Some(&now.to_string()));
        }
        let upstream = &self.upstreams[network];
        if let Some(target) = upstream.target_of(&message) {
            let name = upstream.server.name.clone();
            self.history.append(&name, &target, &message.to_line());
        }
        let lines: Vec<(ClientId, String)> = self
            .clients
            .iter()
            .filter(|client| client.network == Some(network))
            .map(|client| (client.id, client.shape(&message)))
            .collect();
        for (client, line) in lines {
            self.send(client, line);
        }
    }

    pub fn client_attached(&mut self, id: ClientId) {
        self.clients.push(Client {
            id,
            password: None,
            nick: None,
            user: None,
            negotiating: false,
            caps: Vec::new(),
            network: None,
        });
    }

    pub fn client_closed(&mut self, id: ClientId, now: Timestamp) {
        let Some(index) = self.clients.iter().position(|client| client.id == id) else {
            return;
        };
        let client = self.clients.remove(index);
        if let Some(network) = client.network
            && !self
                .clients
                .iter()
                .any(|other| other.network == Some(network))
        {
            self.upstreams[network].detached = Some(now);
        }
    }

    pub fn client_line(&mut self, id: ClientId, line: &str, now: Timestamp) {
        let Some(message) = Message::parse(line) else {
            return;
        };
        let Some(index) = self.clients.iter().position(|client| client.id == id) else {
            return;
        };
        match message.command.as_str() {
            "PING" => {
                let reply = Message {
                    source: Some(NAME.to_owned()),
                    ..Message::new("PONG", [NAME, message.param(0)])
                };
                self.send(id, reply.to_line());
            }
            "CAP" => self.client_capability(index, &message, now),
            "QUIT" => {
                self.actions.push(Action::Drop(id));
                self.client_closed(id, now);
            }
            "OXMACS" if message.param(0).eq_ignore_ascii_case("STOP") => {
                if self.clients[index].network.is_some() {
                    self.stop();
                }
            }
            _ if self.clients[index].network.is_none() => {
                let client = &mut self.clients[index];
                match message.command.as_str() {
                    "PASS" => client.password = Some(message.param(0).to_owned()),
                    "NICK" => client.nick = Some(message.param(0).to_owned()),
                    "USER" => client.user = Some(message.param(0).to_owned()),
                    _ => return,
                }
                self.try_register(index, now);
            }
            _ => {
                let Some(network) = self.clients[index].network else {
                    return;
                };
                self.upstreams[network].session.send_raw(line);
                self.flush(network);
            }
        }
    }

    fn stop(&mut self) {
        for network in 0..self.upstreams.len() {
            self.upstreams[network]
                .session
                .send(&Message::new("QUIT", ["oxmacs bouncer stopping"]));
            self.flush(network);
        }
        self.actions.push(Action::Stop);
    }

    fn client_capability(&mut self, index: usize, message: &Message, now: Timestamp) {
        let id = self.clients[index].id;
        let target = self.clients[index]
            .nick
            .clone()
            .unwrap_or_else(|| "*".to_owned());
        match message.param(0).to_ascii_uppercase().as_str() {
            "LS" => {
                self.clients[index].negotiating = true;
                let offered = Message::new("CAP", [target, "LS".to_owned(), OFFERED.join(" ")]);
                self.send(id, offered.to_line());
            }
            "REQ" => {
                let wanted = message.param(1).to_owned();
                let names: Vec<&str> = wanted.split_whitespace().collect();
                let ok = names.iter().all(|name| OFFERED.contains(name));
                if ok {
                    self.clients[index]
                        .caps
                        .extend(names.iter().map(|name| (*name).to_owned()));
                }
                let verdict = if ok { "ACK" } else { "NAK" };
                self.send(
                    id,
                    Message::new("CAP", [target, verdict.to_owned(), wanted]).to_line(),
                );
            }
            "END" => {
                self.clients[index].negotiating = false;
                self.try_register(index, now);
            }
            _ => {}
        }
    }

    fn refuse(&mut self, index: usize, reason: &str, now: Timestamp) {
        let id = self.clients[index].id;
        self.send(id, Message::new("ERROR", [reason]).to_line());
        self.actions.push(Action::Drop(id));
        self.client_closed(id, now);
    }

    fn try_register(&mut self, index: usize, now: Timestamp) {
        let client = &self.clients[index];
        let (Some(_), Some(user)) = (&client.nick, &client.user) else {
            return;
        };
        if client.negotiating {
            return;
        }
        if client.password.as_deref() != Some(self.password.as_str()) {
            self.refuse(index, "Wrong bouncer password", now);
            return;
        }
        let wanted = user.split_once('/').map(|(_, network)| network.to_owned());
        let network = match &wanted {
            Some(name) => self
                .upstreams
                .iter()
                .position(|upstream| upstream.server.name.eq_ignore_ascii_case(name)),
            None => (self.upstreams.len() == 1).then_some(0),
        };
        let Some(network) = network else {
            let name = wanted.unwrap_or_default();
            self.refuse(index, &format!("No network called {name}"), now);
            return;
        };
        self.clients[index].network = Some(network);
        self.welcome(index, network);
        self.upstreams[network].detached = None;
    }

    fn welcome(&mut self, index: usize, network: usize) {
        let id = self.clients[index].id;
        let upstream = &self.upstreams[network];
        let nick = upstream.session.nick().to_owned();
        let name = upstream.server.name.clone();
        let multi = self.clients[index].has("multi-prefix");
        let mut lines = vec![
            Message {
                source: Some(NAME.to_owned()),
                ..Message::new(
                    "001",
                    [
                        nick.clone(),
                        format!("Welcome to oxmacs, attached to {name}"),
                    ],
                )
            }
            .to_line(),
        ];
        lines.extend(upstream.support.iter().cloned());
        let server = |command: &str, params: Vec<String>| {
            Message {
                source: Some(NAME.to_owned()),
                ..Message::new(command, params)
            }
            .to_line()
        };
        for chat in upstream.network.chats().iter().filter(|chat| chat.joined) {
            let Target::Channel(channel) = &chat.target else {
                continue;
            };
            lines.push(format!(":{nick}!{NAME}@{NAME} JOIN {channel}"));
            if let Some(topic) = &chat.topic {
                lines.push(server(
                    "332",
                    vec![nick.clone(), channel.clone(), topic.clone()],
                ));
            }
            let names: Vec<String> = chat
                .members(&upstream.session)
                .into_iter()
                .map(|member| {
                    let prefixes = if multi {
                        member.prefixes
                    } else {
                        member.prefixes.chars().take(1).collect()
                    };
                    format!("{prefixes}{}", member.nick)
                })
                .collect();
            lines.push(server(
                "353",
                vec![
                    nick.clone(),
                    "=".to_owned(),
                    channel.clone(),
                    names.join(" "),
                ],
            ));
            lines.push(server(
                "366",
                vec![
                    nick.clone(),
                    channel.clone(),
                    "End of /NAMES list.".to_owned(),
                ],
            ));
        }
        for line in lines {
            self.send(id, line);
        }
        self.replay(index, network);
    }

    fn replay(&mut self, index: usize, network: usize) {
        let upstream = &self.upstreams[network];
        let name = upstream.server.name.clone();
        let since = upstream.detached;
        let client = &self.clients[index];
        let id = client.id;
        let mut lines = Vec::new();
        for target in self.history.targets(&name) {
            let missed = self.history.since(&name, &target, since, REPLAY);
            if missed.is_empty() {
                continue;
            }
            self.batches += 1;
            let batch = format!("h{}", self.batches);
            let batched = client.has("batch");
            if batched {
                lines.push(format!("BATCH +{batch} chathistory {target}"));
            }
            for line in missed {
                let Some(message) = Message::parse(&line) else {
                    continue;
                };
                let message = if batched {
                    message.with_tag("batch", Some(&batch))
                } else {
                    message
                };
                lines.push(client.shape(&message));
            }
            if batched {
                lines.push(format!("BATCH -{batch}"));
            }
        }
        for line in lines {
            self.send(id, line);
        }
    }
}
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jiff::Timestamp;

    use super::*;
    use crate::bouncer::history::MemoryHistory;
    use crate::config::Config;

    const PASSWORD: &str = "s3cret";
    const ME: ClientId = ClientId(1);

    fn at(time: &str) -> Timestamp {
        format!("2026-10-19T{time}Z").parse().unwrap()
    }

    fn started() -> Bouncer<MemoryHistory> {
        let servers = Config::parse(
            "[[irc-server]]\nname = \"libera\"\nhost = \"irc.libera.chat\"\nnick = \"oxtest\"\nchannels = [\"#rust\"]\n",
        )
        .unwrap()
        .irc_server;
        let mut bouncer = Bouncer::new(servers, vec![None], PASSWORD, MemoryHistory::default());
        bouncer.start();
        bouncer
    }

    fn upstream(bouncer: &mut Bouncer<MemoryHistory>, lines: &[&str], now: Timestamp) {
        for line in lines {
            bouncer.upstream_event(0, Event::Line((*line).to_owned()), now);
        }
    }

    fn registered() -> Bouncer<MemoryHistory> {
        let mut bouncer = started();
        bouncer.upstream_event(0, Event::Connected, at("10:00:00"));
        upstream(
            &mut bouncer,
            &[
                ":srv CAP * LS :server-time message-tags batch echo-message multi-prefix",
                ":srv CAP oxtest ACK :message-tags server-time batch echo-message multi-prefix",
                ":srv 001 oxtest :Welcome",
                ":srv 005 oxtest PREFIX=(ov)@+ CHANTYPES=# NETWORK=Libera.Chat :are supported",
                ":oxtest!o@h JOIN #rust",
                ":srv 332 oxtest #rust :Rust talk",
                ":srv 353 oxtest = #rust :@alice oxtest",
                ":srv 366 oxtest #rust :End",
            ],
            at("10:00:00"),
        );
        bouncer.take_actions();
        bouncer
    }

    fn parsed(lines: &[impl AsRef<str>]) -> Vec<Option<Message>> {
        lines
            .iter()
            .map(|line| Message::parse(line.as_ref()))
            .collect()
    }

    fn sent_upstream(actions: &[Action]) -> Vec<String> {
        actions
            .iter()
            .filter_map(|action| match action {
                Action::Upstream { line, .. } => Some(line.clone()),
                _ => None,
            })
            .collect()
    }

    fn sent_to(actions: &[Action], client: ClientId) -> Vec<String> {
        actions
            .iter()
            .filter_map(|action| match action {
                Action::Client { client: to, line } if *to == client => Some(line.clone()),
                _ => None,
            })
            .collect()
    }

    fn attach(bouncer: &mut Bouncer<MemoryHistory>, lines: &[&str], now: Timestamp) -> Vec<Action> {
        bouncer.client_attached(ME);
        for line in lines {
            bouncer.client_line(ME, line, now);
        }
        bouncer.take_actions()
    }

    fn modern(password: &str) -> Vec<String> {
        vec![
            "CAP LS 302".to_owned(),
            format!("PASS {password}"),
            "NICK whatever".to_owned(),
            "USER me/libera 0 * :me".to_owned(),
            "CAP REQ :server-time message-tags batch echo-message multi-prefix".to_owned(),
            "CAP END".to_owned(),
        ]
    }

    fn attach_modern(bouncer: &mut Bouncer<MemoryHistory>, now: Timestamp) -> Vec<String> {
        let lines = modern(PASSWORD);
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        sent_to(&attach(bouncer, &lines, now), ME)
    }

    #[test]
    fn starting_connects_to_every_server() {
        let mut bouncer = started();

        assert_eq!(
            bouncer.take_actions(),
            [Action::Connect {
                network: 0,
                endpoint: Endpoint {
                    host: "irc.libera.chat".to_owned(),
                    port: 6697,
                    tls: true
                }
            }]
        );
    }

    #[test]
    fn upstreams_register_then_join_their_channels() {
        let mut bouncer = started();
        bouncer.take_actions();

        bouncer.upstream_event(0, Event::Connected, at("10:00:00"));
        assert_eq!(
            sent_upstream(&bouncer.take_actions()),
            ["CAP LS 302", "NICK oxtest", "USER oxtest 0 * oxmacs"]
        );

        upstream(
            &mut bouncer,
            &[":srv CAP * LS :", ":srv 001 oxtest :Welcome"],
            at("10:00:00"),
        );
        assert_eq!(
            sent_upstream(&bouncer.take_actions()),
            ["CAP END", "JOIN #rust"]
        );
    }

    #[test]
    fn clients_need_the_password() {
        let mut bouncer = registered();
        let lines = modern("wrong");
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();

        let actions = attach(&mut bouncer, &lines, at("11:00:00"));

        assert!(sent_to(&actions, ME).contains(&"ERROR :Wrong bouncer password".to_owned()));
        assert!(actions.contains(&Action::Drop(ME)));
    }

    #[test]
    fn unknown_networks_are_refused() {
        let mut bouncer = registered();

        let actions = attach(
            &mut bouncer,
            &["PASS s3cret", "NICK x", "USER me/nope 0 * :me"],
            at("11:00:00"),
        );

        assert!(sent_to(&actions, ME).contains(&"ERROR :No network called nope".to_owned()));
        assert!(actions.contains(&Action::Drop(ME)));
    }

    #[test]
    fn attaching_shows_the_network_as_it_stands() {
        let mut bouncer = registered();

        let lines = attach_modern(&mut bouncer, at("11:00:00"));

        assert!(lines.contains(
            &"CAP * LS :server-time message-tags batch echo-message multi-prefix".to_owned()
        ));
        assert!(
            lines.contains(
                &"CAP whatever ACK :server-time message-tags batch echo-message multi-prefix"
                    .to_owned()
            )
        );
        assert!(
            lines.contains(&":oxmacs 001 oxtest :Welcome to oxmacs, attached to libera".to_owned())
        );
        assert!(
            lines.contains(
                &":srv 005 oxtest PREFIX=(ov)@+ CHANTYPES=# NETWORK=Libera.Chat :are supported"
                    .to_owned()
            )
        );
        assert!(lines.contains(&":oxtest!oxmacs@oxmacs JOIN #rust".to_owned()));
        assert!(lines.contains(&":oxmacs 332 oxtest #rust :Rust talk".to_owned()));
        assert!(lines.contains(&":oxmacs 353 oxtest = #rust :@alice oxtest".to_owned()));
        assert!(lines.contains(&":oxmacs 366 oxtest #rust :End of /NAMES list.".to_owned()));
    }

    #[test]
    fn upstream_traffic_reaches_attached_clients() {
        let mut bouncer = registered();
        attach_modern(&mut bouncer, at("11:00:00"));

        upstream(
            &mut bouncer,
            &["@time=2026-10-19T11:01:00.000Z;msgid=x :alice!a@h PRIVMSG #rust :hi"],
            at("11:01:00"),
        );

        assert_eq!(
            parsed(&sent_to(&bouncer.take_actions(), ME)),
            parsed(&["@time=2026-10-19T11:01:00.000Z;msgid=x :alice!a@h PRIVMSG #rust :hi"])
        );
    }

    #[test]
    fn tags_are_trimmed_for_clients_that_did_not_ask() {
        let mut bouncer = registered();
        attach(
            &mut bouncer,
            &["PASS s3cret", "NICK x", "USER me/libera 0 * :me"],
            at("11:00:00"),
        );

        upstream(
            &mut bouncer,
            &["@time=2026-10-19T11:01:00.000Z;msgid=x :alice!a@h PRIVMSG #rust :hi"],
            at("11:01:00"),
        );

        assert_eq!(
            parsed(&sent_to(&bouncer.take_actions(), ME)),
            parsed(&[":alice!a@h PRIVMSG #rust :hi"])
        );
    }

    #[test]
    fn client_traffic_goes_upstream_and_quitting_only_detaches() {
        let mut bouncer = registered();
        attach_modern(&mut bouncer, at("11:00:00"));

        bouncer.client_line(ME, "PRIVMSG #rust :hello", at("11:02:00"));
        assert_eq!(
            sent_upstream(&bouncer.take_actions()),
            ["PRIVMSG #rust :hello"]
        );

        bouncer.client_line(ME, "QUIT :bye", at("11:03:00"));
        let actions = bouncer.take_actions();
        assert!(actions.contains(&Action::Drop(ME)));
        assert_eq!(sent_upstream(&actions), Vec::<String>::new());
    }

    #[test]
    fn missed_messages_are_replayed_in_a_history_batch() {
        let mut bouncer = registered();
        attach_modern(&mut bouncer, at("11:00:00"));
        bouncer.client_closed(ME, at("11:05:00"));
        upstream(
            &mut bouncer,
            &[
                "@time=2026-10-19T11:04:00.000Z :alice!a@h PRIVMSG #rust :seen already",
                "@time=2026-10-19T11:06:00.000Z :alice!a@h PRIVMSG #rust :while you were away",
            ],
            at("11:06:00"),
        );
        bouncer.take_actions();

        let lines = attach_modern(&mut bouncer, at("12:00:00"));

        let start = lines
            .iter()
            .position(|line| line.starts_with("BATCH +") && line.ends_with(" chathistory #rust"))
            .unwrap();
        let id = lines[start]
            .split(' ')
            .nth(1)
            .and_then(|batch| batch.strip_prefix('+'))
            .unwrap()
            .to_owned();
        assert_eq!(
            Message::parse(&lines[start + 1]),
            Message::parse(&format!(
                "@time=2026-10-19T11:06:00.000Z;batch={id} :alice!a@h PRIVMSG #rust :while you were away"
            ))
        );
        assert_eq!(lines[start + 2], format!("BATCH -{id}"));
    }

    #[test]
    fn clients_are_answered_locally_when_they_ping() {
        let mut bouncer = registered();
        attach_modern(&mut bouncer, at("11:00:00"));

        bouncer.client_line(ME, "PING :abc", at("11:01:00"));

        let actions = bouncer.take_actions();
        assert_eq!(sent_to(&actions, ME), [":oxmacs PONG oxmacs abc"]);
        assert_eq!(sent_upstream(&actions), Vec::<String>::new());
    }

    #[test]
    fn stop_quits_every_upstream_and_ends_the_bouncer() {
        let mut bouncer = registered();
        attach_modern(&mut bouncer, at("11:00:00"));

        bouncer.client_line(ME, "OXMACS STOP", at("11:01:00"));

        let actions = bouncer.take_actions();
        assert_eq!(sent_upstream(&actions), ["QUIT :oxmacs bouncer stopping"]);
        assert_eq!(actions.last(), Some(&Action::Stop));
    }

    #[test]
    fn lost_upstreams_reconnect_with_growing_patience() {
        let mut bouncer = registered();

        bouncer.upstream_event(0, Event::Closed(Some("reset".to_owned())), at("11:00:00"));
        assert_eq!(
            bouncer.take_actions(),
            [Action::Reconnect {
                network: 0,
                after: Duration::from_secs(10)
            }]
        );

        bouncer.upstream_event(0, Event::Closed(None), at("11:00:20"));
        assert_eq!(
            bouncer.take_actions(),
            [Action::Reconnect {
                network: 0,
                after: Duration::from_secs(20)
            }]
        );
    }
}
