use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::{SERVERS, ServerId, ServerSpec};
use crate::config::Config;
use crate::editor::{Editor, Effect};
use crate::external::diagnostics::Severity;
use crate::external::lsp::{Event, Launch};
use crate::testing::{press, type_text};

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\n",
    )
    .unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/main.rs"),
        "fn main() {\n    let 😀 = 1;\n}\n",
    )
    .unwrap();
    dir
}

fn editor() -> Editor {
    let mut editor = Editor::new();
    editor.lsp.find = |program| Some(PathBuf::from(program));
    editor.add_language_servers(SERVERS);
    editor
}

fn sent(effects: &[Effect]) -> Vec<Value> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ToServer(_, body) => serde_json::from_str(body).ok(),
            _ => None,
        })
        .collect()
}

fn methods(effects: &[Effect]) -> Vec<String> {
    sent(effects)
        .iter()
        .map(|message| {
            message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("response")
                .to_owned()
        })
        .collect()
}

fn reply(editor: &mut Editor, message: &Value) {
    editor.server_event(ServerId(0), Event::Message(message.to_string()));
}

fn started() -> (tempfile::TempDir, Editor) {
    let dir = project();
    let mut editor = editor();
    editor.open_file(&dir.path().join("src/main.rs"));
    editor.take_effects();
    reply(
        &mut editor,
        &json!({"jsonrpc": "2.0", "id": 1, "result": {"capabilities": {"positionEncoding": "utf-16"}}}),
    );
    (dir, editor)
}

#[test]
fn opening_a_file_starts_its_server_in_the_project_root() {
    let dir = project();
    let mut editor = editor();

    editor.open_file(&dir.path().join("src/main.rs"));
    let effects = editor.take_effects();

    assert!(effects.contains(&Effect::StartServer(
        ServerId(0),
        Launch {
            program: PathBuf::from("rust-analyzer"),
            args: Vec::new(),
            directory: dir.path().to_path_buf(),
        }
    )));
    let initialize = &sent(&effects)[0];
    assert_eq!(initialize["method"], "initialize");
    assert_eq!(
        initialize["params"]["rootUri"],
        super::protocol::uri(dir.path()).as_str()
    );
    assert!(editor.mode_line().contains("rust-analyzer…"));
    fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    editor.open_file(&dir.path().join("src/lib.rs"));
    assert!(
        !editor
            .take_effects()
            .iter()
            .any(|effect| matches!(effect, Effect::StartServer(..)))
    );
}

#[test]
fn the_server_hears_about_open_files_once_it_is_ready() {
    let (dir, mut editor) = started();
    let effects = editor.take_effects();

    assert_eq!(methods(&effects), ["initialized", "textDocument/didOpen"]);
    let opened = &sent(&effects)[1]["params"]["textDocument"];
    assert_eq!(opened["languageId"], "rust");
    assert_eq!(opened["version"], 1);
    assert_eq!(opened["text"], "fn main() {\n    let 😀 = 1;\n}\n");
    assert_eq!(
        editor.mode_line().rsplit("   ").next(),
        Some("rust-analyzer")
    );

    type_text(&mut editor, "x");
    let changed = sent(&editor.take_effects());
    assert_eq!(changed[0]["method"], "textDocument/didChange");
    assert_eq!(changed[0]["params"]["textDocument"]["version"], 2);
    assert_eq!(
        changed[0]["params"]["contentChanges"][0]["text"],
        "xfn main() {\n    let 😀 = 1;\n}\n"
    );
    assert_eq!(editor.take_effects(), Vec::<Effect>::new());

    press(&mut editor, "C-x C-s");
    assert!(methods(&editor.take_effects()).contains(&"textDocument/didSave".to_owned()));

    press(&mut editor, "C-x k RET");
    assert_eq!(methods(&editor.take_effects()), ["textDocument/didClose"]);
    drop(dir);
}

