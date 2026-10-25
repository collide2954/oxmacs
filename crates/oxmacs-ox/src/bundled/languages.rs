use std::fs;

use oxmacs_core::editor::Editor;
use oxmacs_core::syntax::Language;
use oxmacs_core::theme::Highlight;

use crate::Ox;
use crate::testing::{home_with, press, settle, typed};

fn visiting(name: &str, text: &str) -> (tempfile::TempDir, Editor, Ox) {
    let home = home_with(&[]);
    let path = home.path().join(name);
    fs::write(&path, text).unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    editor.open_file(&path);
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

#[test]
fn python_files_are_highlighted_indented_and_served_by_pyright() {
    let (_home, mut editor, mut ox) = visiting(
        "a.py",
        "def f(x):\n    \"\"\"Doc\n    more\"\"\"\n    return None\n",
    );

    assert_eq!(editor.language().map(Language::name), Some("Python"));
    assert_eq!(
        editor.line_style(0).spans,
        [(0..3, Highlight::Keyword), (4..5, Highlight::Function)]
    );
    assert_eq!(editor.line_style(1).spans, [(4..10, Highlight::String)]);
    assert_eq!(editor.line_style(2).spans, [(0..11, Highlight::String)]);
    assert_eq!(
        editor.line_style(3).spans,
        [(4..10, Highlight::Keyword), (11..15, Highlight::Constant)]
    );

    press(&mut editor, &mut ox, "C-e RET");
    assert_eq!(editor.buffer().line_text(1), "    ");

    let listed = editor.command_named("list-tools").unwrap();
    editor.execute(listed);
    settle(&mut editor, &mut ox);
    let shown = editor.buffer().contents();
    for server in [
        "pyright",
        "gopls",
        "clangd",
        "typescript-language-server",
        "marksman",
    ] {
        assert!(shown.contains(server), "{server} in {shown}");
    }
}

#[test]
fn markdown_files_are_in_markdown_mode_again() {
    let (_home, editor, _ox) = visiting("README.md", "# Title\n\n```sh\nls\n```\n- [x](y)\n");

    assert_eq!(editor.language().map(Language::name), Some("Markdown"));
    assert_eq!(editor.line_style(0).spans, [(0..7, Highlight::Heading)]);
    assert_eq!(editor.line_style(3).spans, [(0..2, Highlight::String)]);
    assert_eq!(
        editor.line_style(5).spans,
        [(0..1, Highlight::Keyword), (2..8, Highlight::Link)]
    );
}

#[test]
fn go_indents_with_tabs_and_closing_braces_dedent() {
    let (_home, mut editor, mut ox) = visiting("main.go", "func main() {");

    press(&mut editor, &mut ox, "C-e RET");
    assert_eq!(editor.buffer().contents(), "func main() {\n\t");

    typed(&mut editor, &mut ox, "}");
    assert_eq!(editor.buffer().contents(), "func main() {\n}");
}

#[test]
fn c_indents_with_four_spaces_and_json_with_two() {
    let (_home, mut editor, mut ox) = visiting("a.c", "int main(void) {");
    press(&mut editor, &mut ox, "C-e RET");
    assert_eq!(editor.buffer().contents(), "int main(void) {\n    ");

    let (_home, mut editor, mut ox) = visiting("a.json", "{");
    press(&mut editor, &mut ox, "C-e RET");
    assert_eq!(editor.buffer().contents(), "{\n  ");
}
