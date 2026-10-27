use std::cell::RefCell;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use ox_lang::value::print;
use oxmacs_core::editor::Editor;
use oxmacs_core::input::KeySequence;
use oxmacs_core::text::Buffer;

use crate::Ox;
use crate::testing::started;

struct Fake {
    port: u16,
    heard: Receiver<String>,
    log: RefCell<Vec<String>>,
    out: Arc<Mutex<Option<TcpStream>>>,
}

impl Fake {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (sender, heard) = mpsc::channel();
        let out = Arc::new(Mutex::new(None));
        let writer = Arc::clone(&out);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                *writer.lock().unwrap() = Some(stream.try_clone().unwrap());
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    if sender.send(line).is_err() {
                        return;
                    }
                }
            }
        });
        Self {
            port,
            heard,
            log: RefCell::new(Vec::new()),
            out,
        }
    }

    fn say(&self, line: &str) {
        let mut out = self.out.lock().unwrap();
        let stream = out.as_mut().expect("nobody connected yet");
        write!(stream, "{line}\r\n").unwrap();
        stream.flush().unwrap();
    }

    fn hang_up(&self) {
        if let Some(stream) = self.out.lock().unwrap().take() {
            stream.shutdown(Shutdown::Both).unwrap();
        }
    }

    fn drain(&self) {
        while let Ok(line) = self.heard.try_recv() {
            self.log.borrow_mut().push(line);
        }
    }

    fn expect(&self, editor: &mut Editor, ox: &mut Ox, wanted: &str) {
        let heard = pump(editor, ox, |_, _| {
            self.drain();
            self.log.borrow().iter().any(|line| line == wanted)
        });
        assert!(
            heard,
            "never heard {wanted:?}, only {:?}",
            self.log.borrow()
        );
        self.log.borrow_mut().retain(|line| line != wanted);
    }
}

fn pump(
    editor: &mut Editor,
    ox: &mut Ox,
    mut done: impl FnMut(&mut Editor, &mut Ox) -> bool,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        ox.tick(editor);
        if done(editor, ox) {
            return true;
        }
        thread::sleep(Duration::from_millis(2));
    }
    false
}

fn keys(editor: &mut Editor, ox: &mut Ox, notation: &str) {
    let keys: KeySequence = notation.parse().unwrap();
    for key in keys.keys() {
        editor.handle_key(*key);
        for _ in 0..5 {
            ox.tick(editor);
        }
    }
}

fn type_in(editor: &mut Editor, ox: &mut Ox, text: &str) {
    if text.is_empty() {
        return;
    }
    let notation: Vec<String> = text
        .chars()
        .map(|c| match c {
            ' ' => "SPC".to_owned(),
            other => other.to_string(),
        })
        .collect();
    keys(editor, ox, &notation.join(" "));
}

fn run(editor: &mut Editor, ox: &mut Ox, command: &str) {
    let id = editor.command_named(command).unwrap();
    editor.execute(id);
    for _ in 0..5 {
        ox.tick(editor);
    }
}

fn contents(editor: &Editor, name: &str) -> String {
    editor
        .buffer_named(name)
        .and_then(|id| editor.buffer_by_id(id))
        .map(Buffer::contents)
        .unwrap_or_default()
}

fn wait_for_text(editor: &mut Editor, ox: &mut Ox, buffer: &str, wanted: &str) {
    let seen = pump(editor, ox, |editor, _| {
        contents(editor, buffer).contains(wanted)
    });
    assert!(
        seen,
        "{buffer} never showed {wanted:?}: {}",
        contents(editor, buffer)
    );
}

fn network(port: u16, extra: &str) -> String {
    format!(
        r##"(use-plugin irc :config {{networks: [{{name: "Test" host: "127.0.0.1" port: {port} tls: false nick: "ana" channels: ["#oxmacs"] {extra}}}] notify: false logs: false}})"##
    )
}

const CAPS: &str = "server-time message-tags echo-message batch draft/chathistory multi-prefix";

