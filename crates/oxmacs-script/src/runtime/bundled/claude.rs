use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use ox_lang::value::print;
use oxmacs_core::editor::Editor;
use oxmacs_core::input::KeySequence;
use oxmacs_core::text::Buffer;
use serde_json::{Value, json};

use crate::Ox;
use crate::testing::home_with;

struct Request {
    head: String,
    body: Value,
}

struct Fake {
    url: String,
    seen: Arc<Mutex<Vec<Request>>>,
}

fn read_request(stream: &mut TcpStream) -> Request {
    let mut seen = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = stream.read(&mut chunk).unwrap_or(0);
        seen.extend_from_slice(&chunk[..read]);
        let text = String::from_utf8_lossy(&seen).to_string();
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let wanted = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .and_then(|length| length.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if body.len() >= wanted || read == 0 {
                return Request {
                    head: head.to_ascii_lowercase(),
                    body: serde_json::from_str(body).unwrap_or(Value::Null),
                };
            }
        }
        if read == 0 {
            return Request {
                head: text,
                body: Value::Null,
            };
        }
    }
}

impl Fake {
    fn start(replies: Vec<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&seen);
        thread::spawn(move || {
            for reply in replies {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let request = read_request(&mut stream);
                kept.lock().unwrap().push(request);
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        Self { url, seen }
    }

    fn request(&self, index: usize) -> (String, Value) {
        let seen = self.seen.lock().unwrap();
        let request = &seen[index];
        (request.head.clone(), request.body.clone())
    }
}

fn streamed(events: &[Value]) -> String {
    let mut reply = String::from(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
    );
    for event in events {
        reply.push_str(&format!(
            "event: {}\ndata: {event}\n\n",
            event["type"].as_str().unwrap()
        ));
    }
    reply
}

fn started_reply() -> Value {
    json!({"type": "message_start", "message": {"model": "claude-opus-5-5", "usage": {"input_tokens": 10}}})
}

fn said(text: &str) -> String {
    streamed(&[
        started_reply(),
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": text}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 5}}),
        json!({"type": "message_stop"}),
    ])
}

fn calls(id: &str, name: &str, input: &Value) -> String {
    let text = input.to_string();
    let (start, rest) = text.split_at(text.len() / 2);
    streamed(&[
        started_reply(),
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": ""}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "A tool will do"}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "signature_delta", "signature": "sig"}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "content_block_start", "index": 1, "content_block": {"type": "tool_use", "id": id, "name": name, "input": {}}}),
        json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": start}}),
        json!({"type": "content_block_delta", "index": 1, "delta": {"type": "input_json_delta", "partial_json": rest}}),
        json!({"type": "content_block_stop", "index": 1}),
        json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 9}}),
        json!({"type": "message_stop"}),
    ])
}

fn cheap_vault(home: &Path) {
    let mut isolate = ox_library::isolate();
    let made = isolate
        .eval(
            r#"(module (import (ox crypto) (ox json)))
(let [salt (crypto/random 16)
      key (crypto/derive-key "master" salt :memory 64 :iterations 1 :parallelism 1)]
  (json/write {version: 1 salt: salt memory: 64 iterations: 1 parallelism: 1
               sealed: (crypto/seal key (json/write {entries: [{id: "1" title: "Anthropic API key" username: "" password: "test-key" url: "" totp: "" notes: ""}]}))}))"#,
        )
        .unwrap();
    fs::write(
        home.join("vault.json"),
        print::display(isolate.heap(), made),
    )
    .unwrap();
}

fn started(fake: &Fake, extra: &str) -> (tempfile::TempDir, Editor, Ox) {
    let init = format!(
        "(use-plugin claude :config {{api-url: {:?}}})\n{extra}",
        fake.url
    );
    let home = home_with(&[("init.ox", &init), ("data/claude/key-in-vault", "yes\n")]);
    cheap_vault(home.path());
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    pump(&mut editor, &mut ox, |_| false, Duration::from_millis(200));
    (home, editor, ox)
}

fn pump(
    editor: &mut Editor,
    ox: &mut Ox,
    mut done: impl FnMut(&Editor) -> bool,
    limit: Duration,
) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        ox.tick(editor);
        if done(editor) {
            return true;
        }
        thread::sleep(Duration::from_millis(2));
    }
    false
}

