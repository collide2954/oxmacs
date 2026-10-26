use std::fs;
use std::path::Path;
use std::process::Command;

use oxmacs_core::editor::{Cell, Clicks, Editor};
use oxmacs_core::theme::Highlight;

use crate::Ox;
use crate::testing::{press, question, settle, typed};

const TREE: &str = "*explorer*";

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in [
        ("src/main.rs", "fn main() {}\n"),
        ("src/lib.rs", "pub fn lib() {}\n"),
        ("docs/guide.md", "# Guide\n"),
        ("README.md", "# Demo\n"),
        (".env", "SECRET=1\n"),
        (".projectile", ""),
    ] {
        let path = dir.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    dir
}

fn started(config: &str) -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    fs::write(
        home.path().join("init.ox"),
        format!("(configure-plugin 'explorer {{delete-to-trash: false{config}}})"),
    )
    .unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn opened(project: &Path, file: &str) -> (tempfile::TempDir, Editor, Ox) {
    let (home, mut editor, mut ox) = started("");
    editor.open_file(&project.join(file));
    settle(&mut editor, &mut ox);
    press(&mut editor, &mut ox, "C-x C-d");
    (home, editor, ox)
}

fn tree(editor: &Editor) -> Vec<String> {
    let id = editor.buffer_named(TREE).unwrap();
    editor
        .buffer_by_id(id)
        .unwrap()
        .contents()
        .lines()
        .map(|line| line.trim_end().to_owned())
        .collect()
}

fn line_at_point(editor: &Editor) -> String {
    let buffer = editor.buffer();
    buffer
        .line_text(buffer.line_of(buffer.point()))
        .trim()
        .to_owned()
}

fn git(directory: &Path, args: &[&str]) {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    let output = command.args(args).current_dir(directory).output().unwrap();
    assert!(output.status.success(), "git {args:?}");
}

#[test]
fn c_x_c_d_shows_the_project_beside_the_windows_open_at_the_current_file() {
    let project = project();
    let (_home, editor, _ox) = opened(project.path(), "src/main.rs");

    let side = editor.side_window().unwrap();
    assert_eq!(editor.selected_window(), side.window);
    assert_eq!(editor.buffer().name(), TREE);
    assert_eq!(
        tree(&editor),
        [
            "▸ docs",
            "▾ src",
            "    lib.rs",
            "    main.rs",
            "  README.md"
        ]
    );
    assert_eq!(line_at_point(&editor), "main.rs");
    assert_eq!(editor.line_style(3).background, Some(Highlight::Region));
    let main = editor.tiling().windows()[0];
    assert_eq!(editor.pane_of(main).unwrap().buffer().name(), "main.rs");
}

#[test]
fn c_x_c_d_moves_to_the_tree_and_then_hides_it() {
    let project = project();
    let (_home, mut editor, mut ox) = opened(project.path(), "src/main.rs");

    press(&mut editor, &mut ox, "C-x o");
    assert_eq!(editor.buffer().name(), "main.rs");
    press(&mut editor, &mut ox, "C-x C-d");
    assert_eq!(editor.buffer().name(), TREE);
    press(&mut editor, &mut ox, "C-x C-d");
    assert_eq!(editor.side_window(), None);
    assert_eq!(editor.buffer().name(), "main.rs");
}

#[test]
fn ret_opens_files_in_the_other_window_and_folders_in_place() {
    let project = project();
    let (_home, mut editor, mut ox) = opened(project.path(), "src/main.rs");

    press(&mut editor, &mut ox, "M-< RET");
    assert_eq!(tree(&editor)[1], "    guide.md");
    press(&mut editor, &mut ox, "C-n RET");
    assert_eq!(editor.buffer().name(), "guide.md");
    assert_eq!(editor.buffer().contents(), "# Guide\n");
    assert_eq!(tree(&editor)[1], "    guide.md");
    let side = editor.side_window().unwrap().window;
    assert_eq!(
        editor.pane_of(side).unwrap().line_style(1).background,
        Some(Highlight::Region)
    );
}

#[test]
fn typing_filters_the_tree_and_del_and_c_g_take_it_back() {
    let project = project();
    let (_home, mut editor, mut ox) = opened(project.path(), "src/main.rs");

    typed(&mut editor, &mut ox, "mai");
    assert_eq!(tree(&editor), ["▾ src", "    main.rs"]);
    assert_eq!(line_at_point(&editor), "main.rs");
    let header: Vec<String> = editor
        .pane()
        .header_lines()
        .into_iter()
        .map(|(text, _)| text)
        .collect();
    assert_eq!(header[1], "  Filter: mai  1 found");

    press(&mut editor, &mut ox, "DEL DEL");
    assert_eq!(
        tree(&editor),
        [
            "▾ docs",
            "    guide.md",
            "▾ src",
            "    main.rs",
            "  README.md"
        ]
    );
    press(&mut editor, &mut ox, "C-g");
    assert_eq!(tree(&editor)[0], "▸ docs");
    press(&mut editor, &mut ox, "C-g");
    assert_eq!(editor.buffer().name(), "main.rs");
}