#[test]
fn diagnostics_from_the_server_show_where_they_point() {
    let (dir, mut editor) = started();
    let path = dir.path().join("src/main.rs");

    reply(
        &mut editor,
        &json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": {
            "uri": super::protocol::uri(&path),
            "diagnostics": [
                {"range": {"start": {"line": 1, "character": 8}, "end": {"line": 1, "character": 10}},
                 "severity": 2, "message": "unused variable"},
                {"range": {"start": {"line": 1, "character": 13}, "end": {"line": 1, "character": 14}},
                 "severity": 1, "message": "expected `;`"},
                {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 2}},
                 "severity": 4, "message": "a hint"}
            ]
        }}),
    );

    let found: Vec<_> = editor
        .lints
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.start,
                diagnostic.end,
                diagnostic.severity,
                diagnostic.message.as_str(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ((2, 9), (2, 10), Severity::Warning, "unused variable"),
            ((2, 13), (2, 14), Severity::Error, "expected `;`"),
        ]
    );
}

#[test]
fn missing_servers_are_named_once_with_how_to_get_them() {
    let dir = project();
    fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    let mut editor = editor();
    editor.lsp.find = |_| None;

    editor.open_file(&dir.path().join("src/main.rs"));
    editor.take_effects();
    assert_eq!(
        editor.message(),
        Some(
            "rust-analyzer would add completion, docs and more here; M-x list-tools can install it"
        )
    );

    editor.show_message("");
    editor.open_file(&dir.path().join("src/lib.rs"));
    editor.take_effects();
    assert_eq!(editor.message(), Some(""));
}

#[test]
fn the_setting_keeps_servers_from_starting() {
    let dir = project();
    let mut editor = editor();
    editor.apply_config(
        &Config::default()
            .with("language-servers", json!(false))
            .unwrap(),
    );

    editor.open_file(&dir.path().join("src/main.rs"));

    assert_eq!(editor.take_effects(), Vec::<Effect>::new());
}

#[test]
fn server_requests_are_answered() {
    let (_dir, mut editor) = started();
    editor.take_effects();

    reply(
        &mut editor,
        &json!({"jsonrpc": "2.0", "id": 7, "method": "workspace/configuration",
                "params": {"items": [{"section": "a"}, {"section": "b"}]}}),
    );
    reply(
        &mut editor,
        &json!({"jsonrpc": "2.0", "id": "x", "method": "window/workDoneProgress/create", "params": {"token": 1}}),
    );
    reply(
        &mut editor,
        &json!({"jsonrpc": "2.0", "id": 8, "method": "rust-analyzer/strange", "params": {}}),
    );

    let answers = sent(&editor.take_effects());
    assert_eq!(
        answers[0],
        json!({"jsonrpc": "2.0", "id": 7, "result": [null, null]})
    );
    assert_eq!(
        answers[1],
        json!({"jsonrpc": "2.0", "id": "x", "result": null})
    );
    assert_eq!(answers[2]["error"]["code"], -32601);
}

#[test]
fn progress_shows_in_the_mode_line() {
    let (_dir, mut editor) = started();
    let progress = |value: Value| json!({"jsonrpc": "2.0", "method": "$/progress", "params": {"token": "index", "value": value}});

    reply(
        &mut editor,
        &progress(json!({"kind": "begin", "title": "Indexing", "percentage": 0})),
    );
    reply(
        &mut editor,
        &progress(json!({"kind": "report", "percentage": 40})),
    );
    assert!(
        editor.mode_line().ends_with("rust-analyzer Indexing (40%)"),
        "{}",
        editor.mode_line()
    );

    reply(&mut editor, &progress(json!({"kind": "end"})));
    assert!(editor.mode_line().ends_with("   rust-analyzer"));
}

