pub(crate) mod commands;
pub(crate) mod connect;
pub(crate) mod render;
pub(crate) mod setup;

use std::ops::{Deref, DerefMut};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::{ConnectionId, Editor, InputLine, Offer};
use oxmacs_core::external::net::Event as NetEvent;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Highlight;

use crate::bouncer::daemon::LocalDaemon;
use crate::keychain;
use crate::protocol::message::Message;
use crate::protocol::network::{Kind, LineEvent, Member, Network, Target, Update};
use crate::protocol::session::{Event, Session};
use crate::settings::{self, BouncerMode, IrcServer, IrcSettings};
use commands::Cycle;

const DEFAULT_WIDTH: usize = 80;
const PROMPT: &str = " › ";

#[derive(Debug)]
pub(crate) struct IrcState {
    pub(crate) chat_scope: ScopeId,
    pub(crate) setup_scope: ScopeId,
    links: Vec<Link>,
    focus: Option<(String, Target)>,
    hide_members: bool,
    pub(crate) zone: TimeZone,
    pub(crate) keychain: Keychain,
    pub(crate) local_daemon: fn() -> Option<LocalDaemon>,
    waiting: Vec<String>,
    launching: bool,
    relaunched: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Keychain {
    pub(crate) read: fn(&str, &str) -> Result<Option<String>, String>,
    pub(crate) write: fn(&str, &str, &str) -> Result<(), String>,
}

impl IrcState {
    pub(crate) fn new(chat_scope: ScopeId, setup_scope: ScopeId) -> Self {
        Self {
            chat_scope,
            setup_scope,
            links: Vec::new(),
            focus: None,
            hide_members: false,
            zone: TimeZone::system(),
            keychain: Keychain {
                read: keychain::read,
                write: keychain::write,
            },
            local_daemon: || LocalDaemon::file().and_then(|file| LocalDaemon::read(&file)),
            waiting: Vec::new(),
            launching: false,
            relaunched: false,
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
    connected: bool,
    via_bouncer: bool,
}

#[derive(Debug)]
pub(crate) struct ChatView {
    scope: ScopeId,
    server: String,
    target: Target,
    input: InputLine,
    styles: Vec<LineStyle>,
    cycle: Option<Cycle>,
    width: usize,
}

impl AppletBuffer for ChatView {
    fn scope(&self) -> ScopeId {
        self.scope
    }

    fn line_style(&self, line: usize) -> LineStyle {
        self.styles.get(line).cloned().unwrap_or_default()
    }

    fn input_line(&mut self) -> Option<&mut InputLine> {
        Some(&mut self.input)
    }
}

pub(crate) struct Chat<'a> {
    editor: &'a mut Editor,
    pub(crate) irc: IrcState,
}

impl Deref for Chat<'_> {
    type Target = Editor;

    fn deref(&self) -> &Editor {
        self.editor
    }
}

impl DerefMut for Chat<'_> {
    fn deref_mut(&mut self) -> &mut Editor {
        self.editor
    }
}

pub(crate) fn chat<'a, R>(
    editor: &'a mut Editor,
    work: impl FnOnce(&mut Chat<'a>) -> R,
) -> Option<R> {
    let irc = editor.take_applet_state::<IrcState>()?;
    let mut chat = Chat { editor, irc };
    let result = work(&mut chat);
    chat.editor.set_applet_state(chat.irc);
    Some(result)
}

fn label(server: &str, target: &Target) -> String {
    target.name().unwrap_or(server).to_owned()
}

pub(crate) fn irc_settings(editor: &Editor) -> IrcSettings {
    editor.applet_config(settings::TABLE)
}

pub(crate) fn members(editor: &Editor, view: &ChatView) -> Option<(Vec<Member>, String)> {
    let irc = editor.applet_state::<IrcState>()?;
    if irc.hide_members || !matches!(view.target, Target::Channel(_)) {
        return None;
    }
    let link = irc
        .links
        .iter()
        .find(|link| link.server.name == view.server)?;
    let chat = link.network.find(&view.target)?;
    let members = link.network.chats().get(chat)?.members(&link.session);
    Some((members, link.session.nick().to_owned()))
}

fn buffer_offers(editor: &Editor) -> Vec<Offer> {
    let Some(irc) = editor.applet_state::<IrcState>() else {
        return Vec::new();
    };
    let mut found: Vec<(usize, usize, Offer)> = Vec::new();
    for id in editor.applet_buffers::<ChatView>() {
        let Some((buffer, view)) = editor.applet_buffer::<ChatView>(id) else {
            continue;
        };
        let (unread, mentions) = irc
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
            Offer {
                label: buffer.name().to_owned(),
                note: (!counts.is_empty()).then(|| counts.join(" · ")),
            },
        ));
    }
    found.sort_by_key(|(mentions, unread, _)| {
        (std::cmp::Reverse(*mentions), std::cmp::Reverse(*unread))
    });
    found.into_iter().map(|(_, _, offer)| offer).collect()
}

