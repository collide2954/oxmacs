use std::collections::VecDeque;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use lsp_server::{Connection, Message, Notification, Request, RequestId};
use serde_json::{Value, json};

struct Client {
    connection: Connection,
    next: i32,
    waiting: VecDeque<Notification>,
    server: Option<JoinHandle<()>>,
}

const WAIT: Duration = Duration::from_secs(20);

impl Client {
    fn start() -> Self {
        let (server, connection) = Connection::memory();
        let thread = thread::spawn(move || {
            ox_lsp::run(&server, &ox_lsp::Options::plain()).unwrap();
        });
        let mut client = Self {
            connection,
            next: 0,
            waiting: VecDeque::new(),
            server: Some(thread),
        };
        client.request("initialize", json!({"capabilities": {}}));
        client.notify("initialized", json!({}));
        client
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = RequestId::from(self.next);
        let request = Request::new(id.clone(), method.to_owned(), params);
        self.connection.sender.send(request.into()).unwrap();
        loop {
            match self.connection.receiver.recv_timeout(WAIT).unwrap() {
                Message::Response(response) if response.id == id => {
                    return response.response_result.unwrap();
                }
                Message::Notification(notification) => self.waiting.push_back(notification),
                _ => {}
            }
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        let notification = Notification::new(method.to_owned(), params);
        self.connection.sender.send(notification.into()).unwrap();
    }

    fn notification(&mut self, method: &str) -> Value {
        if let Some(at) = self
            .waiting
            .iter()
            .position(|notification| notification.method == method)
        {
            return self.waiting.remove(at).unwrap().params;
        }
        loop {
            if let Message::Notification(notification) =
                self.connection.receiver.recv_timeout(WAIT).unwrap()
            {
                if notification.method == method {
                    return notification.params;
                }
                self.waiting.push_back(notification);
            }
        }
    }

    fn open(&mut self, uri: &str, text: &str) -> Value {
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": uri, "languageId": "ox", "version": 1, "text": text}}),
        );
        self.notification("textDocument/publishDiagnostics")
    }

    fn at(&mut self, method: &str, uri: &str, line: u32, character: u32) -> Value {
        self.request(
            method,
            json!({
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character},
                "context": {"includeDeclaration": true},
                "newName": "surface"
            }),
        )
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

const SHAPES: &str = "(def (area w h) \"Width times height.\" (* w h))
(def limit 5)
(area 1 (inc limit))
(helpr 2)
";

fn shapes() -> (tempfile::TempDir, String, Client) {
    let dir = tempfile::tempdir().unwrap();
    let file = oxmacs_files::uri::from_path(&dir.path().join("shapes.ox"));
    (dir, file, Client::start())
}

#[test]
fn opening_a_file_publishes_the_checker_findings() {
    let (_dir, file, mut client) = shapes();

    let published = client.open(&file, SHAPES);

    assert_eq!(published["uri"], json!(file));
    let found = published["diagnostics"].as_array().unwrap();
    assert_eq!(found.len(), 1, "{published}");
    assert_eq!(found[0]["message"], json!("helpr is not defined"));
    assert_eq!(found[0]["severity"], json!(1));
    assert_eq!(
        found[0]["range"],
        json!({"start": {"line": 3, "character": 1}, "end": {"line": 3, "character": 6}})
    );

    client.notify(
        "textDocument/didChange",
        json!({"textDocument": {"uri": file, "version": 2}, "contentChanges": [{"text": "(def x 1)\nx\n"}]}),
    );
    let published = client.notification("textDocument/publishDiagnostics");
    assert_eq!(published["diagnostics"], json!([]));
}

#[test]
fn completion_offers_the_file_s_names_and_the_library() {
    let (_dir, file, mut client) = shapes();
    client.open(&file, "(def (area w h) (* w h))\n(ar");

    let found = client.at("textDocument/completion", &file, 1, 3);

    let labels: Vec<&str> = found
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"area"), "{labels:?}");
    assert!(labels.contains(&"arity-error"), "{labels:?}");
    assert!(!labels.contains(&"inc"), "{labels:?}");
}