#[test]
fn a_server_that_stops_says_so_and_logs_why() {
    let (_dir, mut editor) = started();

    editor.server_event(ServerId(0), Event::Log("thread panicked".to_owned()));
    editor.server_event(
        ServerId(0),
        Event::Exited(Some("exited with code 101".to_owned())),
    );

    assert_eq!(
        editor.message(),
        Some("rust-analyzer stopped: exited with code 101; M-x lsp-log shows why")
    );
    assert!(editor.mode_line().ends_with("rust-analyzer stopped"));
    press(&mut editor, "M-x");
    type_text(&mut editor, "lsp-log");
    press(&mut editor, "RET");
    let log = editor.buffer().contents();
    assert!(log.contains("[rust-analyzer] ready\n"), "{log}");
    assert!(log.contains("[rust-analyzer] thread panicked\n"), "{log}");
    assert!(
        log.contains("[rust-analyzer] stopped: exited with code 101\n"),
        "{log}"
    );
}

#[test]
fn shutting_down_asks_first_then_tells_the_server_to_exit() {
    let (_dir, mut editor) = started();
    editor.take_effects();

    editor.shutdown_servers();
    let asked = sent(&editor.take_effects());
    assert_eq!(asked[0]["method"], "shutdown");
    reply(
        &mut editor,
        &json!({"jsonrpc": "2.0", "id": asked[0]["id"], "result": null}),
    );

    let effects = editor.take_effects();
    assert_eq!(methods(&effects), ["exit"]);
    assert!(effects.contains(&Effect::StopServer(ServerId(0))));
    editor.server_event(ServerId(0), Event::Exited(None));
    assert_eq!(editor.message(), None);
}

#[test]
fn rust_files_with_a_server_skip_the_cargo_lint() {
    let (dir, mut editor) = started();
    editor.take_effects();

    type_text(&mut editor, "x");
    press(&mut editor, "C-x C-s");

    assert!(
        !editor
            .take_effects()
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn(_))),
        "{}",
        dir.path().display()
    );
}

#[test]
fn restarting_stops_the_server_and_starts_a_fresh_one() {
    let (_dir, mut editor) = started();
    editor.take_effects();

    press(&mut editor, "M-x");
    type_text(&mut editor, "lsp-restart");
    press(&mut editor, "RET");

    let effects = editor.take_effects();
    assert!(effects.contains(&Effect::StopServer(ServerId(0))));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::StartServer(ServerId(1), _)))
    );
    assert_eq!(editor.message(), Some("Restarting rust-analyzer"));
}

#[test]
fn roots_fall_back_to_git_and_then_the_file_s_folder() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("repo/.git")).unwrap();
    fs::create_dir_all(dir.path().join("repo/scripts")).unwrap();
    let spec = SERVERS[0];

    assert_eq!(
        super::root_of(&spec, &dir.path().join("repo/scripts/a.rs")),
        dir.path().join("repo")
    );
    assert_eq!(
        super::root_of(&spec, &dir.path().join("loose.rs")),
        dir.path().to_path_buf()
    );
}

fn asked(editor: &mut Editor, method: &str) -> Value {
    sent(&editor.take_effects())
        .into_iter()
        .find(|message| message["method"] == method)
        .unwrap_or_else(|| panic!("no {method} request"))
}

fn answer(editor: &mut Editor, request: &Value, result: &Value) {
    reply(
        editor,
        &json!({"jsonrpc": "2.0", "id": request["id"], "result": result}),
    );
}

fn capable() -> (tempfile::TempDir, Editor) {
    let dir = project();
    let mut editor = editor();
    editor.open_file(&dir.path().join("src/main.rs"));
    editor.take_effects();
    reply(
        &mut editor,
        &json!({"jsonrpc": "2.0", "id": 1, "result": {"capabilities": {
            "positionEncoding": "utf-16",
            "completionProvider": {"triggerCharacters": ["."]},
            "signatureHelpProvider": {},
            "hoverProvider": true,
            "definitionProvider": true,
            "referencesProvider": true,
            "renameProvider": true,
            "codeActionProvider": true,
            "documentFormattingProvider": true
        }}}),
    );
    editor.take_effects();
    (dir, editor)
}

