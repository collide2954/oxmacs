use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use oxmacs_core::editor::Editor;

use crate::Ox;
use crate::testing::{press, run_command, settle, typed};

fn started(init: &str) -> (tempfile::TempDir, Editor, Ox) {
    crate::testing::started(&format!("(use-plugin calendar)\n{init}"))
}

fn answer(editor: &mut Editor, ox: &mut Ox, text: &str) {
    typed(editor, ox, text);
    press(editor, ox, "RET");
}

fn eval(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
    match ox.eval(editor, source) {
        Ok(value) => value,
        Err(error) => panic!("{}", error.message),
    }
}

fn contents(editor: &Editor, name: &str) -> String {
    editor
        .buffer_named(name)
        .and_then(|id| editor.buffer_by_id(id))
        .map(oxmacs_core::text::Buffer::contents)
        .unwrap_or_default()
}

fn personal(home: &Path) -> PathBuf {
    home.join("calendar").join("Personal")
}

fn add_event(editor: &mut Editor, ox: &mut Ox, title: &str, start: &str, repeats: &str) {
    run_command(editor, ox, "calendar-add");
    answer(editor, ox, title);
    answer(editor, ox, "");
    answer(editor, ox, start);
    answer(editor, ox, "");
    answer(editor, ox, repeats);
    answer(editor, ox, "Room 1");
}

#[test]
fn added_events_show_in_the_agenda_and_their_file() {
    let (home, mut editor, mut ox) = started("");

    add_event(&mut editor, &mut ox, "Standup", "9:30", "never");
    run_command(&mut editor, &mut ox, "calendar");

    let agenda = contents(&editor, "*Calendar*");
    assert!(agenda.contains("today"), "{agenda}");
    assert!(
        agenda.contains("09:30–10:30  Standup  @ Room 1  Personal"),
        "{agenda}"
    );
    let files: Vec<_> = fs::read_dir(personal(home.path())).unwrap().collect();
    assert_eq!(files.len(), 1);
    let written = fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
    assert!(written.contains("SUMMARY:Standup"), "{written}");
    assert!(written.contains("TRIGGER:-PT10M"), "{written}");

    press(&mut editor, &mut ox, "RET");
    assert_eq!(editor.buffer().name(), "*Calendar event*");
    assert!(editor.buffer().contents().contains("Where      Room 1"));
}

#[test]
fn one_repeat_of_an_event_can_be_deleted() {
    let (home, mut editor, mut ox) = started("");
    add_event(&mut editor, &mut ox, "Gym", "18:00", "daily");
    run_command(&mut editor, &mut ox, "calendar");
    let before = contents(&editor, "*Calendar*").matches("Gym").count();
    assert_eq!(before, 14);

    press(&mut editor, &mut ox, "d");
    press(&mut editor, &mut ox, "y");

    assert_eq!(contents(&editor, "*Calendar*").matches("Gym").count(), 13);
    let file = fs::read_dir(personal(home.path()))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(fs::read_to_string(file).unwrap().contains("EXDATE"));
}

#[test]
fn the_month_and_week_views_show_the_events() {
    let (_home, mut editor, mut ox) = started("");
    add_event(&mut editor, &mut ox, "Lunch", "12:00", "never");

    run_command(&mut editor, &mut ox, "calendar-month");
    let month = contents(&editor, "*Calendar month*");
    let title = eval(
        &mut editor,
        &mut ox,
        r#"(module (import (ox time))) (time/format (time/now) "%B %Y")"#,
    );
    assert!(month.starts_with(title.trim_matches('"')), "{month}");
    assert!(month.contains("12:00 Lu"), "{month}");

    run_command(&mut editor, &mut ox, "calendar-week");
    assert_eq!(editor.buffer().name(), "*Calendar week*");
    let id = editor.buffer_named("*Calendar week*").unwrap();
    let (_, view) = editor
        .special_buffer::<oxmacs_core::view::ViewBuffer>(id)
        .unwrap();
    let drawn = format!("{:?}", view.node);
    assert!(drawn.contains("Lunch"), "{drawn} {:?}", editor.message());
}

#[test]
fn reminders_come_before_events_once() {
    let (_home, mut editor, mut ox) = started("");
    eval(
        &mut editor,
        &mut ox,
        r#"(module (import (calendar store :as store) (ox time)))
           (store/save-event! {uid: "soon" summary: "Call Bo" calendar: "Personal" start: (+ (time/now) (time/seconds 570)) end: (+ (time/now) (time/minutes 20)) all-day: false})"#,
    );
    settle(&mut editor, &mut ox);

    eval(
        &mut editor,
        &mut ox,
        "(module (import (calendar reminders :as r))) (r/remind!)",
    );
    settle(&mut editor, &mut ox);
    let shown = editor.message().unwrap_or_default().to_owned();
    assert!(shown.starts_with("Call Bo at "), "{shown}");

    editor.show_message("");
    eval(
        &mut editor,
        &mut ox,
        "(module (import (calendar reminders :as r))) (r/remind!)",
    );
    settle(&mut editor, &mut ox);
    assert_eq!(editor.message(), Some(""));
}

const INVITATION: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:review@example.com\r\nDTSTART:20300105T140000Z\r\nDTEND:20300105T150000Z\r\nSUMMARY:Design review\r\nORGANIZER;CN=Bo:mailto:bo@example.com\r\nATTENDEE;CN=Ana;PARTSTAT=NEEDS-ACTION:mailto:ana@example.com\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

