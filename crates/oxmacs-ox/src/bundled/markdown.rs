use std::fs;

use oxmacs_core::editor::Editor;
use oxmacs_core::theme::Highlight;

use crate::Ox;
use crate::testing::{home_with, press, settle};

fn started() -> (tempfile::TempDir, Editor, Ox) {
    let home = home_with(&[]);
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn eval(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
    let source = format!("(module (import (markdown render :as md))) {source}");
    match ox.eval(editor, &source) {
        Ok(value) => value,
        Err(error) => panic!("{}", error.message),
    }
}

#[test]
fn lines_are_read_into_code_strong_text_and_links() {
    let (_home, mut editor, mut ox) = started();

    assert_eq!(
        eval(
            &mut editor,
            &mut ox,
            r#"(md/inline "Use `x` with **care**, see [the start](start).")"#
        ),
        r#"[{text: "Use "} {text: "x" face: :constant} {text: " with "} {text: "care" face: :keyword} {text: ", see "} {text: "the start" face: :link target: "start"} {text: "."}]"#
    );
}

#[test]
fn pages_wrap_to_their_width_and_keep_their_links_and_code() {
    let (_home, mut editor, mut ox) = started();

    let page = eval(
        &mut editor,
        &mut ox,
        r##"(md/render "# Title\n\nOne two three four five six [seven](next) eight.\n\n- a bullet that wraps around\n\n```\n(def x 1)\n```\n" 20)"##,
    );

    assert_eq!(
        page,
        concat!(
            r#"{lines: [[["Title" :heading]] [""] ["" "One" " " "two" " " "three" " " "four"] ["" "five" " " "six" " " ["seven" :link]] ["" "eight."] [""] ["  • " "a" " " "bullet" " " "that"] ["    " "wraps" " " "around"] [""] ["    " "(" ["def" :keyword] " " ["x" :function] " " ["1" :constant] ")"]] "#,
            r#"links: [{start: 9 end: 14 target: "next" line: 3}] headings: [{line: 0 title: "Title"}] blocks: [{first: 9 last: 9 source: "(def x 1)"}]}"#
        )
    );
}

#[test]
fn a_link_of_several_words_is_one_link() {
    let (_home, mut editor, mut ox) = started();

    assert_eq!(
        eval(
            &mut editor,
            &mut ox,
            r#"(get (md/render "See [the start page](start), then [more](next)." 80) :links)"#
        ),
        r#"[{start: 4 end: 18 target: "start" line: 0} {start: 25 end: 29 target: "next" line: 0}]"#
    );
}

#[test]
fn a_markdown_buffer_previews_beside_and_follows_its_links() {
    let (home, mut editor, mut ox) = started();
    let readme = home.path().join("README.md");
    fs::write(&readme, "# Notes\n\nSee [the plan](plan.md).\n").unwrap();
    fs::write(home.path().join("plan.md"), "the plan\n").unwrap();
    editor.open_file(&readme);

    let id = editor.command_named("markdown-preview").unwrap();
    editor.execute(id);
    settle(&mut editor, &mut ox);

    let preview = editor
        .buffer_named("*Markdown: README.md*")
        .expect("a preview");
    assert!(editor.buffer_shown(preview));
    editor.show_buffer(preview);
    let shown = editor.buffer().contents();
    assert_eq!(shown, "Notes\n\nSee the plan.");
    assert_eq!(editor.line_style(0).spans, [(0..5, Highlight::Heading)]);
    let link_line = editor.buffer().line_start(2);
    editor.buffer_mut().set_point(link_line);
    press(&mut editor, &mut ox, "RET");
    assert_eq!(editor.buffer().contents(), "the plan\n");
}
