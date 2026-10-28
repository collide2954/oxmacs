use oxmacs_core::display::LineStyle;
use oxmacs_core::editor::Editor;
use oxmacs_core::special_buffer::SpecialBuffer;
use oxmacs_core::theme::Highlight;
use oxmacs_core::view::overlays::{Look, Piece, Row, shown};

use crate::Ox;
use crate::api::buffers::LinesBuffer;
use crate::testing::{press, settle};

fn opened(source: &str) -> (Editor, Ox) {
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.eval(&mut editor, source).unwrap();
    settle(&mut editor, &mut ox);
    (editor, ox)
}

const SHAPES: &str = r#"[{line: ["see " ["docs" :link :link "manual"] " or " ["main" :type :pill] " " ["C-x" nil :keycap]] background: :region name: "first"}]"#;

#[test]
fn lines_buffers_headers_and_overlays_read_the_same_pieces() {
    let (editor, _) = opened(&format!(
        "(open-buffer \"*shapes*\" {SHAPES}) (set-header \"*shapes*\" {SHAPES}) (set-overlay :shapes {SHAPES})"
    ));

    let style = LineStyle {
        background: Some(Highlight::Region),
        spans: vec![(4..8, Highlight::Link), (12..16, Highlight::Type)],
    };
    assert_eq!(editor.buffer().contents(), "see docs or main C-x");
    assert_eq!(editor.line_style(0), style);
    let (_, view) = editor
        .special_buffer::<LinesBuffer>(editor.buffer().id())
        .unwrap();
    assert_eq!(view.pills(0), [(12..16, Highlight::Type)]);
    let docs = 4..8;
    assert_eq!(view.links(0), [docs]);
    assert_eq!(
        editor.pane().header_lines(),
        [("see docs or main C-x".to_owned(), style)]
    );
    let piece = |text: &str, look| Piece {
        text: text.to_owned(),
        look,
    };
    assert_eq!(
        shown(&editor)[0].rows,
        [Row {
            pieces: vec![
                piece("see ", Look::Plain),
                piece("docs", Look::Face(Highlight::Link)),
                piece(" or ", Look::Plain),
                piece("main", Look::Face(Highlight::Type)),
                piece(" ", Look::Plain),
                piece("C-x", Look::Keycap),
            ],
            opacity: 1.0,
        }]
    );
}

#[test]
fn mistakes_in_pieces_are_explained_the_same_everywhere() {
    let mut editor = Editor::new();
    let mut ox = Ox::new();

    for native in [
        "(open-buffer \"*x*\" [[[\"a\" :nope]]])",
        "(set-overlay :x [[[\"a\" :nope]]])",
    ] {
        assert!(
            ox.eval(&mut editor, native)
                .unwrap_err()
                .message
                .ends_with("doesn't know the face :nope"),
            "{native}"
        );
    }
    assert_eq!(
        ox.eval(&mut editor, r#"(open-buffer "*x*" [[["a" :link :url]]])"#)
            .unwrap_err()
            .message,
        "open-buffer expects text, [text face], [text face :pill], [text face :keycap] or [text face :link target] as a piece, got [\"a\" :link :url]"
    );
}

#[test]
fn map_lines_are_items_found_by_their_fields() {
    let (mut editor, mut ox) = opened(
        r#"(open-buffer "*items*" ["Files" {line: "a.txt" name: "a.txt" size: 3} "" {line: "b.txt" name: "b.txt" size: 5}])"#,
    );
    let eval = |ox: &mut Ox, editor: &mut Editor, source: &str| ox.eval(editor, source).unwrap();

    assert_eq!(eval(&mut ox, &mut editor, "(line-item)"), "nil");
    assert_eq!(
        eval(
            &mut ox,
            &mut editor,
            "[(get (next-item) :name) (current-line)]"
        ),
        r#"["a.txt" 1]"#
    );
    assert_eq!(
        eval(
            &mut ox,
            &mut editor,
            "[(get (next-item) :name) (current-line) (next-item)]"
        ),
        r#"["b.txt" 3 nil]"#
    );
    assert_eq!(
        eval(
            &mut ox,
            &mut editor,
            "[(get (next-item -1) :size) (line-item 3)]"
        ),
        r#"[3 {line: "b.txt" name: "b.txt" size: 5}]"#
    );
    assert_eq!(
        eval(
            &mut ox,
            &mut editor,
            r#"[(get (goto-item {name: "b.txt"}) :size) (current-line) (goto-item {name: "c.txt"})]"#
        ),
        "[5 3 nil]"
    );

    eval(
        &mut ox,
        &mut editor,
        r#"(set-lines "*items*" [{line: "c.txt" name: "c.txt"}]) (append-lines "*items*" ["" {line: "d.txt" name: "d.txt"}])"#,
    );
    assert_eq!(
        eval(
            &mut ox,
            &mut editor,
            "[(get (line-item 0) :name) (line-item 1) (get (line-item 2) :name)]"
        ),
        r#"["c.txt" nil "d.txt"]"#
    );

    eval(&mut ox, &mut editor, r#"(close-buffer "*items*")"#);
    settle(&mut editor, &mut ox);
    assert!(
        editor
            .plugin_state::<super::Kept>()
            .is_some_and(|kept| kept.0.is_empty())
    );
}

#[test]
fn links_call_their_buffers_on_link_from_keys_and_clicks() {
    let (mut editor, mut ox) = opened(
        r#"(def followed (atom [])) (open-buffer "*links*" ["Read" ["see " ["docs" :link :link "manual"] " and " ["code" :link :link {file: "a.ox"}]]] :keys {"RET" 'follow-link "TAB" 'next-link "S-TAB" 'previous-link} :on-link (fn [target] (swap! followed conj target)))"#,
    );

    press(&mut editor, &mut ox, "TAB RET TAB RET");
    press(&mut editor, &mut ox, "S-TAB");
    assert_eq!(editor.buffer().point(), 9);

    assert_eq!(
        ox.eval(&mut editor, "@followed").unwrap(),
        r#"["manual" {file: "a.ox"}]"#
    );
}