#[test]
fn invitations_are_imported_and_answered_with_a_reply_file() {
    let (home, mut editor, mut ox) =
        started(r#"(use-plugin calendar :config {email: "ana@example.com"})"#);
    let invite = home.path().join("invite.ics");
    fs::write(&invite, INVITATION).unwrap();

    run_command(&mut editor, &mut ox, "calendar-import");
    press(&mut editor, &mut ox, "C-a C-k");
    answer(&mut editor, &mut ox, &invite.display().to_string());
    assert!(contents(&editor, "*Calendar*").contains("Design review"));
    assert!(contents(&editor, "*Calendar*").contains("invitation"));

    press(&mut editor, &mut ox, "RET");
    assert_eq!(
        editor.buffer().name(),
        "*Calendar event*",
        "{:?} {}",
        editor.message(),
        editor.buffer().contents()
    );
    press(&mut editor, &mut ox, "a");
    assert!(
        editor
            .message()
            .unwrap_or_default()
            .starts_with("Answered accepted"),
        "{:?}",
        editor.message()
    );

    let reply = personal(home.path()).join("review_example.com-reply.ics");
    let written = fs::read_to_string(&reply).unwrap();
    assert!(written.contains("METHOD:REPLY"), "{written}");
    assert!(
        written.contains("PARTSTAT=ACCEPTED:mailto:ana@example.com"),
        "{written}"
    );
    assert!(editor.buffer().contents().contains("Ana accepted"));
}

#[derive(Default)]
struct Dav {
    heard: Vec<String>,
    resources: Vec<(String, String, String)>,
    version: usize,
}

fn dav_server(dav: Arc<Mutex<Dav>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let mut headers = Vec::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let line = line.trim_end().to_owned();
                if line.is_empty() {
                    break;
                }
                if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                headers.push(line);
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let body = String::from_utf8(body).unwrap();
            let parts: Vec<&str> = first.split_whitespace().collect();
            let (method, path) = (parts[0].to_owned(), parts[1].to_owned());
            let mut state = dav.lock().unwrap();
            let matching = headers
                .iter()
                .filter(|line| line.to_lowercase().starts_with("if-"))
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            state.heard.push(
                format!("{method} {path} {}", matching.to_lowercase())
                    .trim()
                    .to_owned(),
            );
            let (status, etag, reply) = match method.as_str() {
                "REPORT" => {
                    let mut responses = String::new();
                    for (href, etag, data) in &state.resources {
                        let _ = write!(
                            responses,
                            "<d:response><d:href>{href}</d:href><d:propstat><d:prop><d:getetag>{etag}</d:getetag><cal:calendar-data>{}</cal:calendar-data></d:prop></d:propstat></d:response>",
                            data.replace('&', "&amp;").replace('<', "&lt;")
                        );
                    }
                    (
                        "207 Multi-Status",
                        String::new(),
                        format!(
                            "<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\" xmlns:cal=\"urn:ietf:params:xml:ns:caldav\">{responses}</d:multistatus>"
                        ),
                    )
                }
                "PUT" => {
                    state.version += 1;
                    let etag = format!("\"v{}\"", state.version);
                    state.resources.retain(|(href, _, _)| *href != path);
                    state.resources.push((path.clone(), etag.clone(), body));
                    ("201 Created", etag, String::new())
                }
                "DELETE" => {
                    state.resources.retain(|(href, _, _)| *href != path);
                    ("204 No Content", String::new(), String::new())
                }
                _ => ("405 Method Not Allowed", String::new(), String::new()),
            };
            drop(state);
            let etag = if etag.is_empty() {
                String::new()
            } else {
                format!("ETag: {etag}\r\n")
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\n{etag}Content-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    port
}

#[test]
fn caldav_calendars_bring_in_server_events_and_send_local_changes() {
    let dav = Arc::new(Mutex::new(Dav::default()));
    dav.lock().unwrap().resources.push((
        "/cal/work/review.ics".to_owned(),
        "\"r1\"".to_owned(),
        INVITATION.replace("METHOD:REQUEST\r\n", ""),
    ));
    let port = dav_server(Arc::clone(&dav));
    let (home, mut editor, mut ox) = started(&format!(
        r#"(use-plugin calendar :config {{calendars: [{{name: "Work" url: "http://127.0.0.1:{port}/cal/work/"}}]}})"#
    ));

    run_command(&mut editor, &mut ox, "calendar-sync");
    assert_eq!(editor.message(), Some("Calendars synced"));
    let work = home.path().join("calendar").join("Work");
    assert!(
        fs::read_to_string(work.join("review.ics"))
            .unwrap()
            .contains("Design review")
    );

    add_event(&mut editor, &mut ox, "Planning", "10:00", "never");
    run_command(&mut editor, &mut ox, "calendar-sync");
    let heard = dav.lock().unwrap().heard.clone();
    assert!(
        heard
            .iter()
            .any(|line| line.starts_with("PUT /cal/work/") && line.ends_with("if-none-match: *")),
        "{heard:?}"
    );
    assert_eq!(dav.lock().unwrap().resources.len(), 2);

    fs::remove_file(work.join("review.ics")).ok();
    eval(
        &mut editor,
        &mut ox,
        r#"(module (import (calendar store :as store)))
           (store/load!)
           (store/delete-event! (find (fn [event] (= event.summary "Planning")) (store/events)))"#,
    );
    settle(&mut editor, &mut ox);
    run_command(&mut editor, &mut ox, "calendar-sync");
    let heard = dav.lock().unwrap().heard.clone();
    assert!(
        heard
            .iter()
            .any(|line| line.starts_with("DELETE /cal/work/") && line.contains("if-match: \"v1\"")),
        "{heard:?}"
    );
    assert_eq!(dav.lock().unwrap().resources.len(), 1);
}