fn joined() -> (Fake, tempfile::TempDir, Editor, Ox) {
    let fake = Fake::start();
    let (home, mut editor, mut ox) = started(&network(fake.port, ""));
    run(&mut editor, &mut ox, "irc");
    fake.expect(&mut editor, &mut ox, "CAP LS 302");
    fake.expect(&mut editor, &mut ox, "NICK ana");
    fake.expect(&mut editor, &mut ox, "USER ana 0 * ana");
    fake.say(&format!(":srv CAP * LS :{CAPS} sts"));
    fake.expect(&mut editor, &mut ox, &format!("CAP REQ :{CAPS}"));
    fake.say(&format!(":srv CAP ana ACK :{CAPS}"));
    fake.expect(&mut editor, &mut ox, "CAP END");
    fake.say(":srv 001 ana :Welcome to the Test network, ana");
    fake.say(":srv 005 ana PREFIX=(ov)@+ NETWORK=Test :are supported by this server");
    fake.expect(&mut editor, &mut ox, "JOIN #oxmacs");
    fake.say(":ana!a@host JOIN #oxmacs");
    fake.say(":srv 353 ana = #oxmacs :@bo ana +cy");
    fake.say(":srv 366 ana #oxmacs :End of /NAMES list.");
    fake.expect(&mut editor, &mut ox, "CHATHISTORY LATEST #oxmacs * 50");
    (fake, home, editor, ox)
}

fn eval(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
    ox.eval(editor, source).unwrap()
}

#[test]
fn messages_fill_the_width_beside_an_aligned_nick_column() {
    let (_home, mut editor, mut ox) = started("");

    eval(
        &mut editor,
        &mut ox,
        r#"(module (import (irc format :as f) (ox time)))
           (def at (time/instant "2026-10-23T09:30:00Z"))
           (def first-one (f/message-lines {kind: :privmsg time: at nick: "bo" text: "hi https://example.org/a. and some more words to wrap"} (f/fresh-context) 60 6))
           (def second-one (f/message-lines {kind: :privmsg time: (+ at 1000) nick: "bo" text: "again" mention: true} first-one.context 60 6))
           (open-buffer "*t*" (concat first-one.lines second-one.lines))"#,
    );

    let clock = eval(
        &mut editor,
        &mut ox,
        r#"(module (import (irc format :as f) (ox time))) (f/clock (time/instant "2026-10-23T09:30:00Z"))"#,
    );
    let clock = clock.trim_matches('"');
    let lines: Vec<String> = contents(&editor, "*t*")
        .lines()
        .map(str::to_owned)
        .collect();
    assert!(lines[0].starts_with("─── "), "{lines:?}");
    assert_eq!(
        lines[1],
        format!("{clock}     bo hi https://example.org/a. and some more words")
    );
    assert_eq!(lines[2], "             to wrap");
    assert_eq!(lines[3], "             again");
    let spans = editor.line_style(1).spans;
    assert!(
        spans.contains(&(16..37, oxmacs_core::theme::Highlight::Link)),
        "{spans:?}"
    );
    assert_eq!(
        editor.line_style(3).background,
        Some(oxmacs_core::theme::Highlight::Marked)
    );
}

#[test]
fn connecting_negotiates_capabilities_joins_and_shows_history() {
    let (fake, _home, mut editor, mut ox) = joined();

    fake.say(":srv BATCH +h1 chathistory #oxmacs");
    fake.say(
        "@batch=h1;time=2026-10-23T08:00:00.000Z;msgid=m1 :bo!b@host PRIVMSG #oxmacs :good morning",
    );
    fake.say(":srv BATCH -h1");
    wait_for_text(&mut editor, &mut ox, "#oxmacs", "good morning");
    assert_eq!(editor.buffer().name(), "*irc Test*");
    assert!(contents(&editor, "*irc Test*").contains("Welcome to the Test network"));

    fake.say("@time=2026-10-23T08:05:00.000Z;msgid=m2 :cy!c@host PRIVMSG #oxmacs :ana: see https://example.org");
    wait_for_text(&mut editor, &mut ox, "#oxmacs", "see https://example.org");
    assert!(contents(&editor, "#oxmacs").contains(" new "));
    assert!(
        editor.mode_line_segments().join(" ").contains("[#oxmacs!]"),
        "{:?} {}",
        editor.mode_line_segments(),
        contents(&editor, "*ox log*")
    );

    fake.say("PING :token");
    fake.expect(&mut editor, &mut ox, "PONG token");
}