#[test]
fn typing_a_name_asks_for_completions_and_offers_them() {
    let (_dir, mut editor) = capable();
    press(&mut editor, "M->");
    type_text(&mut editor, "pri");
    assert!(editor.wants_auto_complete());

    editor.auto_complete();
    let request = asked(&mut editor, "textDocument/completion");
    assert_eq!(
        request["params"]["position"],
        json!({"line": 3, "character": 3})
    );
    assert!(!editor.wants_auto_complete());
    answer(
        &mut editor,
        &request,
        &json!({"isIncomplete": false, "items": [
            {"label": "println!", "sortText": "2"},
            {"label": "print!", "sortText": "1"},
            {"label": "private", "insertText": "private", "sortText": "3"}
        ]}),
    );

    assert_eq!(
        editor.pane().popup().map(|popup| popup.items),
        Some(vec![
            "print!".to_owned(),
            "println!".to_owned(),
            "private".to_owned()
        ])
    );
}

#[test]
fn c_m_i_completes_at_once_when_there_is_one_answer() {
    let (_dir, mut editor) = capable();
    press(&mut editor, "M->");
    type_text(&mut editor, "Vec::wi");

    press(&mut editor, "C-M-i");
    let request = asked(&mut editor, "textDocument/completion");
    answer(
        &mut editor,
        &request,
        &json!([{"label": "with_capacity", "textEdit": {
            "range": {"start": {"line": 3, "character": 5}, "end": {"line": 3, "character": 7}},
            "newText": "with_capacity"
        }}]),
    );

    assert!(
        editor
            .buffer()
            .contents()
            .ends_with("}\nVec::with_capacity"),
        "{}",
        editor.buffer().contents()
    );
}

#[test]
fn the_signature_of_the_call_at_point_shows_in_the_echo_area() {
    let (_dir, mut editor) = capable();
    press(&mut editor, "M->");
    type_text(&mut editor, "f(");
    assert!(editor.wants_signature());

    editor.show_signature();
    let request = asked(&mut editor, "textDocument/signatureHelp");
    answer(
        &mut editor,
        &request,
        &json!({"signatures": [{"label": "fn f(x: i32) -> i32"}], "activeSignature": 0}),
    );
    assert_eq!(editor.message(), Some("fn f(x: i32) -> i32"));
    assert!(!editor.wants_signature());

    type_text(&mut editor, "1)");
    editor.show_signature();
    let request = asked(&mut editor, "textDocument/signatureHelp");
    answer(&mut editor, &request, &Value::Null);
    assert_eq!(editor.message(), None);
}

#[test]
fn c_c_c_d_shows_the_docs_beside() {
    let (_dir, mut editor) = capable();

    press(&mut editor, "C-c C-d");
    let request = asked(&mut editor, "textDocument/hover");
    answer(
        &mut editor,
        &request,
        &json!({"contents": {"kind": "markdown", "value": "```rust\nfn main()\n```\n\nThe entry point."}}),
    );

    assert_eq!(editor.buffer().name(), "*doc*");
    assert_eq!(
        editor.buffer().contents(),
        "fn main()\n\nThe entry point.\n"
    );
    press(&mut editor, "q");
    assert_eq!(editor.buffer().name(), "main.rs");
}

#[test]
fn m_dot_goes_to_the_definition_and_m_comma_comes_back() {
    let (dir, mut editor) = capable();
    fs::write(dir.path().join("src/lib.rs"), "pub fn helper() {}\n").unwrap();
    press(&mut editor, "C-n C-e");

    press(&mut editor, "M-.");
    let request = asked(&mut editor, "textDocument/definition");
    answer(
        &mut editor,
        &request,
        &json!([{"uri": super::protocol::uri(&dir.path().join("src/lib.rs")),
                "range": {"start": {"line": 0, "character": 7}, "end": {"line": 0, "character": 13}}}]),
    );
    assert_eq!(editor.buffer().name(), "lib.rs");
    assert_eq!(editor.buffer().point(), 7);

    press(&mut editor, "M-,");
    assert_eq!(editor.buffer().name(), "main.rs");
}

