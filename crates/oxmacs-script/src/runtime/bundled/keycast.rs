use oxmacs_core::editor::Editor;

use crate::Ox;
use crate::testing::{press, settle, typed};
use crate::view::overlays::{self, Look, Place};

fn started(init: &str) -> (tempfile::TempDir, Editor, Ox) {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("init.ox"), init).unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    (home, editor, ox)
}

fn casting(config: &str) -> (tempfile::TempDir, Editor, Ox) {
    started(&format!("(use-plugin keycast{config})"))
}

fn rows(editor: &Editor) -> Vec<String> {
    overlays::shown(editor)
        .iter()
        .flat_map(|overlay| overlay.rows.iter())
        .map(|row| {
            row.pieces
                .iter()
                .map(|piece| match piece.look {
                    Look::Keycap => format!("[{}]", piece.text),
                    _ => piece.text.clone(),
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

fn opacities(editor: &Editor) -> Vec<f32> {
    overlays::shown(editor)
        .iter()
        .flat_map(|overlay| overlay.rows.iter())
        .map(|row| row.opacity)
        .collect()
}

fn evaluated(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
    match ox.eval(
        editor,
        &format!("(module (import (keycast stack :as stack))) {source}"),
    ) {
        Ok(shown) => shown,
        Err(error) => format!("error: {}", error.message),
    }
}

#[test]
fn keys_show_only_once_keycast_is_used() {
    let (_home, mut editor, mut ox) = started("");
    press(&mut editor, &mut ox, "C-x h");
    assert_eq!(rows(&editor), Vec::<String>::new());

    let (_home, mut editor, mut ox) = casting("");
    press(&mut editor, &mut ox, "C-x h");

    assert_eq!(rows(&editor), ["[C-x] [h] mark-whole-buffer"]);
    let shown = overlays::shown(&editor);
    assert_eq!(shown[0].place, Place::BottomRight);
    assert!((shown[0].scale - 1.0).abs() < f32::EPSILON);
}

#[test]
fn typing_plain_newlines_and_indents_never_show() {
    let (_home, mut editor, mut ox) = casting("");

    typed(&mut editor, &mut ox, "hello there");
    press(&mut editor, &mut ox, "RET TAB <delete>");

    assert_eq!(rows(&editor), Vec::<String>::new());
    assert!(editor.buffer().contents().starts_with("hello there\n"));
    assert_eq!(
        evaluated(
            &mut editor,
            &mut ox,
            r#"(map stack/shown? [{keys: ["RET"] command: "newline"} {keys: ["TAB"] command: "indent-for-tab-command"}
                                  {keys: ["TAB"] command: "ox-indent-or-complete"} {keys: ["RET"] command: "dired-find-file"}
                                  {keys: ["TAB"] command: "git-toggle"} {keys: [] command: "isearch-printing-char"}])"#
        ),
        "[false false false true true false]"
    );
}

#[test]
fn chords_named_keys_and_keys_special_buffers_bind_show() {
    let (_home, mut editor, mut ox) = casting("");

    assert_eq!(
        evaluated(
            &mut editor,
            &mut ox,
            r#"(map (fn [keys] (stack/shown? {keys: keys command: "x"}))
                 [["C-x" "C-s"] ["M-x"] ["s-a"] ["C--"] ["C-M-%"] ["RET"] ["TAB"] ["ESC" "ESC"] ["<up>"] ["S-<left>"] ["<f5>"] ["n"] ["SPC"] ["g" "r"]
                  ["DEL"] ["<home>"] ["<delete>"] ["<next>"] ["S-<home>"]])"#
        ),
        "[true true true true true true true true true true true true true true true true false true true]"
    );
    ox.eval(
        &mut editor,
        r#"(module (import (ox ui :only [text])))
(defcommand bump "Bump" (message "bumped"))
(open-view "*keys*" (fn [] (text "k")) :keys {n: 'bump})"#,
    )
    .unwrap();
    settle(&mut editor, &mut ox);
    press(&mut editor, &mut ox, "n");

    assert_eq!(rows(&editor), ["[n] bump"]);
}

#[test]
fn repeats_count_up_and_the_newest_three_stay_with_older_ones_fainter() {
    let (_home, mut editor, mut ox) = casting("");

    press(&mut editor, &mut ox, "C-n C-n C-n C-n");
    assert_eq!(rows(&editor), ["[C-n] ×4 next-line"]);

    press(&mut editor, &mut ox, "C-p");
    typed(&mut editor, &mut ox, "a");
    press(&mut editor, &mut ox, "C-p");
    assert_eq!(
        rows(&editor),
        [
            "[C-n] ×4 next-line",
            "[C-p] previous-line",
            "[C-p] previous-line"
        ]
    );

    press(&mut editor, &mut ox, "C-p C-f C-b");
    assert_eq!(
        rows(&editor),
        [
            "[C-p] ×2 previous-line",
            "[C-f] forward-char",
            "[C-b] backward-char"
        ]
    );
    assert_eq!(opacities(&editor), [0.35, 0.6, 1.0]);
}

#[test]
fn m_x_shows_the_command_it_ran() {
    let (_home, mut editor, mut ox) = casting("");

    press(&mut editor, &mut ox, "M-x");
    assert_eq!(rows(&editor), Vec::<String>::new());
    typed(&mut editor, &mut ox, "end-of-buffer");
    press(&mut editor, &mut ox, "RET");

    assert_eq!(rows(&editor), ["[M-x] end-of-buffer"]);
}

#[test]
fn keys_can_show_large_at_the_bottom_center_or_in_the_mode_line() {
    let (_home, mut editor, mut ox) = casting(" :config {placement: :bottom-center}");
    press(&mut editor, &mut ox, "C-x h");
    let shown = overlays::shown(&editor);
    assert_eq!(shown[0].place, Place::BottomCenter);
    assert!(shown[0].scale > 1.5);

    let (_home, mut editor, mut ox) = casting(" :config {placement: :mode-line}");
    press(&mut editor, &mut ox, "C-x h C-n C-n");

    assert_eq!(rows(&editor), Vec::<String>::new());
    assert_eq!(editor.mode_line_segments(), ["[C-n] ×2 next-line"]);
}