#[test]
fn typing_sends_messages_commands_and_completes_nicks() {
    let (fake, _home, mut editor, mut ox) = joined();
    fake.say(":bo!b@host PRIVMSG #oxmacs :hello ana");
    wait_for_text(&mut editor, &mut ox, "#oxmacs", "hello ana");

    run(&mut editor, &mut ox, "irc-next-active");
    assert_eq!(editor.buffer().name(), "#oxmacs");
    assert!(editor.buffer().contents().ends_with("#oxmacs> "));
    assert!(!editor.mode_line_segments().join(" ").contains("#oxmacs"));

    type_in(&mut editor, &mut ox, "hi there");
    keys(&mut editor, &mut ox, "RET");
    fake.expect(&mut editor, &mut ox, "PRIVMSG #oxmacs :hi there");
    fake.say(":ana!a@host PRIVMSG #oxmacs :hi there");
    wait_for_text(&mut editor, &mut ox, "#oxmacs", "ana hi there");

    type_in(&mut editor, &mut ox, "/me waves");
    keys(&mut editor, &mut ox, "RET");
    fake.expect(
        &mut editor,
        &mut ox,
        "PRIVMSG #oxmacs :\u{1}ACTION waves\u{1}",
    );

    type_in(&mut editor, &mut ox, "c");
    keys(&mut editor, &mut ox, "TAB");
    assert_eq!(eval(&mut editor, &mut ox, "(input-text)"), r#""cy: ""#);
    keys(&mut editor, &mut ox, "C-a C-k");
    type_in(&mut editor, &mut ox, "/to");
    keys(&mut editor, &mut ox, "TAB");
    assert_eq!(eval(&mut editor, &mut ox, "(input-text)"), r#""/topic ""#);
    type_in(&mut editor, &mut ox, "coffee");
    keys(&mut editor, &mut ox, "RET");
    fake.expect(&mut editor, &mut ox, "TOPIC #oxmacs coffee");
    type_in(&mut editor, &mut ox, "/part #o");
    keys(&mut editor, &mut ox, "TAB");
    assert_eq!(
        eval(&mut editor, &mut ox, "(input-text)"),
        r#""/part #oxmacs ""#
    );
    keys(&mut editor, &mut ox, "C-a C-k");

    run(&mut editor, &mut ox, "irc-nicks");
    let nicks = contents(&editor, "*irc nicks*");
    for wanted in [
        "3 people",
        "Operators 1",
        "@bo",
        "Voiced 1",
        "+cy",
        "Everyone 1",
        "ana",
    ] {
        assert!(nicks.contains(wanted), "{wanted} missing from {nicks}");
    }
}

#[test]
fn errors_standard_replies_and_invites_show_where_you_are() {
    let (fake, _home, mut editor, mut ox) = joined();
    run(&mut editor, &mut ox, "irc-switch");
    type_in(&mut editor, &mut ox, "#oxmacs");
    keys(&mut editor, &mut ox, "RET");

    fake.say(":srv FAIL CHATHISTORY MESSAGE_ERROR LATEST :Messages could not be retrieved");
    wait_for_text(
        &mut editor,
        &mut ox,
        "#oxmacs",
        "CHATHISTORY failed: Messages could not be retrieved",
    );
    fake.say(":srv WARN REHASH CERTS_EXPIRED :Certificate has expired");
    wait_for_text(
        &mut editor,
        &mut ox,
        "#oxmacs",
        "REHASH: Certificate has expired",
    );
    fake.say(":srv 482 ana #oxmacs :You're not channel operator");
    wait_for_text(
        &mut editor,
        &mut ox,
        "#oxmacs",
        "! You're not channel operator",
    );
    fake.say(":srv 433 ana bob :Nickname is already in use");
    wait_for_text(
        &mut editor,
        &mut ox,
        "#oxmacs",
        "bob: Nickname is already in use",
    );
    fake.say(":bo!b@host INVITE cy #oxmacs");
    wait_for_text(&mut editor, &mut ox, "#oxmacs", "bo invited cy");

    fake.say(":bo!b@host INVITE ana #secret");
    wait_for_text(&mut editor, &mut ox, "#oxmacs", "bo invites you to #secret");
    run(&mut editor, &mut ox, "irc-join");
    assert!(pump(&mut editor, &mut ox, |editor, _| editor
        .minibuffer()
        .is_some()));
    keys(&mut editor, &mut ox, "RET");
    fake.expect(&mut editor, &mut ox, "JOIN #secret");
}

#[test]
fn lost_connections_come_back_and_rejoin_the_open_channels() {
    let (fake, _home, mut editor, mut ox) = joined();
    type_in(&mut editor, &mut ox, "/join #extra");
    keys(&mut editor, &mut ox, "RET");
    fake.expect(&mut editor, &mut ox, "JOIN #extra");
    fake.say(":ana!a@host JOIN #extra");
    wait_for_text(&mut editor, &mut ox, "#extra", "You joined #extra");

    fake.hang_up();
    wait_for_text(
        &mut editor,
        &mut ox,
        "#oxmacs",
        "Disconnected; trying again in 1 second",
    );
    fake.expect(&mut editor, &mut ox, "CAP LS 302");
    fake.say(":srv 001 ana :Welcome back");
    fake.expect(&mut editor, &mut ox, "JOIN #extra,#oxmacs");
    fake.say(":ana!a@host JOIN #extra");
    fake.say(":ana!a@host JOIN #oxmacs");
    assert!(pump(&mut editor, &mut ox, |editor, _| contents(
        editor, "#oxmacs"
    )
    .matches("You joined #oxmacs")
    .count()
        == 2));

    type_in(&mut editor, &mut ox, "/reconnect");
    keys(&mut editor, &mut ox, "RET");
    fake.expect(&mut editor, &mut ox, "QUIT Reconnecting");
    fake.expect(&mut editor, &mut ox, "CAP LS 302");
    fake.say(":srv 001 ana :Welcome again");
    fake.expect(&mut editor, &mut ox, "JOIN #extra,#oxmacs");
}

#[test]
fn the_channel_list_is_filtered_and_joined_from() {
    let (fake, _home, mut editor, mut ox) = joined();
    type_in(&mut editor, &mut ox, "/list");
    keys(&mut editor, &mut ox, "RET");
    fake.expect(&mut editor, &mut ox, "LIST");
    fake.say(":srv 321 ana Channel :Users  Name");
    fake.say(":srv 322 ana #small 3 :a quiet place");
    fake.say(":srv 322 ana #rust 120 :\u{2}Rust\u{2} talk");
    fake.say(":srv 323 ana :End of /LIST");
    wait_for_text(&mut editor, &mut ox, "*irc channels Test*", "#small");

    assert_eq!(editor.buffer().name(), "*irc channels Test*");
    let lines: Vec<String> = editor
        .buffer()
        .contents()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(lines[0], "Channels on Test  2 channels");
    assert_eq!(lines[3], "#rust     120  Rust talk");
    assert_eq!(lines[4], "#small      3  a quiet place");

    keys(&mut editor, &mut ox, "/");
    assert!(pump(&mut editor, &mut ox, |editor, _| editor
        .minibuffer()
        .is_some()));
    type_in(&mut editor, &mut ox, "QUIET");
    keys(&mut editor, &mut ox, "RET");
    wait_for_text(
        &mut editor,
        &mut ox,
        "*irc channels Test*",
        "1 of 2 channels with \"QUIET\"",
    );
    let lines: Vec<String> = editor
        .buffer()
        .contents()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(lines.len(), 4);
    assert!(lines[3].starts_with("#small "), "{lines:?}");

    keys(&mut editor, &mut ox, "RET");
    fake.expect(&mut editor, &mut ox, "JOIN #small");
}

#[test]
fn whois_answers_show_in_the_buffer_that_asked() {
    let (fake, _home, mut editor, mut ox) = joined();
    run(&mut editor, &mut ox, "irc-switch");
    type_in(&mut editor, &mut ox, "#oxmacs");
    keys(&mut editor, &mut ox, "RET");

    type_in(&mut editor, &mut ox, "/whois bo");
    keys(&mut editor, &mut ox, "RET");
    fake.expect(&mut editor, &mut ox, "WHOIS bo");
    fake.say(":srv 311 ana bo b host * :Bo Real");
    fake.say(":srv 319 ana bo :@#oxmacs #rust");
    fake.say(":srv 312 ana bo srv.example :Example server");
    fake.say(":srv 330 ana bo bo-account :is logged in as");
    fake.say(":srv 301 ana bo :gone fishing");
    fake.say(":srv 317 ana bo 300 1792742400 :seconds idle, signon time");
    fake.say(":srv 671 ana bo :is using a secure connection");
    fake.say(":srv 318 ana bo :End of /WHOIS list.");
    wait_for_text(
        &mut editor,
        &mut ox,
        "#oxmacs",
        "bo is using a secure connection",
    );

    let shown = contents(&editor, "#oxmacs");
    for wanted in [
        "bo is b@host (Bo Real)",
        "bo is in @#oxmacs #rust",
        "bo is on srv.example (Example server)",
        "bo is logged in as bo-account",
        "bo is away: gone fishing",
        "bo has been idle 5 minutes, online since ",
    ] {
        assert!(shown.contains(wanted), "{wanted} missing from {shown}");
    }
    assert!(!shown.contains("End of /WHOIS"), "{shown}");
    assert!(!contents(&editor, "*irc Test*").contains("Bo Real"));
}

#[test]
fn monitored_nicks_say_when_they_come_and_go() {
    let fake = Fake::start();
    let (_home, mut editor, mut ox) = started(&network(fake.port, r#"monitor: ["bo" "cy"]"#));
    run(&mut editor, &mut ox, "irc");
    fake.expect(&mut editor, &mut ox, "CAP LS 302");
    fake.say(":srv 001 ana :Welcome");
    fake.say(":srv 005 ana MONITOR=100 :are supported by this server");
    fake.say(":srv 376 ana :End of /MOTD command.");
    fake.expect(&mut editor, &mut ox, "MONITOR + bo,cy");

    fake.say(":srv 730 ana :bo!b@host");
    wait_for_text(&mut editor, &mut ox, "*irc Test*", "bo is online");
    fake.say(":srv 731 ana :cy");
    wait_for_text(&mut editor, &mut ox, "*irc Test*", "cy is offline");
}

#[test]
fn private_chats_follow_nick_changes() {
    let (fake, _home, mut editor, mut ox) = joined();
    fake.say(":dee!d@host PRIVMSG ana :psst");
    wait_for_text(&mut editor, &mut ox, "dee", "psst");

    fake.say(":dee!d@host NICK dee2");
    wait_for_text(&mut editor, &mut ox, "dee", "dee is now dee2");
    fake.say(":dee2!d@host PRIVMSG ana :still me");
    wait_for_text(&mut editor, &mut ox, "dee", "still me");
    assert!(editor.buffer_named("dee2").is_none());

    run(&mut editor, &mut ox, "irc-switch");
    type_in(&mut editor, &mut ox, "dee");
    keys(&mut editor, &mut ox, "RET");
    assert!(editor.buffer().contents().ends_with("dee2> "));
    type_in(&mut editor, &mut ox, "hi");
    keys(&mut editor, &mut ox, "RET");
    fake.expect(&mut editor, &mut ox, "PRIVMSG dee2 hi");
}

#[test]
fn capabilities_the_server_adds_or_drops_are_followed() {
    let (fake, _home, mut editor, mut ox) = joined();
    let away_notify = r#"(module (import (irc client :as c))) (c/has-cap? "Test" "away-notify")"#;

    fake.say(":srv CAP ana NEW :away-notify sasl unknown-cap");
    fake.expect(&mut editor, &mut ox, "CAP REQ away-notify");
    fake.say(":srv CAP ana ACK :away-notify");
    assert!(pump(&mut editor, &mut ox, |editor, ox| eval(
        editor,
        ox,
        away_notify
    ) == "true"));

    fake.say(":srv CAP ana DEL :away-notify");
    assert!(pump(&mut editor, &mut ox, |editor, ox| eval(
        editor,
        ox,
        away_notify
    ) == "false"));
}

#[test]
fn joins_and_parts_of_quiet_people_stay_hidden_until_asked_for() {
    let (fake, _home, mut editor, mut ox) = joined();
    fake.say(":bo!b@host PRIVMSG #oxmacs :I talk");
    fake.say(":dee!d@host JOIN #oxmacs");
    fake.say(":bo!b@host PART #oxmacs :bye");
    wait_for_text(&mut editor, &mut ox, "#oxmacs", "bo left (bye)");
    assert!(!contents(&editor, "#oxmacs").contains("dee joined"));

    run(&mut editor, &mut ox, "irc-toggle-noise");
    wait_for_text(&mut editor, &mut ox, "#oxmacs", "dee joined");
}

#[test]
fn replies_and_reactions_point_at_the_message_they_answer() {
    let (fake, _home, mut editor, mut ox) = joined();
    fake.say("@msgid=m9 :bo!b@host PRIVMSG #oxmacs :lunch?");
    wait_for_text(&mut editor, &mut ox, "#oxmacs", "lunch?");
    run(&mut editor, &mut ox, "irc-next-active");

    run(&mut editor, &mut ox, "irc-reply");
    assert!(editor.buffer().contents().ends_with("↪ bo #oxmacs> "));
    type_in(&mut editor, &mut ox, "sure");
    keys(&mut editor, &mut ox, "RET");
    fake.expect(
        &mut editor,
        &mut ox,
        "@+draft/reply=m9 PRIVMSG #oxmacs sure",
    );
    assert!(editor.buffer().contents().ends_with("\n#oxmacs> "));

    run(&mut editor, &mut ox, "irc-react");
    assert!(pump(&mut editor, &mut ox, |editor, _| editor
        .minibuffer()
        .is_some()));
    type_in(&mut editor, &mut ox, "👍");
    keys(&mut editor, &mut ox, "RET");
    fake.expect(
        &mut editor,
        &mut ox,
        "@+draft/react=👍;+draft/reply=m9 TAGMSG #oxmacs",
    );
    fake.say("@+draft/react=🎉;+draft/reply=m9 :cy!c@host TAGMSG #oxmacs");
    wait_for_text(&mut editor, &mut ox, "#oxmacs", "🎉 1");
}

fn one_page(body: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        while reader.read_line(&mut line).is_ok_and(|read| read > 2) {
            line.clear();
        }
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    port
}

#[test]
fn previews_say_the_title_of_the_page_a_link_leads_to() {
    let (fake, _home, mut editor, mut ox) = joined();
    let port = one_page("<html><head><title>\n  A fine page </title></head></html>");
    fake.say(&format!(
        ":bo!b@host PRIVMSG #oxmacs :look http://127.0.0.1:{port}/x"
    ));
    wait_for_text(&mut editor, &mut ox, "#oxmacs", "look http");
    run(&mut editor, &mut ox, "irc-next-active");
    keys(&mut editor, &mut ox, "C-p C-e C-b");

    run(&mut editor, &mut ox, "irc-preview");

    assert!(
        pump(&mut editor, &mut ox, |editor, _| editor.message()
            == Some("A fine page")),
        "{:?}",
        editor.message()
    );
}

#[test]
fn pasted_lines_are_sent_one_by_one_after_asking() {
    let (fake, _home, mut editor, mut ox) = joined();
    run(&mut editor, &mut ox, "irc-switch");
    type_in(&mut editor, &mut ox, "#oxmacs");
    keys(&mut editor, &mut ox, "RET");
    eval(
        &mut editor,
        &mut ox,
        "(set-input \"#oxmacs\" \"one\ntwo\n\")",
    );

    keys(&mut editor, &mut ox, "RET");
    assert!(pump(&mut editor, &mut ox, |editor, _| editor
        .question()
        .is_some()));
    keys(&mut editor, &mut ox, "y");

    fake.expect(&mut editor, &mut ox, "PRIVMSG #oxmacs one");
    fake.expect(&mut editor, &mut ox, "PRIVMSG #oxmacs two");
}

#[test]
fn a_carriage_return_alone_splits_lines_too_instead_of_reaching_the_server() {
    let (fake, _home, mut editor, mut ox) = joined();
    run(&mut editor, &mut ox, "irc-switch");
    type_in(&mut editor, &mut ox, "#oxmacs");
    keys(&mut editor, &mut ox, "RET");
    eval(&mut editor, &mut ox, "(set-input \"#oxmacs\" \"hi\rQUIT\")");

    keys(&mut editor, &mut ox, "RET");
    assert!(pump(&mut editor, &mut ox, |editor, _| editor
        .question()
        .is_some()));
    keys(&mut editor, &mut ox, "y");

    fake.expect(&mut editor, &mut ox, "PRIVMSG #oxmacs hi");
    fake.expect(&mut editor, &mut ox, "PRIVMSG #oxmacs QUIT");
}

#[test]
fn messages_are_logged_by_day_and_found_again() {
    let fake = Fake::start();
    let (home, mut editor, mut ox) =
        started(&network(fake.port, "").replace("logs: false", "logs: true"));
    run(&mut editor, &mut ox, "irc");
    fake.expect(&mut editor, &mut ox, "CAP LS 302");
    fake.say(":srv 001 ana :Welcome");
    fake.expect(&mut editor, &mut ox, "JOIN #oxmacs");
    fake.say(":ana!a@host JOIN #oxmacs");
    fake.say("@time=2026-10-23T09:00:00.000Z :bo!b@host PRIVMSG #oxmacs :the build is green");
    let folder = home
        .path()
        .join("data")
        .join("irc")
        .join("logs")
        .join("Test")
        .join("#oxmacs");
    let logged = pump(&mut editor, &mut ox, |_, _| {
        fs::read_dir(&folder).is_ok_and(|mut entries| entries.next().is_some())
    });
    assert!(logged, "nothing was logged under {}", folder.display());

    run(&mut editor, &mut ox, "irc-search");
    type_in(&mut editor, &mut ox, "GREEN");
    keys(&mut editor, &mut ox, "RET");
    wait_for_text(
        &mut editor,
        &mut ox,
        "*irc search*",
        "<bo> the build is green",
    );
    keys(&mut editor, &mut ox, "RET");
    assert!(pump(&mut editor, &mut ox, |editor, _| editor
        .buffer()
        .contents()
        .contains("<bo> the build is green")));
    assert!(editor.buffer().file().is_some());
}

#[test]
fn setup_saves_the_network_in_custom_ox_once() {
    let (home, mut editor, mut ox) = started("(use-plugin irc)");
    let answer = |editor: &mut Editor, ox: &mut Ox, text: &str| {
        assert!(pump(editor, ox, |editor, _| editor.minibuffer().is_some()));
        type_in(editor, ox, text);
        keys(editor, ox, "RET");
    };
    let decide = |editor: &mut Editor, ox: &mut Ox, key: &str| {
        assert!(pump(editor, ox, |editor, _| editor.question().is_some()));
        keys(editor, ox, key);
    };

    for _ in 0..2 {
        run(&mut editor, &mut ox, "irc-setup");
        answer(&mut editor, &mut ox, "");
        answer(&mut editor, &mut ox, "");
        answer(&mut editor, &mut ox, "");
        decide(&mut editor, &mut ox, "y");
        answer(&mut editor, &mut ox, "ana");
        decide(&mut editor, &mut ox, "n");
        answer(&mut editor, &mut ox, "");
        decide(&mut editor, &mut ox, "n");
        pump(&mut editor, &mut ox, |editor, _| {
            editor.question().is_none()
        });
    }

    assert_eq!(
        fs::read_to_string(home.path().join("init.ox")).unwrap(),
        "(use-plugin irc)"
    );
    assert_eq!(
        fs::read_to_string(home.path().join("custom.ox")).unwrap(),
        ";; Written by M-x customize. Your own settings belong in init.ox,\n;; which runs after this file and wins.\n\n(configure-plugin 'irc {networks: [{name: \"Libera\" host: \"irc.libera.chat\" port: 6697 tls: true nick: \"ana\" channels: [\"#oxmacs\"]}]})\n"
    );
    assert_eq!(
        eval(
            &mut editor,
            &mut ox,
            "(count (plugin-setting 'irc :networks))"
        ),
        "1"
    );
    assert_eq!(
        eval(
            &mut editor,
            &mut ox,
            "(get (first (plugin-setting 'irc :networks)) :nick)"
        ),
        r#""ana""#
    );
}

fn cheap_vault(home: &Path) {
    let mut isolate = ox_library::isolate();
    let made = isolate
        .eval(
            r#"(module (import (ox crypto) (ox json)))
(let [salt (crypto/random 16)
      key (await (crypto/derive-key "master" salt :memory 64 :iterations 1 :parallelism 1))]
  (json/write {version: 1 salt: salt memory: 64 iterations: 1 parallelism: 1
               sealed: (crypto/seal key (json/write {entries: [{id: "1" title: "IRC Test" username: "ana" password: "pw" url: "" totp: "" notes: ""}]}))}))"#,
        )
        .unwrap();
    fs::write(
        home.join("vault.json"),
        print::display(isolate.heap(), made),
    )
    .unwrap();
}

#[test]
fn sasl_logs_in_from_the_vault_and_again_after_reconnecting() {
    let fake = Fake::start();
    let (home, mut editor, mut ox) = started(&network(fake.port, r#"login: "IRC Test""#));
    cheap_vault(home.path());

    run(&mut editor, &mut ox, "irc");
    assert!(pump(&mut editor, &mut ox, |editor, _| editor
        .minibuffer()
        .is_some()));
    assert_eq!(editor.minibuffer().unwrap().prompt(), "Vault password: ");
    type_in(&mut editor, &mut ox, "master");
    keys(&mut editor, &mut ox, "RET");
    fake.expect(&mut editor, &mut ox, "CAP LS 302");
    fake.say(":srv CAP * LS :sasl server-time");
    fake.expect(&mut editor, &mut ox, "CAP REQ :server-time sasl");
    fake.say(":srv CAP ana ACK :server-time sasl");
    fake.expect(&mut editor, &mut ox, "AUTHENTICATE PLAIN");
    fake.say("AUTHENTICATE +");
    fake.expect(&mut editor, &mut ox, "AUTHENTICATE YW5hAGFuYQBwdw==");
    fake.say(":srv 903 ana :SASL authentication successful");
    fake.expect(&mut editor, &mut ox, "CAP END");
    wait_for_text(&mut editor, &mut ox, "*irc Test*", "Logged in");

    eval(
        &mut editor,
        &mut ox,
        "(module (import (vault store :as v))) (v/lock)",
    );
    fake.hang_up();
    fake.expect(&mut editor, &mut ox, "CAP LS 302");
    assert!(editor.minibuffer().is_none());
    fake.say(":srv CAP * LS :sasl server-time");
    fake.expect(&mut editor, &mut ox, "CAP REQ :server-time sasl");
    fake.say(":srv CAP ana ACK :server-time sasl");
    fake.expect(&mut editor, &mut ox, "AUTHENTICATE PLAIN");
    fake.say("AUTHENTICATE +");
    fake.expect(&mut editor, &mut ox, "AUTHENTICATE YW5hAGFuYQBwdw==");
}
