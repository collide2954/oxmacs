use std::fs;
use std::path::{MAIN_SEPARATOR, Path};

use oxmacs_core::editor::Editor;
use oxmacs_core::theme::Highlight;

use crate::Ox;
use crate::testing::{at_point, press, question, settle, typed};

fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello").unwrap();
    fs::write(dir.path().join("b.txt"), "bee").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub").join("c.txt"), "sea").unwrap();
    dir
}

fn listed(dir: &Path) -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    fs::write(
        home.path().join("init.ox"),
        "(use-plugin dired :config {delete-to-trash: false})",
    )
    .unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    editor.open_file(dir);
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn lines(editor: &Editor) -> Vec<String> {
    editor
        .buffer()
        .contents()
        .lines()
        .skip(1)
        .map(str::to_owned)
        .collect()
}

#[test]
fn directories_open_in_a_listing_with_details() {
    let dir = workspace();
    let (_home, editor, _) = listed(dir.path());

    assert_eq!(editor.buffer().name(), dir.path().display().to_string());
    let lines = lines(&editor);
    assert_eq!(lines.len(), 4);
    assert!(lines[0].ends_with(".."), "{lines:?}");
    assert!(lines[1].starts_with("   -  "), "{lines:?}");
    assert!(
        lines[1].ends_with(&format!("  sub{MAIN_SEPARATOR}")),
        "{lines:?}"
    );
    assert!(
        lines[2].starts_with("  5B  ") && lines[2].ends_with("  a.txt"),
        "{lines:?}"
    );
    assert!(
        lines[3].starts_with("  3B  ") && lines[3].ends_with("  b.txt"),
        "{lines:?}"
    );
    assert_eq!(at_point(&editor), format!("sub{MAIN_SEPARATOR}"));
    assert_eq!(
        editor.line_style(2).spans.last().map(|span| span.1),
        Some(Highlight::Link)
    );
}

#[test]
fn listings_move_sort_and_hide_details() {
    let dir = workspace();
    let (_home, mut editor, mut ox) = listed(dir.path());

    press(&mut editor, &mut ox, "(");
    assert_eq!(
        lines(&editor),
        [
            "  ..",
            &format!("  sub{MAIN_SEPARATOR}"),
            "  a.txt",
            "  b.txt"
        ]
    );
    press(&mut editor, &mut ox, "n n n");
    assert_eq!(at_point(&editor), "b.txt");
    press(&mut editor, &mut ox, "p");
    assert_eq!(at_point(&editor), "a.txt");
    press(&mut editor, &mut ox, "s");
    assert!(
        editor
            .buffer()
            .contents()
            .lines()
            .next()
            .unwrap()
            .ends_with("(newest first):")
    );
    assert_eq!(at_point(&editor), "a.txt");
}

#[test]
fn moving_up_from_the_first_file_reaches_the_parent_line() {
    let dir = workspace();
    let (_home, mut editor, mut ox) = listed(dir.path());

    press(&mut editor, &mut ox, "p");
    assert_eq!(at_point(&editor), "..");
    press(&mut editor, &mut ox, "p <up> C-p");
    assert_eq!(at_point(&editor), "..");
    press(&mut editor, &mut ox, "<down>");
    assert_eq!(at_point(&editor), format!("sub{MAIN_SEPARATOR}"));

    press(&mut editor, &mut ox, "<up> RET");
    let parent = dir.path().parent().unwrap().display().to_string();
    assert_eq!(editor.buffer().name(), parent);
}

#[test]
fn return_enters_directories_and_caret_comes_back() {
    let dir = workspace();
    let (_home, mut editor, mut ox) = listed(dir.path());

    press(&mut editor, &mut ox, "RET");
    assert_eq!(
        editor.buffer().name(),
        dir.path().join("sub").display().to_string()
    );
    assert!(lines(&editor).iter().any(|line| line.ends_with("c.txt")));
    assert_eq!(editor.buffer_named(&dir.path().display().to_string()), None);

    press(&mut editor, &mut ox, "^");
    assert_eq!(editor.buffer().name(), dir.path().display().to_string());
    assert_eq!(at_point(&editor), format!("sub{MAIN_SEPARATOR}"));

    press(&mut editor, &mut ox, "n RET");
    assert_eq!(editor.buffer().contents(), "hello");
}

#[test]
fn marks_and_flags_show_and_x_deletes_after_asking() {
    let dir = workspace();
    let (_home, mut editor, mut ox) = listed(dir.path());

    press(&mut editor, &mut ox, "n m");
    assert!(lines(&editor)[2].starts_with('*'));
    assert_eq!(editor.line_style(3).background, Some(Highlight::Marked));
    assert_eq!(at_point(&editor), "b.txt");
    press(&mut editor, &mut ox, "d");
    assert!(lines(&editor)[3].starts_with('D'));
    press(&mut editor, &mut ox, "t");
    assert!(lines(&editor)[2].starts_with(' '));
    assert!(lines(&editor)[1].starts_with('*'));
    press(&mut editor, &mut ox, "U x");
    assert_eq!(editor.message(), Some("(No deletions requested)"));

    press(&mut editor, &mut ox, "p d d x");
    assert_eq!(
        question(&editor),
        Some("Delete a.txt and b.txt? (y or n) ".to_owned())
    );
    press(&mut editor, &mut ox, "y");

    assert!(!dir.path().join("a.txt").exists());
    assert!(!dir.path().join("b.txt").exists());
    assert_eq!(lines(&editor).len(), 2);
    assert_eq!(editor.message(), Some("Deleted 2 files"));
}