#[test]
fn opening_a_found_file_clears_the_filter_but_peeking_keeps_it() {
    let project = project();
    let (_home, mut editor, mut ox) = opened(project.path(), "src/main.rs");

    typed(&mut editor, &mut ox, "gui");
    press(&mut editor, &mut ox, "SPC");
    assert_eq!(editor.buffer().name(), TREE);
    assert_eq!(tree(&editor), ["▾ docs", "    guide.md"]);

    press(&mut editor, &mut ox, "RET");
    assert_eq!(editor.buffer().name(), "guide.md");
    press(&mut editor, &mut ox, "C-x C-d");
    typed(&mut editor, &mut ox, "lib");
    assert_eq!(tree(&editor), ["▾ src", "    lib.rs"]);
}

#[test]
fn the_tree_follows_the_file_shown_in_the_other_windows() {
    let project = project();
    let (_home, mut editor, mut ox) = opened(project.path(), "src/main.rs");
    press(&mut editor, &mut ox, "C-x o");

    editor.open_file(&project.path().join("docs").join("guide.md"));
    settle(&mut editor, &mut ox);

    let rows = tree(&editor);
    let guide = rows.iter().position(|row| row == "    guide.md").unwrap();
    let tree_buffer = editor.buffer_named(TREE).unwrap();
    let index = editor.buffer_ids().iter().position(|id| *id == tree_buffer);
    assert!(index.is_some());
    let side = editor.side_window().unwrap().window;
    let pane = editor.pane_of(side).unwrap();
    assert_eq!(pane.buffer().line_of(pane.point()), guide);
}

#[test]
fn new_files_open_and_renames_and_deletes_happen_on_disk() {
    let project = project();
    let (_home, mut editor, mut ox) = opened(project.path(), "src/main.rs");

    press(&mut editor, &mut ox, "C-c C-n");
    assert_eq!(editor.minibuffer().unwrap().prompt(), "New file in src/: ");
    typed(&mut editor, &mut ox, "notes.txt");
    press(&mut editor, &mut ox, "RET");
    assert!(project.path().join("src").join("notes.txt").is_file());
    assert_eq!(editor.buffer().name(), "notes.txt");

    press(&mut editor, &mut ox, "C-x C-d");
    assert_eq!(line_at_point(&editor), "notes.txt");
    press(&mut editor, &mut ox, "C-c C-r C-a C-k");
    typed(&mut editor, &mut ox, "docs/notes.md");
    press(&mut editor, &mut ox, "RET");
    assert!(project.path().join("docs").join("notes.md").is_file());
    assert!(!project.path().join("src").join("notes.txt").exists());
    assert_eq!(line_at_point(&editor), "notes.md");

    press(&mut editor, &mut ox, "<delete>");
    assert_eq!(
        question(&editor).as_deref(),
        Some("Delete docs/notes.md? (y or n) ")
    );
    press(&mut editor, &mut ox, "y");
    assert!(!project.path().join("docs").join("notes.md").exists());
}

#[test]
fn dragging_a_file_onto_a_folder_moves_it_there() {
    let project = project();
    let (_home, mut editor, mut ox) = opened(project.path(), "src/main.rs");
    let side = editor.side_window().unwrap().window;
    editor.resize_pane(side, 20, 40);
    let row = |row| Cell { row, column: 4 };

    editor.click(row(4), Clicks::Single, false);
    editor.drag(row(0));
    editor.release(row(0));
    settle(&mut editor, &mut ox);
    assert_eq!(
        question(&editor).as_deref(),
        Some("Move README.md into docs/? (y or n) ")
    );
    press(&mut editor, &mut ox, "y");

    assert!(project.path().join("docs").join("README.md").is_file());
    assert!(!project.path().join("README.md").exists());
}

#[test]
fn git_changes_and_ignored_files_show_in_the_tree() {
    let project = project();
    git(project.path(), &["init", "-q"]);
    fs::write(project.path().join(".gitignore"), "target/\n").unwrap();
    fs::create_dir_all(project.path().join("target")).unwrap();
    fs::write(project.path().join("target").join("app"), "x").unwrap();
    git(project.path(), &["add", "src/lib.rs"]);

    let (_home, editor, _ox) = opened(project.path(), "src/main.rs");

    let rows = tree(&editor);
    assert!(!rows.iter().any(|row| row.contains("target")), "{rows:?}");
    assert!(
        rows.iter()
            .any(|row| row.starts_with("    lib.rs") && row.ends_with('A')),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.starts_with("    main.rs") && row.ends_with('U')),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.starts_with("▾ src") && row.ends_with('●')),
        "{rows:?}"
    );
}