#[test]
fn m_question_lists_the_references_in_xref() {
    let (dir, mut editor) = capable();
    let main = super::protocol::uri(&dir.path().join("src/main.rs"));
    press(&mut editor, "C-n C-f C-f C-f C-f C-f C-f C-f C-f");

    press(&mut editor, "M-?");
    let request = asked(&mut editor, "textDocument/references");
    assert_eq!(request["params"]["context"]["includeDeclaration"], true);
    answer(
        &mut editor,
        &request,
        &json!([
            {"uri": main, "range": {"start": {"line": 1, "character": 8}, "end": {"line": 1, "character": 10}}},
            {"uri": main, "range": {"start": {"line": 0, "character": 3}, "end": {"line": 0, "character": 7}}}
        ]),
    );

    assert_eq!(editor.buffer().name(), "*xref*");
    let contents = editor.buffer().contents();
    assert!(contents.starts_with("2 references to "), "{contents}");
    assert!(contents.contains("     1:4    fn main() {"), "{contents}");
    assert!(contents.contains("     2:9    let 😀 = 1;"), "{contents}");
}

#[test]
fn f2_renames_through_the_server_and_leaves_files_unsaved() {
    let (dir, mut editor) = capable();
    let main = super::protocol::uri(&dir.path().join("src/main.rs"));
    press(&mut editor, "C-f C-f C-f C-f");

    press(&mut editor, "<f2>");
    assert_eq!(editor.minibuffer().unwrap().prompt(), "Rename main to: ");
    press(&mut editor, "C-a C-k");
    type_text(&mut editor, "start");
    press(&mut editor, "RET");
    let request = asked(&mut editor, "textDocument/rename");
    assert_eq!(request["params"]["newName"], "start");
    answer(
        &mut editor,
        &request,
        &json!({"changes": {main: [
            {"range": {"start": {"line": 0, "character": 3}, "end": {"line": 0, "character": 7}}, "newText": "start"}
        ]}}),
    );

    assert!(editor.buffer().contents().starts_with("fn start() {"));
    assert!(editor.buffer().is_modified());
    assert_eq!(
        editor.message(),
        Some("Renamed main in 1 places in 1 files; C-x s saves them")
    );
}

#[test]
fn edits_and_diagnostics_reach_a_buffer_through_another_spelling_of_its_file() {
    let (dir, mut editor) = capable();
    let other = super::protocol::uri(&dir.path().join("src").join(".").join("main.rs"));
    let buffers = editor.buffer_ids().len();
    press(&mut editor, "C-f C-f C-f C-f");

    press(&mut editor, "<f2>");
    press(&mut editor, "C-a C-k");
    type_text(&mut editor, "start");
    press(&mut editor, "RET");
    let request = asked(&mut editor, "textDocument/rename");
    answer(
        &mut editor,
        &request,
        &json!({"changes": {other.clone(): [
            {"range": {"start": {"line": 0, "character": 3}, "end": {"line": 0, "character": 7}}, "newText": "start"}
        ]}}),
    );
    reply(
        &mut editor,
        &json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
                "params": {"uri": other, "diagnostics": [
                    {"range": {"start": {"line": 1, "character": 8}, "end": {"line": 1, "character": 10}},
                     "severity": 2, "message": "unused variable"}]}}),
    );

    assert!(editor.buffer().contents().starts_with("fn start() {"));
    assert_eq!(editor.buffer_ids().len(), buffers);
    assert_eq!(editor.diagnostics_on_line(0, 1).len(), 1);
}

#[test]
fn edits_inserted_at_the_same_place_keep_the_server_s_order() {
    let (dir, mut editor) = capable();
    let main = super::protocol::uri(&dir.path().join("src/main.rs"));
    press(&mut editor, "C-f C-f C-f C-f");

    press(&mut editor, "<f2>");
    press(&mut editor, "C-a C-k");
    type_text(&mut editor, "start");
    press(&mut editor, "RET");
    let request = asked(&mut editor, "textDocument/rename");
    let at = json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}});
    answer(
        &mut editor,
        &request,
        &json!({"changes": {main: [
            {"range": at, "newText": "a"},
            {"range": at, "newText": "b"},
            {"range": at, "newText": "c"}
        ]}}),
    );

    assert!(
        editor.buffer().contents().starts_with("abcfn main() {"),
        "{}",
        editor.buffer().contents()
    );
}

