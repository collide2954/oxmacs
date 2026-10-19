pub(super) mod commands;
pub(super) mod render;

use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::config::{IrcServer, PasswordSource};
use crate::display::LineStyle;
use crate::editor::buffers::Mode;
use crate::editor::chat::commands::Cycle;
use crate::editor::prompts::input_line::InputLine;
use crate::editor::prompts::minibuffer::Purpose;
use crate::editor::{Choice, Completion, ConnectionId, Editor, Effect};
use crate::external::net::{Endpoint, Event as NetEvent};
use crate::irc::message::Message;
use crate::irc::network::{Kind, LineEvent, Member, Network, Target, Update};
use crate::irc::session::{Event, Identity, Sasl, Session};
use crate::text::Buffer;
use crate::theme::Highlight;

const NO_SERVERS: &str = "Add an [[irc-server]] to config.toml with M-x find-config-file";
const DEFAULT_WIDTH: usize = 80;
const PROMPT: &str = " › ";

#[derive(Debug)]
pub(in crate::editor) struct Irc {
    links: Vec<Link>,
    next: u64,
    focus: Option<(String, Target)>,
    hide_members: bool,
    pub(in crate::editor) zone: TimeZone,
}

impl Default for Irc {
    fn default() -> Self {
        Self {
            links: Vec::new(),
            next: 0,
            focus: None,
            hide_members: false,
            zone: TimeZone::system(),
        }
    }
}

#[derive(Debug)]
struct Link {
    id: ConnectionId,
    server: IrcServer,
    session: Session,
    network: Network,
    open: bool,
}

#[derive(Debug)]
pub(in crate::editor) struct ChatView {
    server: String,
    target: Target,
    pub(in crate::editor) input: InputLine,
    styles: Vec<LineStyle>,
    cycle: Option<Cycle>,
    width: usize,
}

impl ChatView {
    pub(in crate::editor) fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }
}

fn label(server: &str, target: &Target) -> String {
    target.name().unwrap_or(server).to_owned()
}

impl Editor {
    pub(in crate::editor) fn irc(&mut self) {
        match self.config.irc_server.as_slice() {
            [] => self.show_message(NO_SERVERS),
            [server] => {
                let server = server.clone();
                self.connect_irc(&server);
            }
            _ => {
                self.open_minibuffer("IRC server: ", Purpose::IrcServer);
            }
        }
    }

    pub(in crate::editor) fn irc_server_completions(
        &self,
        matches: impl Fn(&str) -> bool,
    ) -> Vec<Completion> {
        self.config
            .irc_server
            .iter()
            .enumerate()
            .filter(|(_, server)| matches(&server.name))
            .map(|(index, server)| Completion {
                label: server.name.clone(),
                annotation: Some(format!("{}:{}", server.host, server.port)),
                summary: None,
                choice: Choice::IrcServer(index),
            })
            .collect()
    }

    pub(in crate::editor) fn choose_irc_server(&mut self, index: usize) {
        if let Some(server) = self.config.irc_server.get(index).cloned() {
            self.connect_irc(&server);
        }
    }

    fn connect_irc(&mut self, server: &IrcServer) {
        let existing = self
            .irc
            .links
            .iter()
            .position(|link| link.server.name == server.name);
        if existing.is_some_and(|index| self.irc.links[index].open) {
            self.show_chat(&server.name, &Target::Server);
            return;
        }
        let password = match irc_password(server) {
            Ok(password) => password,
            Err(problem) => {
                self.show_message(problem);
                return;
            }
        };
        let identity = Identity {
            nick: server.nick.clone(),
            username: server.username().to_owned(),
            realname: server.realname().to_owned(),
            sasl: server
                .sasl
                .clone()
                .zip(password)
                .map(|(account, password)| Sasl { account, password }),
        };
        let id = ConnectionId(self.irc.next);
        self.irc.next += 1;
        let session = Session::new(identity);
        if let Some(index) = existing {
            let link = &mut self.irc.links[index];
            link.id = id;
            link.session = session;
            link.server = server.clone();
            link.open = true;
        } else {
            self.irc.links.push(Link {
                id,
                server: server.clone(),
                session,
                network: Network::new(&server.name),
                open: true,
            });
        }
        self.effects.push(Effect::Connect(
            id,
            Endpoint {
                host: server.host.clone(),
                port: server.port,
                tls: server.tls,
            },
        ));
        let note = format!("Connecting to {}:{}…", server.host, server.port);
        self.irc_note(&server.name, &note, false);
        self.show_chat(&server.name, &Target::Server);
    }