impl Chat<'_> {
    pub(crate) fn irc_settings(&self) -> IrcSettings {
        irc_settings(self)
    }

    pub(crate) fn irc(&mut self) {
        match self.irc_settings().server.as_slice() {
            [] => self.ask("No IRC servers yet; set one up?", |editor, yes| {
                if yes {
                    chat(editor, Chat::irc_setup);
                }
            }),
            [server] => {
                let server = server.clone();
                self.connect_irc(&server);
            }
            _ => self.prompt_with(
                "IRC server: ",
                |editor| {
                    irc_settings(editor)
                        .server
                        .iter()
                        .map(|server| {
                            Offer::noted(&server.name, format!("{}:{}", server.host, server.port))
                        })
                        .collect()
                },
                |editor, name| {
                    chat(editor, |chat| chat.connect_named_irc(&name));
                },
            ),
        }
    }

    pub(crate) fn irc_event(&mut self, id: ConnectionId, event: NetEvent) {
        let Some(index) = self.irc.links.iter().position(|link| link.id == id) else {
            return;
        };
        let name = self.irc.links[index].server.name.clone();
        match event {
            NetEvent::Connected => {
                let link = &mut self.irc.links[index];
                link.connected = true;
                link.session.start();
                self.irc.relaunched = false;
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
                let local = self.irc_settings().bouncer.mode == BouncerMode::Local;
                let link = &mut self.irc.links[index];
                link.open = false;
                link.network.disconnected();
                let stale = link.via_bouncer && !link.connected && local && !self.irc.relaunched;
                if stale {
                    self.irc.relaunched = true;
                    let server = self.irc.links[index].server.clone();
                    self.launch_daemon(&server);
                    return;
                }
                let text = problem.map_or_else(
                    || "Disconnected".to_owned(),
                    |problem| format!("Disconnected: {problem}"),
                );
                self.irc_note(&name, &text, true);
            }
        }
        self.irc_flush(index);
    }

    pub(crate) fn irc_farewell(&mut self) {
        for index in 0..self.irc.links.len() {
            if self.irc.links[index].open {
                self.irc_quit(index, "oxmacs");
            }
        }
    }

    pub(crate) fn irc_disconnect(&mut self, id: ConnectionId) {
        if let Some(index) = self
            .irc
            .links
            .iter()
            .position(|link| link.id == id && link.open)
        {
            self.irc_quit(index, "oxmacs");
        }
    }

    pub(crate) fn visit_irc(&mut self, id: ConnectionId) {
        if let Some(name) = self
            .irc
            .links
            .iter()
            .find(|link| link.id == id)
            .map(|link| link.server.name.clone())
        {
            self.show_irc_server(&name);
        }
    }

    fn show_irc_server(&mut self, name: &str) {
        let id = self.ensure_chat_buffer(name, &Target::Server);
        self.show_buffer_beside(id);
    }

    fn irc_quit(&mut self, index: usize, reason: &str) {
        let link = &mut self.irc.links[index];
        link.open = false;
        link.session.send(&Message::new("QUIT", [reason]));
        let id = link.id;
        self.irc_flush(index);
        self.disconnect(id);
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

    fn announce(&mut self, index: usize, target: &Target, buffer: BufferId) {
        let visible = self.buffer_shown(buffer);
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

    pub(crate) fn read_visible_chats(&mut self) {
        for id in self.applet_buffers::<ChatView>() {
            if !self.buffer_shown(id) {
                continue;
            }
            let Some((server, target)) = self
                .applet_buffer::<ChatView>(id)
                .map(|(_, view)| (view.server.clone(), view.target.clone()))
            else {
                continue;
            };
            let Some(link) = self
                .irc
                .links
                .iter_mut()
                .find(|link| link.server.name == server)
            else {
                continue;
            };
            if let Some(chat) = link.network.find(&target) {
                link.network.mark_read(chat);
            }
        }
    }

    pub(crate) fn refill_chats(&mut self) {
        let mut stale = Vec::new();
        for id in self.applet_buffers::<ChatView>() {
            let width = self.buffer_width(id);
            let Some((_, view)) = self.applet_buffer::<ChatView>(id) else {
                continue;
            };
            if width.is_some_and(|width| width != view.width) {
                stale.push((view.server.clone(), view.target.clone()));
            }
        }
        for (server, target) in stale {
            self.render_chat(&server, &target);
        }
    }

    pub(crate) fn irc_toggle_members(&mut self) {
        self.irc.hide_members = !self.irc.hide_members;
    }

    pub(crate) fn irc_switch(&mut self) {
        self.prompt_with("IRC buffer: ", buffer_offers, |editor, name| {
            match editor.buffer_named(&name) {
                Some(id) => editor.show_buffer(id),
                None => editor.show_message(format!("No IRC buffer named {name}")),
            }
        });
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
        let (id, lines) = (link.id, link.session.take_outgoing());
        self.transmit(id, lines);
    }

    fn chat_buffer(&self, server: &str, target: &Target) -> Option<BufferId> {
        self.find_applet_buffer::<ChatView>(|view| {
            view.server == server && view.target.same(target)
        })
    }

    fn ensure_chat_buffer(&mut self, server: &str, target: &Target) -> BufferId {
        if let Some(id) = self.chat_buffer(server, target) {
            return id;
        }
        let base = label(server, target);
        let name = if self.buffer_named(&base).is_some() {
            format!("{base}<{server}>")
        } else {
            base
        };
        let scope = self.irc.chat_scope;
        let (server, target) = (server.to_owned(), target.clone());
        self.add_applet_buffer_with(&name, move |buffer| ChatView {
            scope,
            server,
            target,
            input: InputLine::new(buffer),
            styles: Vec::new(),
            cycle: None,
            width: 0,
        })
    }

    fn show_chat(&mut self, server: &str, target: &Target) {
        let id = self.ensure_chat_buffer(server, target);
        self.show_buffer(id);
    }

    fn render_chat(&mut self, server: &str, target: &Target) {
        let Some(id) = self.chat_buffer(server, target) else {
            return;
        };
        let width = self.buffer_width(id).unwrap_or(DEFAULT_WIDTH);
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
        let (text, mut styles) = render::page(chat, width, &self.irc.zone).into_parts();
        let name = label(server, target);
        let shown = format!("{text}{name}{PROMPT}");
        let Some((buffer, view)) = self.applet_buffer_mut::<ChatView>(id) else {
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
        styles.push(LineStyle {
            background: None,
            spans: vec![(0..name.chars().count(), Highlight::Heading)],
        });
        view.styles = styles;
        view.width = width;
        self.move_points(id, |point, _| {
            (point == prefix && prefix < start).then_some(start)
        });
    }

    pub(crate) fn irc_send_input(&mut self) {
        let id = self.buffer().id();
        let Some((buffer, view)) = self.applet_buffer_mut::<ChatView>(id) else {
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
#[cfg(test)]
mod tests {
    use oxmacs_core::config::Config;
    use oxmacs_core::editor::{BufferState, ConnectionId, Editor, Effect};
    use oxmacs_core::external::net::{Endpoint, Event};

    use super::ChatView;
    use crate::bouncer::daemon::LocalDaemon;
    use crate::testing::{labels, lines, press, state, type_text};
    use crate::{DaemonReady, LaunchDaemon};

    const LIBERA: &str = "[[irc.server]]\nname = \"libera\"\nhost = \"irc.libera.chat\"\nnick = \"oxtest\"\nchannels = [\"#oxmacs-test\"]\n";

    fn configured(text: &str) -> Editor {
        let mut editor = crate::testing::editor();
        editor.apply_config(&Config::parse(text).unwrap());
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
            editor.connection_event(id, Event::Line((*line).to_owned()));
        }
    }

    fn connected_with(caps: &str) -> (Editor, ConnectionId) {
        let mut editor = configured(LIBERA);
        irc(&mut editor);
        let id = connections(&mut editor)[0].0;
        editor.connection_event(id, Event::Connected);
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

    fn members(editor: &Editor) -> Option<(Vec<String>, String)> {
        let view = editor.pane().applet_buffer::<ChatView>()?;
        let (members, own) = super::members(editor, view)?;
        let shown = members
            .iter()
            .map(|member| format!("{}{}", member.prefixes, member.nick))
            .collect();
        Some((shown, own))
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
    fn without_servers_m_x_irc_offers_the_setup() {
        let mut editor = configured("");

        irc(&mut editor);

        assert_eq!(
            crate::testing::question(&editor),
            Some("No IRC servers yet; set one up? (y or n) ".to_owned())
        );
        assert_eq!(connections(&mut editor), []);
        press(&mut editor, "y");
        assert_eq!(editor.buffer().name(), "*IRC Setup*");
    }

    #[test]
    fn a_connection_registers_then_joins_the_configured_channels() {
        let mut editor = configured(LIBERA);
        irc(&mut editor);
        let id = connections(&mut editor)[0].0;

        editor.connection_event(id, Event::Connected);
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

        editor.connection_event(id, Event::Closed(Some("connection reset".to_owned())));
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
        editor.prepare_display();

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

        let (shown, own) = members(&editor).unwrap();
        assert_eq!(shown, ["@bob", "alice", "oxtest"]);
        assert_eq!(own, "oxtest");

        press(&mut editor, "C-c C-n");
        assert_eq!(members(&editor), None);
        press(&mut editor, "C-c C-n");
        visit(&mut editor, "libera");
        assert_eq!(members(&editor), None);
    }

    #[test]
    fn drafts_do_not_mark_chats_modified() {
        let (mut editor, _) = connected();
        visit(&mut editor, "#oxmacs-test");

        type_text(&mut editor, "draft");

        assert_eq!(editor.pane().status().state, BufferState::Unmodified);
    }

    fn with_sasl(password: &str) -> Editor {
        configured(&format!(
            "[[irc.server]]\nname = \"libera\"\nhost = \"irc.libera.chat\"\nnick = \"oxtest\"\nsasl = \"oxtest\"\npassword = {password}\n"
        ))
    }

    fn authenticates_with(editor: &mut Editor, id: ConnectionId) -> Vec<String> {
        editor.connection_event(id, Event::Connected);
        feed(
            editor,
            id,
            &[
                ":srv CAP * LS :sasl",
                ":srv CAP oxtest ACK :sasl",
                "AUTHENTICATE +",
            ],
        );
        sent(editor)
    }

    #[test]
    fn asked_passwords_are_typed_unseen_then_used() {
        let mut editor = with_sasl("\"ask\"");

        irc(&mut editor);
        assert_eq!(connections(&mut editor), []);
        let minibuffer = editor.minibuffer().unwrap();
        assert_eq!(minibuffer.prompt(), "Password for oxtest on libera: ");
        assert!(minibuffer.is_secret());
        type_text(&mut editor, "hunter2");
        press(&mut editor, "RET");

        let id = connections(&mut editor)[0].0;
        assert!(
            authenticates_with(&mut editor, id)
                .contains(&"AUTHENTICATE b3h0ZXN0AG94dGVzdABodW50ZXIy".to_owned())
        );
    }

    #[test]
    fn keychain_passwords_connect_at_once() {
        let mut editor = with_sasl("\"keychain\"");
        state(&mut editor).keychain.read = |server, account| {
            assert_eq!((server, account), ("libera", "oxtest"));
            Ok(Some("hunter2".to_owned()))
        };

        irc(&mut editor);

        assert!(editor.minibuffer().is_none());
        let id = connections(&mut editor)[0].0;
        assert!(
            authenticates_with(&mut editor, id)
                .contains(&"AUTHENTICATE b3h0ZXN0AG94dGVzdABodW50ZXIy".to_owned())
        );
    }

    std::thread_local! {
        static STORED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    #[test]
    fn missing_keychain_passwords_are_asked_for_and_kept() {
        let mut editor = with_sasl("\"keychain\"");
        state(&mut editor).keychain.read = |_, _| Ok(None);
        state(&mut editor).keychain.write = |server, account, password| {
            STORED.with(|stored| {
                stored
                    .borrow_mut()
                    .push(format!("{account}@{server}={password}"));
            });
            Ok(())
        };

        irc(&mut editor);
        assert!(editor.minibuffer().unwrap().is_secret());
        type_text(&mut editor, "hunter2");
        press(&mut editor, "RET");

        assert_eq!(connections(&mut editor).len(), 1);
        STORED.with(|stored| assert_eq!(*stored.borrow(), ["oxtest@libera=hunter2"]));
    }

    #[test]
    fn missing_environment_passwords_are_asked_for_instead() {
        let mut editor = with_sasl("{ env = \"OXMACS_TEST_NO_SUCH_VARIABLE\" }");
        state(&mut editor).keychain.write = |server, account, password| {
            STORED.with(|stored| {
                stored
                    .borrow_mut()
                    .push(format!("{account}@{server}={password}"));
            });
            Ok(())
        };

        irc(&mut editor);

        let prompt = editor
            .minibuffer()
            .map(|minibuffer| minibuffer.prompt().to_owned());
        assert_eq!(
            prompt.as_deref(),
            Some("OXMACS_TEST_NO_SUCH_VARIABLE is not set; password for oxtest on libera: ")
        );
        assert_eq!(connections(&mut editor), []);
        type_text(&mut editor, "hunter2");
        press(&mut editor, "RET");
        assert_eq!(connections(&mut editor).len(), 1);
        STORED.with(|stored| assert!(stored.borrow().is_empty()));
    }

    fn local(address: &str) -> LocalDaemon {
        LocalDaemon {
            address: address.to_owned(),
            token: "tok".to_owned(),
            pid: 1,
        }
    }

    #[test]
    fn a_remote_bouncer_carries_every_server() {
        let mut editor = configured(&format!(
            "[irc.bouncer]\nmode = \"remote\"\naddress = \"bnc.example.org:6697\"\npassword = {{ text = \"pw\" }}\n\n{LIBERA}"
        ));

        irc(&mut editor);

        let (id, endpoint) = connections(&mut editor).remove(0);
        assert_eq!(
            endpoint,
            Endpoint {
                host: "bnc.example.org".to_owned(),
                port: 6697,
                tls: true
            }
        );
        editor.connection_event(id, Event::Connected);
        assert_eq!(
            sent(&mut editor),
            [
                "CAP LS 302",
                "PASS pw",
                "NICK oxtest",
                "USER oxtest/libera 0 * oxmacs"
            ]
        );
    }

    #[test]
    fn a_running_local_bouncer_is_used_with_its_token() {
        let mut editor = configured(&format!("[irc.bouncer]\nmode = \"local\"\n\n{LIBERA}"));
        state(&mut editor).local_daemon = || Some(local("127.0.0.1:4321"));

        irc(&mut editor);

        let (id, endpoint) = connections(&mut editor).remove(0);
        assert_eq!(
            endpoint,
            Endpoint {
                host: "127.0.0.1".to_owned(),
                port: 4321,
                tls: false
            }
        );
        editor.connection_event(id, Event::Connected);
        assert!(sent(&mut editor).contains(&"PASS tok".to_owned()));
    }

    #[test]
    fn a_missing_local_bouncer_is_started_first() {
        let mut editor = configured(&format!("[irc.bouncer]\nmode = \"local\"\n\n{LIBERA}"));
        state(&mut editor).local_daemon = || None;

        irc(&mut editor);

        let launched = editor.take_effects().iter().any(|effect| {
            matches!(effect, Effect::Applet(payload) if payload.get::<LaunchDaemon>().is_some())
        });
        assert!(launched);
        assert_eq!(editor.message(), Some("Starting the oxmacs bouncer…"));
        state(&mut editor).local_daemon = || Some(local("127.0.0.1:4321"));
        editor.deliver(&oxmacs_core::applet::Payload::new(DaemonReady(true)));
        assert_eq!(connections(&mut editor)[0].1.port, 4321);
    }

    #[test]
    fn stopping_the_daemon_asks_the_bouncer_to_quit() {
        let mut editor = configured(&format!("[irc.bouncer]\nmode = \"local\"\n\n{LIBERA}"));
        state(&mut editor).local_daemon = || Some(local("127.0.0.1:4321"));
        irc(&mut editor);
        let id = connections(&mut editor)[0].0;
        editor.connection_event(id, Event::Connected);
        sent(&mut editor);

        press(&mut editor, "M-x");
        type_text(&mut editor, "irc-stop-daemon");
        press(&mut editor, "RET");

        assert_eq!(sent(&mut editor), ["OXMACS STOP"]);
    }

    #[test]
    fn leaving_says_goodbye_to_every_open_server() {
        let (mut editor, id) = connected();

        editor.quitting();

        let effects = editor.take_effects();
        assert!(effects.contains(&Effect::Transmit(id, vec!["QUIT oxmacs".to_owned()])));
        assert!(effects.contains(&Effect::Disconnect(id)));
        editor.quitting();
        assert_eq!(editor.take_effects(), []);
    }

    #[test]
    fn several_servers_are_chosen_by_name() {
        let mut editor = configured(&format!(
            "{LIBERA}[[irc.server]]\nname = \"local\"\nhost = \"localhost\"\nport = 6667\ntls = false\nnick = \"me\"\n"
        ));

        irc(&mut editor);
        assert_eq!(editor.minibuffer().unwrap().prompt(), "IRC server: ");
        assert_eq!(labels(&editor), ["libera", "local"]);
        type_text(&mut editor, "local");
        press(&mut editor, "RET");

        assert_eq!(connections(&mut editor)[0].1.host, "localhost");
    }
}