#[test]
fn hover_and_signature_help_show_signatures_and_docs() {
    let (_dir, file, mut client) = shapes();
    client.open(&file, SHAPES);

    let hover = client.at("textDocument/hover", &file, 2, 2);
    let shown = hover["contents"]["value"].as_str().unwrap();
    assert!(shown.contains("(area w h)"), "{shown}");
    assert!(shown.contains("Width times height."), "{shown}");

    let hover = client.at("textDocument/hover", &file, 2, 10);
    let shown = hover["contents"]["value"].as_str().unwrap();
    assert!(shown.contains("(inc x)"), "{shown}");

    let help = client.at("textDocument/signatureHelp", &file, 2, 7);
    assert_eq!(help["signatures"][0]["label"], json!("(area w h)"));
}

#[test]
fn definitions_references_and_renames_follow_names() {
    let (_dir, file, mut client) = shapes();
    client.open(&file, SHAPES);

    let found = client.at("textDocument/definition", &file, 2, 2);
    assert_eq!(found["uri"], json!(file));
    assert_eq!(found["range"]["start"], json!({"line": 0, "character": 6}));

    let found = client.at("textDocument/references", &file, 2, 2);
    let lines: Vec<u64> = found
        .as_array()
        .unwrap()
        .iter()
        .map(|location| location["range"]["start"]["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, [0, 2]);

    let edit = client.at("textDocument/rename", &file, 2, 2);
    let edits = edit["changes"][&file].as_array().unwrap();
    assert_eq!(edits.len(), 2);
    assert_eq!(edits[0]["newText"], json!("surface"));
    assert_eq!(
        edits[1]["range"],
        json!({"start": {"line": 2, "character": 1}, "end": {"line": 2, "character": 5}})
    );
}

#[test]
fn renames_and_references_follow_the_binding_not_the_spelling() {
    let (dir, file, mut client) = shapes();
    let geometry = oxmacs_files::uri::from_path(&dir.path().join("geometry.ox"));
    let source = "(def (area w h) (* w h))\n(def (scaled area k) (* area k))\n(area 1 2)\n";
    std::fs::write(dir.path().join("geometry.ox"), source).unwrap();
    client.open(&geometry, source);
    client.open(
        &file,
        "(module (import (geometry :as g)))\n(g/area 1 2)\n(def (own area) area)\n",
    );

    let edit = client.at("textDocument/rename", &file, 1, 4);
    let lines = |uri: &str| -> Vec<(u64, u64)> {
        edit["changes"][uri]
            .as_array()
            .map(|edits| {
                edits
                    .iter()
                    .map(|edit| {
                        let start = &edit["range"]["start"];
                        (
                            start["line"].as_u64().unwrap(),
                            start["character"].as_u64().unwrap(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    assert_eq!(lines(&geometry), [(0, 6), (2, 1)]);
    assert_eq!(lines(&file), [(1, 3)]);

    let found = client.at("textDocument/references", &geometry, 1, 25);
    assert_eq!(found.as_array().unwrap().len(), 2);
}

#[test]
fn formatting_and_symbols_follow_the_forms_of_imported_macros() {
    let (dir, file, mut client) = shapes();
    std::fs::write(
        dir.path().join("kit.ox"),
        "(defmacro ^{indent: 1 defines: 0} defwidget [name & body] nil)\n",
    )
    .unwrap();
    client.open(
        &file,
        "(module (import (kit :only [defwidget])))\n(defwidget dial\n1)",
    );

    let edits = client.request(
        "textDocument/formatting",
        json!({"textDocument": {"uri": file}, "options": {"tabSize": 2, "insertSpaces": true}}),
    );
    assert_eq!(
        edits[0]["newText"],
        json!("(module (import (kit :only [defwidget])))\n(defwidget dial\n  1)\n")
    );
    let symbols = client.request(
        "textDocument/documentSymbol",
        json!({"textDocument": {"uri": file}}),
    );
    assert_eq!(symbols[0]["name"], json!("dial"));
}

#[test]
fn formatting_and_symbols_use_the_ox_tools() {
    let (_dir, file, mut client) = shapes();
    client.open(&file, "(def (f x)\nx)\n(def limit 5)");

    let edits = client.request(
        "textDocument/formatting",
        json!({"textDocument": {"uri": file}, "options": {"tabSize": 2, "insertSpaces": true}}),
    );
    assert_eq!(
        edits[0]["newText"],
        json!("(def (f x)\n  x)\n(def limit 5)\n")
    );

    let symbols = client.request(
        "textDocument/documentSymbol",
        json!({"textDocument": {"uri": file}}),
    );
    let names: Vec<&str> = symbols
        .as_array()
        .unwrap()
        .iter()
        .map(|symbol| symbol["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["f", "limit"]);
}