#[test]
fn c_c_dot_offers_the_server_s_fixes_with_the_diagnostics_here() {
    let (dir, mut editor) = capable();
    let main = super::protocol::uri(&dir.path().join("src/main.rs"));
    let diagnostic = json!({"range": {"start": {"line": 1, "character": 8}, "end": {"line": 1, "character": 10}},
                            "severity": 2, "message": "unused variable"});
    reply(
        &mut editor,
        &json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
                "params": {"uri": main, "diagnostics": [diagnostic]}}),
    );
    press(&mut editor, "C-n");

    press(&mut editor, "C-c .");
    let request = asked(&mut editor, "textDocument/codeAction");
    assert_eq!(request["params"]["context"]["diagnostics"][0], diagnostic);
    answer(
        &mut editor,
        &request,
        &json!([{"title": "Rename to _x", "kind": "quickfix", "edit": {"changes": {main: [
            {"range": {"start": {"line": 1, "character": 8}, "end": {"line": 1, "character": 10}}, "newText": "_x"}
        ]}}}]),
    );
    assert_eq!(editor.minibuffer().unwrap().prompt(), "Code action: ");
    press(&mut editor, "RET");

    assert!(
        editor.buffer().contents().contains("let _x = 1;"),
        "{}",
        editor.buffer().contents()
    );
}