    pub fn irc_event(&mut self, id: ConnectionId, event: NetEvent) {
        let Some(index) = self.irc.links.iter().position(|link| link.id == id) else {
            return;
        };
        let name = self.irc.links[index].server.name.clone();
        match event {
            NetEvent::Connected => {
                self.irc.links[index].session.start();
                self.irc_note(&name, "Connected", false);
            }
            NetEvent::Line(received) => {
                let link = &mut self.irc.links[index];
                link.session.receive(&received);
                for event in link.session.take_events() {
                    self.irc_session_event(index, event);
                }
            }
            NetEvent::Closed(problem) => {
                let link = &mut self.irc.links[index];
                link.open = false;
                link.network.disconnected();
                let text = problem.map_or_else(
                    || "Disconnected".to_owned(),
                    |problem| format!("Disconnected: {problem}"),
                );
                self.irc_note(&name, &text, true);
            }
        }
        self.irc_flush(index);
    }

    pub fn irc_farewell(&mut self) {
        for index in 0..self.irc.links.len() {
            if self.irc.links[index].open {
                self.irc_quit(index, "oxmacs");
            }
        }
    }

    fn irc_quit(&mut self, index: usize, reason: &str) {
        let link = &mut self.irc.links[index];
        link.open = false;
        link.session.send(&Message::new("QUIT", [reason]));
        let id = link.id;
        self.irc_flush(index);
        self.effects.push(Effect::Disconnect(id));
    }

    fn irc_open(&mut self, index: usize, target: &Target) {
        let link = &mut self.irc.links[index];
        let mut updates = Vec::new();
        link.network.ensure(target.clone(), &mut updates);
        let name = link.server.name.clone();
        self.irc_updates(index, &updates);
        self.show_chat(&name, target);
    }

    fn irc_help(&mut self, index: usize, target: &Target) {
        let link = &mut self.irc.links[index];
        if let Some(chat) = link.network.find(target) {
            link.network
                .note(chat, commands::HELP, false, Timestamp::now());
        }
        let name = link.server.name.clone();
        self.render_chat(&name, target);
    }