#[test]
fn files_are_renamed_copied_and_directories_made() {
    let dir = workspace();
    let (_home, mut editor, mut ox) = listed(dir.path());

    press(&mut editor, &mut ox, "n R");
    assert_eq!(editor.minibuffer().unwrap().prompt(), "Rename a.txt to: ");
    typed(&mut editor, &mut ox, "moved.txt");
    press(&mut editor, &mut ox, "RET");
    assert!(dir.path().join("moved.txt").exists());
    assert!(!dir.path().join("a.txt").exists());
    assert_eq!(editor.message(), Some("Moved 1 file"));

    press(&mut editor, &mut ox, "g");
    let line = lines(&editor)
        .iter()
        .position(|line| line.ends_with("b.txt"))
        .unwrap();
    let start = editor.buffer().line_start(line + 1);
    editor.buffer_mut().set_point(start);
    press(&mut editor, &mut ox, "C");
    typed(&mut editor, &mut ox, "sub");
    press(&mut editor, &mut ox, "RET RET");
    assert_eq!(
        fs::read_to_string(dir.path().join("sub").join("b.txt")).unwrap(),
        "bee"
    );
    assert_eq!(editor.message(), Some("Copied 1 file"));

    press(&mut editor, &mut ox, "+");
    typed(&mut editor, &mut ox, "fresh");
    press(&mut editor, &mut ox, "RET");
    assert!(dir.path().join("fresh").is_dir());
    assert_eq!(at_point(&editor), format!("fresh{MAIN_SEPARATOR}"));
}

#[test]
fn renaming_moves_the_buffers_visiting_what_moved() {
    let dir = workspace();
    let (_home, mut editor, mut ox) = listed(dir.path());
    editor.open_file(&dir.path().join("a.txt"));
    editor.open_file(&dir.path().join("sub").join("c.txt"));
    editor.open_file(dir.path());
    settle(&mut editor, &mut ox);
    let visiting = |editor: &Editor, name: &str| {
        editor
            .buffer_named(name)
            .and_then(|id| editor.buffer_by_id(id))
            .and_then(|buffer| buffer.file())
            .map(|file| file.path.clone())
    };

    assert_eq!(at_point(&editor), format!("sub{MAIN_SEPARATOR}"));
    press(&mut editor, &mut ox, "R");
    typed(&mut editor, &mut ox, "inner");
    press(&mut editor, &mut ox, "RET g");
    let line = lines(&editor)
        .iter()
        .position(|line| line.ends_with("a.txt"))
        .unwrap();
    let start = editor.buffer().line_start(line + 1);
    editor.buffer_mut().set_point(start);
    press(&mut editor, &mut ox, "R");
    typed(&mut editor, &mut ox, "moved.txt");
    press(&mut editor, &mut ox, "RET");

    assert_eq!(
        visiting(&editor, "c.txt"),
        Some(dir.path().join("inner").join("c.txt"))
    );
    assert_eq!(
        visiting(&editor, "moved.txt"),
        Some(dir.path().join("moved.txt"))
    );
    assert_eq!(visiting(&editor, "a.txt"), None);
}

#[test]
fn marks_made_while_a_listing_refreshes_are_kept() {
    let dir = workspace();
    let (_home, mut editor, mut ox) = listed(dir.path());
    press(&mut editor, &mut ox, "n");

    for key in ["g", "m"] {
        let key: oxmacs_core::input::KeySequence = key.parse().unwrap();
        editor.handle_key(key.keys()[0]);
    }
    settle(&mut editor, &mut ox);

    assert!(lines(&editor)[2].starts_with('*'), "{:?}", lines(&editor));
}

#[test]
fn listings_follow_changes_on_disk() {
    let dir = workspace();
    let (_home, mut editor, mut ox) = listed(dir.path());
    assert!(
        editor
            .watched_directories()
            .contains(&dir.path().to_path_buf())
    );

    fs::write(dir.path().join("later.txt"), "new").unwrap();
    editor.check_disk_changes();
    settle(&mut editor, &mut ox);

    assert!(
        lines(&editor)
            .iter()
            .any(|line| line.ends_with("later.txt"))
    );
}

#[test]
fn dired_jump_lists_the_current_file() {
    let dir = workspace();
    let (_home, mut editor, mut ox) = listed(dir.path());
    editor.open_file(&dir.path().join("b.txt"));

    press(&mut editor, &mut ox, "C-x C-j");

    assert_eq!(editor.buffer().name(), dir.path().display().to_string());
    assert_eq!(at_point(&editor), "b.txt");
}