#[test]
fn servers_format_languages_oxmacs_has_no_formatter_for() {
    let dir = project();
    fs::write(dir.path().join("notes.toml"), "a=1\n").unwrap();
    let mut editor = editor();
    editor.open_file(&dir.path().join("notes.toml"));
    editor.take_effects();
    reply(
        &mut editor,
        &json!({"jsonrpc": "2.0", "id": 1, "result": {"capabilities": {"documentFormattingProvider": true}}}),
    );
    editor.take_effects();

    press(&mut editor, "C-c C-f");
    let request = asked(&mut editor, "textDocument/formatting");
    answer(
        &mut editor,
        &request,
        &json!([{"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 3}}, "newText": "a = 1"}]),
    );

    assert_eq!(editor.buffer().contents(), "a = 1\n");
}

fn toml_with_a_formatter() -> (tempfile::TempDir, Editor) {
    let dir = project();
    fs::write(dir.path().join("notes.toml"), "a=1\n").unwrap();
    let mut editor = editor();
    editor.open_file(&dir.path().join("notes.toml"));
    editor.take_effects();
    reply(
        &mut editor,
        &json!({"jsonrpc": "2.0", "id": 1, "result": {"capabilities": {"documentFormattingProvider": true}}}),
    );
    editor.take_effects();
    (dir, editor)
}

fn spaced() -> Value {
    json!([{"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 3}}, "newText": "a = 1"}])
}

const STALE: &str = "The text changed while the server worked on it, so its edits were dropped";

#[test]
fn edits_for_text_that_changed_since_the_request_are_dropped() {
    let (_dir, mut editor) = toml_with_a_formatter();
    press(&mut editor, "C-c C-f");
    let request = asked(&mut editor, "textDocument/formatting");
    press(&mut editor, "M->");
    type_text(&mut editor, "b=2");

    answer(&mut editor, &request, &spaced());

    assert_eq!(editor.buffer().contents(), "a=1\nb=2");
    assert_eq!(editor.message(), Some(STALE));
}

#[test]
fn renames_for_text_that_changed_since_the_request_are_dropped() {
    let (dir, mut editor) = capable();
    let main = super::protocol::uri(&dir.path().join("src/main.rs"));
    press(&mut editor, "C-f C-f C-f C-f <f2> C-a C-k");
    type_text(&mut editor, "start");
    press(&mut editor, "RET");
    let request = asked(&mut editor, "textDocument/rename");
    press(&mut editor, "C-a");
    type_text(&mut editor, "pub ");

    answer(
        &mut editor,
        &request,
        &json!({"changes": {main: [
            {"range": {"start": {"line": 0, "character": 3}, "end": {"line": 0, "character": 7}}, "newText": "start"}
        ]}}),
    );

    assert!(editor.buffer().contents().starts_with("pub fn main() {"));
    assert_eq!(editor.message(), Some(STALE));
}

#[test]
fn format_on_save_through_a_server_formats_first_and_writes_once() {
    let (dir, mut editor) = toml_with_a_formatter();
    editor.settings.format_on_save = true;
    let path = dir.path().join("notes.toml");
    press(&mut editor, "M->");
    type_text(&mut editor, "b=2");
    press(&mut editor, "RET");

    press(&mut editor, "C-x C-s");
    let request = asked(&mut editor, "textDocument/formatting");
    assert_eq!(fs::read_to_string(&path).unwrap(), "a=1\n");
    answer(&mut editor, &request, &spaced());

    assert_eq!(fs::read_to_string(&path).unwrap(), "a = 1\nb=2\n");
    assert!(!editor.buffer().is_modified());
}

#[test]
fn format_on_save_still_saves_when_the_server_fails_or_stops() {
    let (dir, mut editor) = toml_with_a_formatter();
    editor.settings.format_on_save = true;
    let path = dir.path().join("notes.toml");
    press(&mut editor, "M->");
    type_text(&mut editor, "b=2");
    press(&mut editor, "RET");

    press(&mut editor, "C-x C-s");
    let request = asked(&mut editor, "textDocument/formatting");
    reply(
        &mut editor,
        &json!({"jsonrpc": "2.0", "id": request["id"], "error": {"code": -32603, "message": "broken"}}),
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "a=1\nb=2\n");

    type_text(&mut editor, "c=3");
    press(&mut editor, "RET");
    press(&mut editor, "C-x C-s");
    asked(&mut editor, "textDocument/formatting");
    editor.server_event(ServerId(0), Event::Exited(None));
    assert_eq!(fs::read_to_string(&path).unwrap(), "a=1\nb=2\nc=3\n");
}

#[test]
fn edits_the_server_asks_for_are_applied() {
    let (dir, mut editor) = capable();
    let main = super::protocol::uri(&dir.path().join("src/main.rs"));

    reply(
        &mut editor,
        &json!({"jsonrpc": "2.0", "id": 40, "method": "workspace/applyEdit", "params": {"edit": {"changes": {main: [
            {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}, "newText": "// top\n"}
        ]}}}}),
    );

    assert!(editor.buffer().contents().starts_with("// top\nfn main"));
    let answers = sent(&editor.take_effects());
    assert!(
        answers
            .iter()
            .any(|message| message["id"] == 40 && message["result"]["applied"] == true)
    );
}

#[test]
fn features_without_a_server_say_so() {
    let mut editor = Editor::new();

    press(&mut editor, "M-.");

    assert_eq!(
        editor.message(),
        Some("No language server works on this buffer")
    );
}

#[test]
fn inlay_hints_show_faintly_at_the_end_of_their_lines_until_an_edit() {
    let (_dir, mut editor) = capable();
    let mut capabilities = editor.lsp.running[0].capabilities.clone();
    capabilities["inlayHintProvider"] = json!(true);
    editor.lsp.running[0].capabilities = capabilities;
    press(&mut editor, "C-n");

    editor.show_signature();
    let request = asked(&mut editor, "textDocument/inlayHint");
    assert_eq!(request["params"]["range"]["end"]["line"], 4);
    answer(
        &mut editor,
        &request,
        &json!([
            {"position": {"line": 1, "character": 10}, "label": [{"value": ": "}, {"value": "i32"}], "kind": 1},
            {"position": {"line": 1, "character": 14}, "label": "extra:", "kind": 2}
        ]),
    );

    assert_eq!(
        editor.pane().line_note(1),
        Some((": i32  extra:".to_owned(), crate::theme::Highlight::Comment))
    );
    type_text(&mut editor, "x");
    assert_eq!(editor.pane().line_note(1), None);
}

static MARKDOWN: crate::syntax::LanguageSpec = crate::syntax::LanguageSpec {
    name: "Markdown",
    extensions: &["md"],
    file_names: &[],
    aliases: &["markdown"],
    line_comment: None,
    pairs: &[],
    indentation: crate::syntax::Indentation::KEEP,
    grammar: None,
    grammars: &[],
    outline: crate::syntax::Outline::NONE,
    features: &[],
};

const MARKSMAN: ServerSpec = ServerSpec {
    name: "marksman",
    languages: &[("Markdown", "markdown")],
    programs: &[("marksman", &["server"])],
    roots: &[],
    install: "",
    runs: false,
};

#[test]
fn list_tools_lists_only_the_servers_of_languages_oxmacs_knows() {
    let mut editor = Editor::new();
    editor.lsp.find = |_| None;
    editor.add_language_servers(SERVERS);
    editor.add_language_servers(&[MARKSMAN]);
    let listed = |editor: &mut Editor| {
        press(editor, "M-x");
        type_text(editor, "list-tools");
        press(editor, "RET");
        crate::testing::lines(editor)
            .iter()
            .filter_map(|line| line.trim_start().split(' ').next().map(str::to_owned))
            .collect::<Vec<_>>()
    };

    let names = listed(&mut editor);
    for known in ["rust-analyzer", "taplo"] {
        assert!(names.iter().any(|name| name == known), "{names:?}");
    }
    assert!(!names.iter().any(|name| name == "marksman"), "{names:?}");

    editor.add_language(crate::syntax::Language::new(&MARKDOWN), None);
    let names = listed(&mut editor);
    assert!(names.iter().any(|name| name == "marksman"), "{names:?}");
}

#[test]
fn list_tools_names_the_language_servers_and_offers_to_install_them() {
    let mut editor = Editor::new();
    editor.lsp.find = |program| (program == "taplo").then(|| PathBuf::from("taplo"));
    editor.add_language_servers(SERVERS);

    press(&mut editor, "M-x");
    type_text(&mut editor, "list-tools");
    press(&mut editor, "RET");
    let shown = crate::testing::lines(&editor);
    assert!(
        shown.iter().any(|line| line == "Language servers"),
        "{shown:#?}"
    );
    let row = |name: &str| {
        shown
            .iter()
            .position(|line| line.trim_start().starts_with(&format!("{name} ")))
            .unwrap_or_else(|| panic!("no {name}: {shown:#?}"))
    };
    assert!(
        shown[row("rust-analyzer")]
            .ends_with("✗ not installed; i runs rustup component add rust-analyzer")
    );
    assert!(shown[row("taplo")].contains("✓ found taplo"));
    let line = row("rust-analyzer");
    let buffer = editor.buffer_mut();
    let start = buffer.line_start(line);
    buffer.set_point(start);
    editor.take_effects();

    press(&mut editor, "i");
    assert_eq!(
        editor
            .question()
            .map(|question| question.prompt().to_owned()),
        Some(
            "Run `rustup component add rust-analyzer` to install rust-analyzer? (y or n) "
                .to_owned()
        )
    );
    press(&mut editor, "y");

    assert!(editor.take_effects().iter().any(|effect| matches!(
        effect,
        Effect::Spawn(job) if job.program == "rustup" && job.args == ["component", "add", "rust-analyzer"]
    )));
}

#[test]
fn missing_servers_are_not_searched_for_on_every_keystroke() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEARCHES: AtomicUsize = AtomicUsize::new(0);
    let dir = project();
    let mut editor = editor();
    editor.lsp.find = |_| {
        SEARCHES.fetch_add(1, Ordering::Relaxed);
        None
    };
    editor.open_file(&dir.path().join("src/main.rs"));
    editor.take_effects();
    let after_open = SEARCHES.load(Ordering::Relaxed);

    for _ in 0..20 {
        type_text(&mut editor, "x");
        editor.take_effects();
    }

    assert_eq!(SEARCHES.load(Ordering::Relaxed), after_open);
    editor.retry_servers();
    editor.take_effects();
    assert!(SEARCHES.load(Ordering::Relaxed) > after_open);
}