    fn irc_members(&self, server: &str, target: &Target) -> Vec<String> {
        let Some(link) = self
            .irc
            .links
            .iter()
            .find(|link| link.server.name == server)
        else {
            return Vec::new();
        };
        link.network
            .find(target)
            .and_then(|chat| link.network.chats().get(chat))
            .map(|chat| {
                chat.members(&link.session)
                    .into_iter()
                    .map(|member| member.nick)
                    .filter(|nick| !nick.eq_ignore_ascii_case(link.session.nick()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn irc_session_event(&mut self, index: usize, event: Event) {
        let name = self.irc.links[index].server.name.clone();
        match event {
            Event::Registered(_) => {
                let link = &mut self.irc.links[index];
                let channels = link.server.channels.join(",");
                if !channels.is_empty() {
                    link.session.send(&Message::new("JOIN", [channels]));
                }
            }
            Event::Authenticated(account) => {
                self.irc_note(&name, &format!("Logged in as {account}"), false);
            }
            Event::SaslFailed(text) => self.irc_note(&name, &text, true),
            Event::Message(message) => {
                let link = &mut self.irc.links[index];
                let updates = link
                    .network
                    .apply(&link.session, &message, Timestamp::now());
                self.irc_updates(index, &updates);
            }
        }
    }

    fn irc_updates(&mut self, index: usize, updates: &[Update]) {
        let mut chats: Vec<usize> = Vec::new();
        for update in updates {
            let (Update::Created(chat)
            | Update::Line(chat)
            | Update::Members(chat)
            | Update::Topic(chat)) = *update;
            if !chats.contains(&chat) {
                chats.push(chat);
            }
        }
        let link = &self.irc.links[index];
        let name = link.server.name.clone();
        let targets: Vec<Target> = chats
            .into_iter()
            .filter_map(|chat| link.network.chats().get(chat))
            .map(|chat| chat.target.clone())
            .collect();
        for target in targets {
            let buffer = self.ensure_chat_buffer(&name, &target);
            self.render_chat(&name, &target);
            self.announce(index, &target, buffer);
            let focused = self
                .irc
                .focus
                .as_ref()
                .is_some_and(|(server, wanted)| *server == name && wanted.same(&target));
            if focused {
                self.irc.focus = None;
                self.show_chat(&name, &target);
            }
        }
    }

    fn announce(&mut self, index: usize, target: &Target, buffer: usize) {
        let visible = self
            .buffers
            .get(buffer)
            .is_some_and(|buffer| self.window_showing(buffer.id()).is_some());
        let network = &self.irc.links[index].network;
        let Some(line) = network
            .find(target)
            .and_then(|chat| network.chats().get(chat))
            .and_then(|chat| chat.lines.last())
            .filter(|line| !visible && !line.history)
        else {
            return;
        };
        let LineEvent::Chat {
            nick,
            text,
            kind,
            own: false,
            mention,
        } = &line.event
        else {
            return;
        };
        let announcement = match target {
            Target::Query(_) if *kind != Kind::Notice => format!("{nick}: {text}"),
            Target::Channel(channel) if *mention => {
                format!("{nick} mentioned you in {channel}: {text}")
            }
            _ => return,
        };
        self.show_message(announcement);
    }

    pub(in crate::editor) fn read_visible_chats(&mut self) {
        let mut visible = self.visible_buffers();
        visible.push(0);
        for index in visible {
            let Some(Mode::Chat(view)) = self.buffers.mode(index) else {
                continue;
            };
            let Some(link) = self
                .irc
                .links
                .iter_mut()
                .find(|link| link.server.name == view.server)
            else {
                continue;
            };
            if let Some(chat) = link.network.find(&view.target) {
                link.network.mark_read(chat);
            }
        }
    }

    pub(in crate::editor) fn refill_chats(&mut self) {
        let mut stale = Vec::new();
        for index in 0..self.buffers.len() {
            let (Some(Mode::Chat(view)), Some(buffer)) =
                (self.buffers.mode(index), self.buffers.get(index))
            else {
                continue;
            };
            let width = self.width_showing(buffer.id()).filter(|width| *width > 0);
            if width.is_some_and(|width| width != view.width) {
                stale.push((view.server.clone(), view.target.clone()));
            }
        }
        for (server, target) in stale {
            self.render_chat(&server, &target);
        }
    }

    pub(in crate::editor) fn chat_members(&self, index: usize) -> Option<Vec<Member>> {
        let Some(Mode::Chat(view)) = self.buffers.mode(index) else {
            return None;
        };
        if self.irc.hide_members || !matches!(view.target, Target::Channel(_)) {
            return None;
        }
        let link = self
            .irc
            .links
            .iter()
            .find(|link| link.server.name == view.server)?;
        let chat = link.network.find(&view.target)?;
        Some(link.network.chats().get(chat)?.members(&link.session))
    }

    pub(in crate::editor) fn irc_toggle_members(&mut self) {
        self.irc.hide_members = !self.irc.hide_members;
    }

    pub(in crate::editor) fn irc_switch(&mut self) {
        self.open_minibuffer("IRC buffer: ", Purpose::IrcBuffer);
    }

    pub(in crate::editor) fn irc_buffer_completions(
        &self,
        matches: impl Fn(&str) -> bool,
    ) -> Vec<Completion> {
        let mut found: Vec<(usize, usize, Completion)> = Vec::new();
        for index in 0..self.buffers.len() {
            let (Some(Mode::Chat(view)), Some(buffer)) =
                (self.buffers.mode(index), self.buffers.get(index))
            else {
                continue;
            };
            if !matches(buffer.name()) {
                continue;
            }
            let (unread, mentions) = self
                .irc
                .links
                .iter()
                .find(|link| link.server.name == view.server)
                .and_then(|link| {
                    let chat = link.network.find(&view.target)?;
                    link.network.chats().get(chat)
                })
                .map_or((0, 0), |chat| (chat.unread, chat.mentions));
            let mut counts = Vec::new();
            if unread > 0 {
                counts.push(format!("{unread} new"));
            }
            match mentions {
                0 => {}
                1 => counts.push("1 mention".to_owned()),
                many => counts.push(format!("{many} mentions")),
            }
            found.push((
                mentions,
                unread,
                Completion {
                    label: buffer.name().to_owned(),
                    annotation: (!counts.is_empty()).then(|| counts.join(" · ")),
                    summary: None,
                    choice: Choice::Buffer(index),
                },
            ));
        }
        found.sort_by_key(|(mentions, unread, _)| {
            (std::cmp::Reverse(*mentions), std::cmp::Reverse(*unread))
        });
        found
            .into_iter()
            .map(|(_, _, completion)| completion)
            .collect()
    }

    fn irc_note(&mut self, server: &str, text: &str, error: bool) {
        if let Some(link) = self
            .irc
            .links
            .iter_mut()
            .find(|link| link.server.name == server)
        {
            link.network.note(0, text, error, Timestamp::now());
        }
        self.ensure_chat_buffer(server, &Target::Server);
        self.render_chat(server, &Target::Server);
    }

    fn irc_flush(&mut self, index: usize) {
        let link = &mut self.irc.links[index];
        let lines = link.session.take_outgoing();
        if !lines.is_empty() {
            self.effects.push(Effect::Transmit(link.id, lines));
        }
    }

    fn chat_buffer(&self, server: &str, target: &Target) -> Option<usize> {
        self.buffers.find_mode(|mode| {
            matches!(mode, Mode::Chat(view) if view.server == server && view.target.same(target))
        })
    }

    fn ensure_chat_buffer(&mut self, server: &str, target: &Target) -> usize {
        if let Some(index) = self.chat_buffer(server, target) {
            return index;
        }
        let base = label(server, target);
        let name = if self
            .buffers
            .position(|buffer| buffer.name() == base)
            .is_some()
        {
            format!("{base}<{server}>")
        } else {
            base
        };
        let mut buffer = Buffer::new(name);
        let input = InputLine::new(&mut buffer);
        self.buffers.push_back_with(
            buffer,
            Mode::Chat(ChatView {
                server: server.to_owned(),
                target: target.clone(),
                input,
                styles: Vec::new(),
                cycle: None,
                width: 0,
            }),
        )
    }

    fn show_chat(&mut self, server: &str, target: &Target) {
        let index = self.ensure_chat_buffer(server, target);
        self.select_buffer(index);
    }

    fn render_chat(&mut self, server: &str, target: &Target) {
        let Some(index) = self.chat_buffer(server, target) else {
            return;
        };
        let Some(link) = self
            .irc
            .links
            .iter()
            .find(|link| link.server.name == server)
        else {
            return;
        };
        let Some(chat) = link
            .network
            .find(target)
            .and_then(|chat| link.network.chats().get(chat))
        else {
            return;
        };
        let width = self
            .buffers
            .get(index)
            .and_then(|buffer| self.width_showing(buffer.id()))
            .filter(|width| *width > 0)
            .unwrap_or(DEFAULT_WIDTH);
        let page = render::page(chat, width, &self.irc.zone);
        let name = label(server, target);
        let shown = format!("{}{name}{PROMPT}", page.text);
        let Some((buffer, Mode::Chat(view))) = self.buffers.entry_mut(index) else {
            return;
        };
        let old_start = view
            .input
            .start(buffer)
            .unwrap_or_else(|| buffer.len_chars());
        let old = buffer.text_in(0..old_start);
        let prefix = old
            .chars()
            .zip(shown.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let tail: String = shown.chars().skip(prefix).collect();
        buffer.splice(prefix..old_start, &tail);
        let start = shown.chars().count();
        buffer.set_marker(view.input.mark(), start);
        if buffer.point() == prefix && prefix < start {
            buffer.set_point(start);
        }
        let mut styles = page.styles;
        styles.push(LineStyle {
            background: None,
            spans: vec![(0..name.chars().count(), Highlight::Heading)],
        });
        view.styles = styles;
        view.width = width;
        let id = buffer.id();
        self.move_windows(id, |point, _| {
            (point == prefix && prefix < start).then_some(start)
        });
    }

    pub(in crate::editor) fn irc_send_input(&mut self) {
        let Some((buffer, Mode::Chat(view))) = self.buffers.entry_mut(0) else {
            return;
        };
        let Some(text) = view.input.text(buffer) else {
            return;
        };
        view.input.remember(&text);
        view.input.replace(buffer, "");
        let (server, target) = (view.server.clone(), view.target.clone());
        if text.trim().is_empty() {
            return;
        }
        let Some(index) = self
            .irc
            .links
            .iter()
            .position(|link| link.server.name == server && link.open)
        else {
            self.show_message(format!("Not connected to {server}; M-x irc reconnects"));
            return;
        };
        if let Some(command) = text.strip_prefix('/').filter(|rest| !rest.starts_with('/')) {
            self.irc_command(index, &target, command.trim());
            self.irc_flush(index);
            return;
        }
        let text = text.strip_prefix('/').unwrap_or(&text);
        let Some(name) = target.name().map(str::to_owned) else {
            self.show_message("Type a /command here, such as /join #channel");
            return;
        };
        for line in text.lines().filter(|line| !line.is_empty()) {
            self.irc_say(index, &name, line);
        }
        self.irc_flush(index);
    }

    fn irc_say(&mut self, index: usize, target: &str, text: &str) {
        let link = &mut self.irc.links[index];
        let message = Message::new("PRIVMSG", [target, text]);
        link.session.send(&message);
        if link.session.has_cap("echo-message") {
            return;
        }
        let echo = Message {
            source: Some(format!("{}!local", link.session.nick())),
            ..message
        };
        let updates = link.network.apply(&link.session, &echo, Timestamp::now());
        self.irc_updates(index, &updates);
    }
}

fn irc_password(server: &IrcServer) -> Result<Option<String>, String> {
    if server.sasl.is_none() {
        return Ok(None);
    }
    match &server.password {
        PasswordSource::Text(text) => Ok(Some(text.clone())),
        PasswordSource::Env(variable) => std::env::var(variable)
            .map(Some)
            .map_err(|_| format!("Set {variable} to the password for {}", server.name)),
        PasswordSource::Ask | PasswordSource::Keychain => Err(format!(
            "{}: asking for passwords and the keychain are not ready yet; use password = {{ env = \"NAME\" }}",
            server.name
        )),
    }
}
#[cfg(test)]
mod tests {
    use jiff::tz::TimeZone;

    use crate::config::Config;
    use crate::editor::test_support::{labels, lines, press, type_text};
    use crate::editor::{ConnectionId, Editor, Effect};
    use crate::external::net::{Endpoint, Event};

    const LIBERA: &str = "[[irc-server]]\nname = \"libera\"\nhost = \"irc.libera.chat\"\nnick = \"oxtest\"\nchannels = [\"#oxmacs-test\"]\n";

    fn configured(text: &str) -> Editor {
        let mut editor = Editor::new();
        editor.apply_config(&Config::parse(text).unwrap());
        editor.irc.zone = TimeZone::UTC;
        editor
    }

    fn irc(editor: &mut Editor) {
        press(editor, "M-x");
        type_text(editor, "irc");
        press(editor, "RET");
    }

    fn connections(editor: &mut Editor) -> Vec<(ConnectionId, Endpoint)> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Connect(id, endpoint) => Some((id, endpoint)),
                _ => None,
            })
            .collect()
    }

    fn sent(editor: &mut Editor) -> Vec<String> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Transmit(_, lines) => Some(lines),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn feed(editor: &mut Editor, id: ConnectionId, received: &[&str]) {
        for line in received {
            editor.irc_event(id, Event::Line((*line).to_owned()));
        }
    }

    fn connected_with(caps: &str) -> (Editor, ConnectionId) {
        let mut editor = configured(LIBERA);
        irc(&mut editor);
        let id = connections(&mut editor)[0].0;
        editor.irc_event(id, Event::Connected);
        feed(&mut editor, id, &[&format!(":srv CAP * LS :{caps}")]);
        if !caps.is_empty() {
            feed(&mut editor, id, &[&format!(":srv CAP oxtest ACK :{caps}")]);
        }
        feed(
            &mut editor,
            id,
            &[":srv 001 oxtest :Welcome", ":oxtest!o@h JOIN #oxmacs-test"],
        );
        sent(&mut editor);
        (editor, id)
    }

    fn connected() -> (Editor, ConnectionId) {
        connected_with("")
    }

    fn visit(editor: &mut Editor, name: &str) {
        press(editor, "C-x b");
        type_text(editor, name);
        press(editor, "RET");
    }

    fn shows(editor: &Editor, text: &str) -> bool {
        lines(editor).iter().any(|line| line.contains(text))
    }

    #[test]
    fn m_x_irc_connects_to_the_configured_server() {
        let mut editor = configured(LIBERA);

        irc(&mut editor);

        assert_eq!(
            connections(&mut editor),
            [(
                ConnectionId(0),
                Endpoint {
                    host: "irc.libera.chat".to_owned(),
                    port: 6697,
                    tls: true
                }
            )]
        );
        assert_eq!(editor.buffer().name(), "libera");
        assert!(shows(&editor, "Connecting to irc.libera.chat:6697"));
    }

    #[test]
    fn without_servers_m_x_irc_explains_how_to_add_one() {
        let mut editor = configured("");

        irc(&mut editor);

        assert_eq!(
            editor.message(),
            Some("Add an [[irc-server]] to config.toml with M-x find-config-file")
        );
        assert_eq!(connections(&mut editor), []);
    }

    #[test]
    fn a_connection_registers_then_joins_the_configured_channels() {
        let mut editor = configured(LIBERA);
        irc(&mut editor);
        let id = connections(&mut editor)[0].0;

        editor.irc_event(id, Event::Connected);
        assert_eq!(
            sent(&mut editor),
            ["CAP LS 302", "NICK oxtest", "USER oxtest 0 * oxmacs"]
        );

        feed(&mut editor, id, &[":srv CAP * LS :"]);
        assert_eq!(sent(&mut editor), ["CAP END"]);

        feed(&mut editor, id, &[":srv 001 oxtest :Welcome"]);
        assert_eq!(sent(&mut editor), ["JOIN #oxmacs-test"]);
    }

    #[test]
    fn joined_channels_get_their_own_buffer_with_a_prompt() {
        let (mut editor, _) = connected();

        visit(&mut editor, "#oxmacs-test");

        assert_eq!(lines(&editor).last().unwrap(), "#oxmacs-test › ");
        assert_eq!(editor.buffer().point(), editor.buffer().len_chars());
    }

    #[test]
    fn messages_show_in_the_channel_buffer() {
        let (mut editor, id) = connected();

        feed(
            &mut editor,
            id,
            &["@time=2026-10-19T12:00:00.000Z :alice!a@h PRIVMSG #oxmacs-test :hi there"],
        );
        visit(&mut editor, "#oxmacs-test");

        assert!(
            shows(&editor, "12:00 alice │ hi there"),
            "{:?}",
            lines(&editor)
        );
    }

    #[test]
    fn return_sends_what_you_typed_and_shows_it() {
        let (mut editor, _) = connected();
        visit(&mut editor, "#oxmacs-test");

        type_text(&mut editor, "hello");
        press(&mut editor, "RET");

        assert_eq!(sent(&mut editor), ["PRIVMSG #oxmacs-test hello"]);
        assert!(shows(&editor, "oxtest │ hello"));
        assert_eq!(lines(&editor).last().unwrap(), "#oxmacs-test › ");
    }

    #[test]
    fn echoed_messages_are_shown_once() {
        let (mut editor, id) = connected_with("echo-message");
        visit(&mut editor, "#oxmacs-test");

        type_text(&mut editor, "hello");
        press(&mut editor, "RET");
        assert!(!shows(&editor, "│ hello"));

        feed(
            &mut editor,
            id,
            &[":oxtest!o@h PRIVMSG #oxmacs-test :hello"],
        );
        let count = lines(&editor)
            .iter()
            .filter(|line| line.contains("│ hello"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn earlier_input_is_recalled() {
        let (mut editor, _) = connected();
        visit(&mut editor, "#oxmacs-test");
        for input in ["first", "second"] {
            type_text(&mut editor, input);
            press(&mut editor, "RET");
        }

        press(&mut editor, "M-p");
        assert_eq!(lines(&editor).last().unwrap(), "#oxmacs-test › second");
        press(&mut editor, "M-p");
        assert_eq!(lines(&editor).last().unwrap(), "#oxmacs-test › first");
    }

    #[test]
    fn private_messages_open_a_query_buffer() {
        let (mut editor, id) = connected();

        feed(&mut editor, id, &[":alice!a@h PRIVMSG oxtest :psst"]);

        assert!(editor.buffer_names().contains(&"alice"));
    }

    #[test]
    fn disconnection_is_shown_in_the_server_buffer() {
        let (mut editor, id) = connected();

        editor.irc_event(id, Event::Closed(Some("connection reset".to_owned())));
        visit(&mut editor, "libera");

        assert!(shows(&editor, "Disconnected: connection reset"));
    }

    #[test]
    fn m_x_irc_while_connected_shows_the_server_buffer() {
        let (mut editor, _) = connected();
        visit(&mut editor, "#oxmacs-test");

        irc(&mut editor);

        assert_eq!(connections(&mut editor), []);
        assert_eq!(editor.buffer().name(), "libera");
    }

    #[test]
    fn chats_rewrap_when_their_window_is_resized() {
        let (mut editor, id) = connected();
        feed(
            &mut editor,
            id,
            &[
                ":alice!a@h PRIVMSG #oxmacs-test :one two three four five six seven eight nine ten eleven",
            ],
        );
        visit(&mut editor, "#oxmacs-test");

        editor.resize_pane(editor.selected_window(), 20, 41);

        let shown = lines(&editor);
        assert!(
            shown.iter().all(|line| line.chars().count() <= 40),
            "{shown:?}"
        );
        assert!(shown.iter().filter(|line| line.contains(" │ ")).count() > 1);
    }

    #[test]
    fn mentions_elsewhere_are_announced() {
        let (mut editor, id) = connected();

        feed(
            &mut editor,
            id,
            &[":alice!a@h PRIVMSG #oxmacs-test :oxtest: ping"],
        );
        assert_eq!(
            editor.message(),
            Some("alice mentioned you in #oxmacs-test: oxtest: ping")
        );

        feed(&mut editor, id, &[":bob!b@h PRIVMSG oxtest :psst"]);
        assert_eq!(editor.message(), Some("bob: psst"));
    }

    #[test]
    fn mentions_in_view_are_not_announced_and_count_as_read() {
        let (mut editor, id) = connected();
        visit(&mut editor, "#oxmacs-test");
        editor.show_message("quiet");

        feed(
            &mut editor,
            id,
            &[":alice!a@h PRIVMSG #oxmacs-test :oxtest: ping"],
        );
        editor.prepare_display();

        assert_eq!(editor.message(), Some("quiet"));
        press(&mut editor, "C-c C-b");
        let completions = editor.completions();
        let channel = completions
            .iter()
            .find(|completion| completion.label == "#oxmacs-test")
            .unwrap();
        assert_eq!(channel.annotation, None);
    }

    #[test]
    fn the_switcher_puts_busy_chats_first() {
        let (mut editor, id) = connected();
        feed(
            &mut editor,
            id,
            &[
                ":oxtest!o@h JOIN #quiet",
                ":alice!a@h PRIVMSG #oxmacs-test :hello",
                ":alice!a@h PRIVMSG #oxmacs-test :oxtest: you there?",
            ],
        );
        visit(&mut editor, "libera");

        press(&mut editor, "C-c C-b");

        assert_eq!(editor.minibuffer().unwrap().prompt(), "IRC buffer: ");
        let completions = editor.completions();
        assert_eq!(completions[0].label, "#oxmacs-test");
        assert_eq!(
            completions[0].annotation.as_deref(),
            Some("2 new · 1 mention")
        );
        type_text(&mut editor, "#oxm");
        press(&mut editor, "RET");
        assert_eq!(editor.buffer().name(), "#oxmacs-test");
    }

    #[test]
    fn channel_panes_list_their_members_until_toggled_off() {
        let (mut editor, id) = connected();
        feed(
            &mut editor,
            id,
            &[
                ":srv 353 oxtest = #oxmacs-test :alice @bob oxtest",
                ":srv 366 oxtest #oxmacs-test :End",
            ],
        );
        visit(&mut editor, "#oxmacs-test");

        let members: Vec<String> = editor
            .pane()
            .members()
            .unwrap()
            .iter()
            .map(|member| format!("{}{}", member.prefixes, member.nick))
            .collect();
        assert_eq!(members, ["@bob", "alice", "oxtest"]);

        press(&mut editor, "C-c C-n");
        assert_eq!(editor.pane().members(), None);
        press(&mut editor, "C-c C-n");
        visit(&mut editor, "libera");
        assert_eq!(editor.pane().members(), None);
    }

    #[test]
    fn leaving_says_goodbye_to_every_open_server() {
        let (mut editor, id) = connected();

        editor.irc_farewell();

        let effects = editor.take_effects();
        assert!(effects.contains(&Effect::Transmit(id, vec!["QUIT oxmacs".to_owned()])));
        assert!(effects.contains(&Effect::Disconnect(id)));
        editor.irc_farewell();
        assert_eq!(editor.take_effects(), []);
    }

    #[test]
    fn several_servers_are_chosen_by_name() {
        let mut editor = configured(&format!(
            "{LIBERA}[[irc-server]]\nname = \"local\"\nhost = \"localhost\"\nport = 6667\ntls = false\nnick = \"me\"\n"
        ));

        irc(&mut editor);
        assert_eq!(editor.minibuffer().unwrap().prompt(), "IRC server: ");
        assert_eq!(labels(&editor), ["libera", "local"]);
        type_text(&mut editor, "local");
        press(&mut editor, "RET");

        assert_eq!(connections(&mut editor)[0].1.host, "localhost");
    }
}
