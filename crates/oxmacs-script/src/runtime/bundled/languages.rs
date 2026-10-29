use std::fs;

use oxmacs_core::editor::Editor;
use oxmacs_core::syntax::Language;
use oxmacs_core::theme::Highlight;

use crate::Ox;
use crate::testing::{home_with, press, settle, typed};

const ON: &str = "(use-plugin python) (use-plugin go) (use-plugin c) (use-plugin cpp) (use-plugin typescript) (use-plugin javascript) (use-plugin json)";

fn visiting(name: &str, text: &str) -> (tempfile::TempDir, Editor, Ox) {
    visiting_with(ON, name, text)
}

fn visiting_with(init: &str, name: &str, text: &str) -> (tempfile::TempDir, Editor, Ox) {
    let home = home_with(&[("init.ox", init)]);
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

    assert_eq!(
        editor.language().as_ref().map(Language::name),
        Some("Python")
    );
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
    assert!(shown.contains("pyright"), "{shown}");
    assert!(!shown.contains("gopls"), "{shown}");
}

#[test]
fn a_language_whose_plugin_is_off_opens_as_plain_text_and_says_so() {
    let (_home, editor, _ox) = visiting_with("", "a.py", "def f():\n");

    assert_eq!(editor.language(), None);
    assert_eq!(
        editor.message(),
        Some(
            "a.py opens as plain text while the python plugin is off; (use-plugin python) in init.ox turns it on"
        )
    );
}

#[test]
fn typescript_and_javascript_share_one_server_definition() {
    let (home, mut editor, mut ox) = visiting("a.ts", "const x = 1\n");
    fs::write(home.path().join("b.js"), "let y = 2\n").unwrap();
    editor.open_file(&home.path().join("b.js"));
    settle(&mut editor, &mut ox);
    assert_eq!(
        editor.language().as_ref().map(Language::name),
        Some("JavaScript")
    );

    let listed = editor.command_named("list-tools").unwrap();
    editor.execute(listed);
    settle(&mut editor, &mut ox);
    assert_eq!(
        editor
            .buffer()
            .contents()
            .matches("npm install -g typescript typescript-language-server")
            .count(),
        1
    );
}

#[test]
fn markdown_files_are_in_markdown_mode_again() {
    let (_home, editor, _ox) = visiting("README.md", "# Title\n\n```sh\nls\n```\n- [x](y)\n");

    assert_eq!(
        editor.language().as_ref().map(Language::name),
        Some("Markdown")
    );
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

fn rust_project() -> (tempfile::TempDir, Editor, Ox) {
    let home = home_with(&[
        (
            "init.ox",
            "(settings {language-servers: false}) (use-plugin rust) (use-plugin toml)",
        ),
        ("crab/Cargo.toml", "[package]\nname = \"crab\"\n"),
        ("crab/src/main.rs", "fn main() {}\n"),
    ]);
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    editor.open_file(&home.path().join("crab").join("src").join("main.rs"));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn spawned(editor: &mut Editor) -> Vec<(String, Vec<String>)> {
    editor
        .take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            oxmacs_core::editor::Effect::Spawn(job) => Some((job.program, job.args)),
            _ => None,
        })
        .collect()
}

#[test]
fn rust_files_build_with_cargo_keys_in_their_project() {
    let (home, mut editor, mut ox) = rust_project();

    assert_eq!(editor.language(), Some(Language::rust()));
    for (keys, subcommand) in [
        ("C-c C-c C-k", "check"),
        ("C-c C-c C-b", "build"),
        ("C-c C-c C-t", "test"),
        ("C-c C-c C-r", "run"),
        ("C-c C-c C-l", "clippy"),
    ] {
        editor.take_effects();
        press(&mut editor, &mut ox, keys);
        settle(&mut editor, &mut ox);
        let effects = editor.take_effects();
        let job = effects
            .iter()
            .find_map(|effect| match effect {
                oxmacs_core::editor::Effect::Spawn(job) => Some(job.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{keys} ran nothing"));
        assert_eq!(
            (job.program.as_str(), job.args.as_slice()),
            ("cargo", &[subcommand.to_owned()][..])
        );
        assert_eq!(job.directory, home.path().join("crab"));
    }
    editor.open_file(&home.path().join("crab").join("Cargo.toml"));
    assert_eq!(editor.language(), Some(Language::toml()));
}

#[test]
fn saving_rust_lints_with_the_lint_setting() {
    let (_home, mut editor, mut ox) = rust_project();
    let save = |editor: &mut Editor, ox: &mut Ox| {
        editor.take_effects();
        press(editor, ox, "C-e SPC C-x C-s");
        spawned(editor)
    };

    assert_eq!(
        save(&mut editor, &mut ox),
        [(
            "cargo".to_owned(),
            [
                "check",
                "--workspace",
                "--all-targets",
                "--message-format=json"
            ]
            .map(str::to_owned)
            .to_vec()
        )]
    );
    ox.eval(&mut editor, "(settings 'rust {lint: :clippy})")
        .unwrap();
    assert_eq!(save(&mut editor, &mut ox)[0].1[0], "clippy");
    ox.eval(&mut editor, "(settings 'rust {lint: :off})")
        .unwrap();
    assert_eq!(save(&mut editor, &mut ox), []);
}

#[test]
fn rust_files_open_as_plain_text_until_the_rust_plugin_is_on() {
    let (_home, editor, _ox) = visiting_with("", "main.rs", "fn main() {}\n");

    assert_eq!(editor.language(), None);
    assert_eq!(
        editor.message(),
        Some(
            "main.rs opens as plain text while the rust plugin is off; (use-plugin rust) in init.ox turns it on"
        )
    );
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