fn until(editor: &mut Editor, ox: &mut Ox, what: &str, done: impl FnMut(&Editor) -> bool) {
    assert!(
        pump(editor, ox, done, Duration::from_secs(10)),
        "never saw {what}; the chat shows:\n{}",
        contents(editor, "*claude*")
    );
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

fn ask(editor: &mut Editor, ox: &mut Ox, text: &str) {
    run(editor, ox, "claude");
    assert_eq!(editor.buffer().name(), "*claude*");
    type_in(editor, ox, text);
    keys(editor, ox, "RET");
    until(editor, ox, "the vault's password prompt", |editor| {
        editor
            .minibuffer()
            .is_some_and(|minibuffer| minibuffer.prompt() == "Vault password: ")
    });
    type_in(editor, ox, "master");
    keys(editor, ox, "RET");
}

#[test]
fn a_chat_streams_replies_and_runs_commands_once_allowed() {
    let fake = Fake::start(vec![
        calls("toolu_1", "run_command", &json!({"name": "test-ping"})),
        said("Done, it pinged."),
    ]);
    let (_home, mut editor, mut ox) = started(
        &fake,
        r#"(defonce pinged (atom 0)) (defcommand test-ping "Ping" (swap! pinged inc))"#,
    );

    ask(&mut editor, &mut ox, "ping it");
    until(&mut editor, &mut ox, "the question", |editor| {
        editor.question().is_some()
    });
    keys(&mut editor, &mut ox, "y");
    until(&mut editor, &mut ox, "the answer", |editor| {
        contents(editor, "*claude*").contains("Done, it pinged.")
    });

    let chat = contents(&editor, "*claude*");
    for wanted in [
        "You\nping it\n",
        "Claude\n· A tool will do\n⚙ run_command test-ping\n",
        "  ↳ Ran M-x test-ping\n",
        "Claude\nDone, it pinged.\n",
    ] {
        assert!(chat.contains(wanted), "{wanted:?} isn't in\n{chat}");
    }
    assert_eq!(ox.eval(&mut editor, "@pinged").unwrap(), "1");
    let (head, first) = fake.request(0);
    assert!(head.starts_with("post /v1/messages http/1.1"), "{head}");
    assert!(head.contains("\r\nx-api-key: test-key\r\n"), "{head}");
    assert!(
        head.contains("\r\nanthropic-version: 2026-10-21\r\n"),
        "{head}"
    );
    assert_eq!(first["model"], "claude-opus-5-5");
    assert_eq!(first["stream"], true);
    assert_eq!(first["fallbacks"], "default");
    let context = first["messages"][0]["content"][0]["text"].as_str().unwrap();
    assert!(
        context.starts_with("<editor>\nThe person is in the buffer *scratch*"),
        "{context}"
    );
    assert_eq!(first["messages"][0]["content"][1]["text"], "ping it");
    assert!(
        first["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "run_command")
    );
    let (_, second) = fake.request(1);
    assert_eq!(
        second["messages"][1]["content"],
        json!([
            {"type": "thinking", "thinking": "A tool will do", "signature": "sig"},
            {"type": "tool_use", "id": "toolu_1", "name": "run_command", "input": {"name": "test-ping"}}
        ])
    );
    assert_eq!(
        second["messages"][2]["content"],
        json!([{"type": "tool_result", "tool_use_id": "toolu_1", "content": "Ran M-x test-ping", "is_error": false}])
    );
}

#[test]
fn edits_are_reviewed_and_only_the_accepted_ones_made() {
    let fake = Fake::start(vec![
        calls(
            "toolu_2",
            "edit_file",
            &json!({"target": "notes.txt", "edits": [
                {"old_text": "two", "new_text": "TWO"},
                {"old_text": "nine", "new_text": "NINE"}
            ]}),
        ),
        said("I changed one line."),
    ]);
    let (home, mut editor, mut ox) = started(&fake, "");
    let notes = home.path().join("notes.txt");
    fs::write(
        &notes,
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
    )
    .unwrap();
    editor.open_file(&notes);

    ask(&mut editor, &mut ox, "shout");
    until(&mut editor, &mut ox, "the review", |editor| {
        editor.buffer().name() == "*claude review*"
    });
    let review = contents(&editor, "*claude review*");
    assert!(review.contains("Claude's edits  notes.txt"), "{review}");
    assert!(review.contains("-two"), "{review}");
    assert!(review.contains("+TWO"), "{review}");
    keys(&mut editor, &mut ox, "y");
    ox.eval(&mut editor, "(goto-item {kind: :hunk hunk: 1})")
        .unwrap();
    keys(&mut editor, &mut ox, "n C-c C-c");
    until(&mut editor, &mut ox, "the answer", |editor| {
        contents(editor, "*claude*").contains("I changed one line.")
    });

    assert_eq!(
        contents(&editor, "notes.txt"),
        "one\nTWO\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n"
    );
    assert!(editor.buffer_named("*claude review*").is_none());
    let (_, second) = fake.request(1);
    assert_eq!(
        second["messages"][2]["content"][0]["content"],
        "Made edits 1. The person rejected edits 2. They are in the buffer, not yet saved."
    );
}

#[test]
fn a_refused_key_is_explained_in_the_chat() {
    let fake = Fake::start(vec![
        "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"invalid x-api-key\"}}".to_owned(),
    ]);
    let (_home, mut editor, mut ox) = started(&fake, "");

    ask(&mut editor, &mut ox, "hello");
    until(&mut editor, &mut ox, "the failure", |editor| {
        contents(editor, "*claude*").contains("Anthropic refused the API key")
    });

    assert!(contents(&editor, "*claude*").contains(
        "Anthropic refused the API key (invalid x-api-key); M-x claude-set-key gives another"
    ));
}
