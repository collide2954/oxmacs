use std::path::PathBuf;

use oxmacs_core::editor::{Editor, Effect, ServerId};
use oxmacs_core::external::lsp::{Event, Launch};

use super::{PROMPT, R, ox_text, with_program};
use crate::testing::settle;

fn rscript(r: &R) -> PathBuf {
    r.home.path().join(if cfg!(windows) {
        "Rscript.exe"
    } else {
        "Rscript"
    })
}

fn in_project() -> (tempfile::TempDir, R) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("DESCRIPTION"), "Package: tidy\n").unwrap();
    std::fs::create_dir_all(root.path().join("R")).unwrap();
    let script = root.path().join("R").join("tidy.R");
    std::fs::write(&script, "total <- mean(x)\n").unwrap();
    let mut r = with_program(None, Editor::new());
    std::fs::write(rscript(&r), "").unwrap();
    r.editor.open_file(&script);
    settle(&mut r.editor, &mut r.ox);
    (root, r)
}

fn servers(effects: Vec<Effect>) -> Vec<(ServerId, Launch)> {
    effects
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::StartServer(id, launch) => Some((id, launch)),
            _ => None,
        })
        .collect()
}

fn asked(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ToServer(_, body) => serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|message| message["method"].as_str().map(str::to_owned)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_languageserver_starts_for_r_files_once_r_says_it_is_installed() {
    let (root, mut r) = in_project();
    assert_eq!(servers(r.editor.take_effects()).len(), 0);
    let job = r.running();

    r.output(
        job,
        &format!("\u{1}packages\u{2}devtools\tlanguageserver\u{3}{PROMPT}"),
    );

    let started = servers(r.editor.take_effects());
    assert_eq!(started.len(), 1, "{}", r.logged());
    assert_eq!(started[0].1.program, rscript(&r));
    assert_eq!(started[0].1.args, ["-e", "languageserver::run()"]);
    assert_eq!(started[0].1.directory, root.path());
}

#[test]
fn the_languageserver_setting_turns_it_off() {
    let (_root, mut r) = in_project();
    let job = r.running();
    r.eval("(settings 'r {language-server: false})");

    r.output(
        job,
        &format!("\u{1}packages\u{2}languageserver\u{3}{PROMPT}"),
    );

    assert_eq!(servers(r.editor.take_effects()).len(), 0);
}

#[test]
fn without_a_session_completion_and_definitions_come_from_the_languageserver() {
    let (_root, mut r) = in_project();
    let program = ox_text(&r.program().display().to_string());
    r.eval(&format!(
        "(module (import (r language :as language))) (language/define! {program})"
    ));
    settle(&mut r.editor, &mut r.ox);
    let id = servers(r.editor.take_effects())[0].0;
    r.editor.server_event(
        id,
        Event::Message(
            r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"completionProvider":{},"definitionProvider":true}}}"#
                .to_owned(),
        ),
    );
    r.editor.take_effects();
    r.editor.go_to_line(1, 12);

    r.press("C-M-i");
    assert_eq!(asked(&r.editor.take_effects()), ["textDocument/completion"]);
    r.press("M-.");
    assert_eq!(asked(&r.editor.take_effects()), ["textDocument/definition"]);
}
