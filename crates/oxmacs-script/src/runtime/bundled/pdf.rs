use std::path::PathBuf;

use oxmacs_core::editor::Editor;
use oxmacs_core::view::UiEvent;

use crate::Ox;
use crate::testing::{press, settle, typed};
use oxmacs_core::view::pdf::{Go, View, Zoom};
use oxmacs_core::view::{Arg, Node, ViewBuffer};
use oxmacs_pdf::{Document, Heading, Link, PageSize, PageText, Rect, Target};

const LETTER: PageSize = PageSize {
    width: 612.0,
    height: 792.0,
};

fn line(text: &str) -> PageText {
    PageText {
        text: text.chars().collect(),
        boxes: (0..text.chars().count())
            .map(|at| {
                let left = 72.0 + 6.0 * f32::from(u16::try_from(at).unwrap());
                Rect {
                    left,
                    top: 100.0,
                    right: left + 5.0,
                    bottom: 110.0,
                }
            })
            .collect(),
        links: Vec::new(),
    }
}

struct Viewer {
    _home: tempfile::TempDir,
    papers: tempfile::TempDir,
    editor: Editor,
    ox: Ox,
    paper: PathBuf,
}

fn viewer(config: &str) -> Viewer {
    let home = tempfile::tempdir().unwrap();
    let papers = tempfile::tempdir().unwrap();
    let paper = papers.path().join("paper.pdf");
    std::fs::write(&paper, "%PDF-1.7").unwrap();
    let mut first = line("Hello world");
    first.links.push(Link {
        rect: Rect {
            left: 72.0,
            top: 100.0,
            right: 100.0,
            bottom: 110.0,
        },
        target: Target::Page(3),
    });
    oxmacs_pdf::preload(
        &paper,
        Document {
            pages: vec![LETTER; 5],
            outline: vec![
                Heading {
                    title: "Introduction".to_owned(),
                    page: 0,
                    depth: 0,
                },
                Heading {
                    title: "Details".to_owned(),
                    page: 2,
                    depth: 1,
                },
            ],
            title: Some("A Study of Needles".to_owned()),
            author: Some("Ada Lovelace".to_owned()),
        },
        vec![
            first,
            line("second page"),
            line("third"),
            line("a needle here"),
            line("another needle"),
        ],
    );
    std::fs::write(
        home.path().join("init.ox"),
        format!("(use-plugin pdf{config})"),
    )
    .unwrap();
    let mut editor = Editor::new();
    editor.set_tools_root(Some(home.path().join("tools")));
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    Viewer {
        _home: home,
        papers,
        editor,
        ox,
        paper,
    }
}

impl Viewer {
    fn open(&mut self) {
        let paper = self.paper.clone();
        assert!(self.editor.open_file(&paper));
        settle(&mut self.editor, &mut self.ox);
    }

    fn view(&self) -> View<usize> {
        let id = self.editor.buffer_named("paper.pdf").unwrap();
        match &self.editor.special_buffer::<ViewBuffer>(id).unwrap().1.node {
            Node::Pdf(view) => (**view).clone(),
            other => panic!("not a pdf: {other:?}"),
        }
    }

    fn go(&self) -> Option<Go> {
        self.view().go.map(|(_, to)| to)
    }

    fn call(&mut self, slot: usize, args: &[Arg]) {
        let id = self.editor.buffer_named("paper.pdf").unwrap();
        self.ox.event(
            &mut self.editor,
            UiEvent::Call {
                buffer: id,
                slot,
                args: args.to_vec(),
            },
        );
        settle(&mut self.editor, &mut self.ox);
    }

    fn viewed(&mut self, page: i64, percent: i64) {
        self.call(0, &[Arg::Int(page), Arg::Int(percent)]);
    }

    fn press(&mut self, keys: &str) {
        press(&mut self.editor, &mut self.ox, keys);
    }
}

#[test]
fn pdfs_open_as_pages_with_their_place_in_the_mode_line() {
    let mut viewer = viewer("");

    viewer.open();
    viewer.viewed(0, 100);

    let view = viewer.view();
    assert_eq!(view.path, viewer.paper);
    assert_eq!(view.zoom, Zoom::FitWidth);
    assert!(view.continuous && !view.dark);
    assert_eq!(viewer.editor.mode_line_segments(), ["1/5 · 100%"]);
    assert_eq!(
        viewer.editor.message(),
        Some("A Study of Needles by Ada Lovelace · 5 pages")
    );
}

fn shown_path(editor: &Editor, name: &str) -> Option<PathBuf> {
    let id = editor.buffer_named(name)?;
    match &editor.special_buffer::<ViewBuffer>(id)?.1.node {
        Node::Pdf(view) => Some(view.path.clone()),
        _ => None,
    }
}

#[test]
fn a_closed_pdf_opens_again_and_one_of_the_same_name_gets_its_own_buffer() {
    let mut viewer = viewer("");
    viewer.open();
    viewer.press("C-x k RET");
    assert!(viewer.editor.buffer_named("paper.pdf").is_none());

    viewer.open();
    assert_eq!(viewer.editor.buffer().name(), "paper.pdf");
    assert_eq!(
        shown_path(&viewer.editor, "paper.pdf"),
        Some(viewer.paper.clone())
    );

    let other = viewer.papers.path().join("drafts").join("paper.pdf");
    std::fs::create_dir_all(other.parent().unwrap()).unwrap();
    std::fs::write(&other, "%PDF-1.7").unwrap();
    oxmacs_pdf::preload(
        &other,
        Document {
            pages: vec![LETTER; 2],
            outline: Vec::new(),
            title: None,
            author: None,
        },
        vec![line("draft"), line("end")],
    );
    assert!(viewer.editor.open_file(&other));
    settle(&mut viewer.editor, &mut viewer.ox);

    let first = format!(
        "paper.pdf<{}>",
        viewer.papers.path().file_name().unwrap().to_string_lossy()
    );
    assert_eq!(viewer.editor.buffer().name(), "paper.pdf<drafts>");
    assert_eq!(shown_path(&viewer.editor, "paper.pdf<drafts>"), Some(other));
    assert_eq!(
        shown_path(&viewer.editor, &first),
        Some(viewer.paper.clone())
    );
    assert!(viewer.editor.open_file(&viewer.paper));
    assert_eq!(viewer.editor.buffer().name(), first);
}

#[test]
fn keys_turn_scroll_and_jump_between_pages() {
    let mut viewer = viewer("");
    viewer.open();
    viewer.viewed(1, 100);

    viewer.press("n");
    assert_eq!(viewer.go(), Some(Go::Page { page: 2, y: None }));
    viewer.press("SPC");
    assert_eq!(viewer.go(), Some(Go::Screens(1.0)));
    viewer.press("C-p");
    assert_eq!(viewer.go(), Some(Go::Lines(-1.0)));
    viewer.press("M->");
    assert_eq!(viewer.go(), Some(Go::Page { page: 4, y: None }));

    viewer.press("M-g g");
    typed(&mut viewer.editor, &mut viewer.ox, "2");
    viewer.press("RET");
    assert_eq!(viewer.go(), Some(Go::Page { page: 1, y: None }));

    viewer.viewed(4, 100);
    viewer.press("n");
    assert_eq!(viewer.editor.message(), Some("This is the last page"));
}

#[test]
fn zooming_and_layouts_change_the_view() {
    let mut viewer = viewer(" :config {continuous: false zoom: :fit-page}");
    viewer.open();
    let view = viewer.view();
    assert_eq!(view.zoom, Zoom::FitPage);
    assert!(!view.continuous);
    viewer.viewed(0, 100);

    viewer.press("+");
    assert_eq!(viewer.view().zoom, Zoom::Scale(1.25));
    viewer.press("=");
    assert_eq!(viewer.view().zoom, Zoom::Scale(1.0));
    viewer.press("W");
    assert_eq!(viewer.view().zoom, Zoom::FitWidth);
    viewer.press("c m");
    let view = viewer.view();
    assert!(view.continuous && view.dark);
}

#[test]
fn searching_marks_every_match_and_steps_through_them() {
    let mut viewer = viewer("");
    viewer.open();
    viewer.viewed(0, 100);

    viewer.press("s");
    typed(&mut viewer.editor, &mut viewer.ox, "needle");
    viewer.press("RET");

    assert_eq!(viewer.editor.message(), Some("1 of 2 for needle"));
    viewer.viewed(3, 100);
    assert_eq!(
        viewer.go(),
        Some(Go::Page {
            page: 3,
            y: Some(100.0)
        })
    );
    let marks = viewer.view().marks;
    assert_eq!(marks.len(), 2);
    assert!(marks[0].current && !marks[1].current);
    assert_eq!(marks[0].page, 3);

    viewer.press("C-s");
    assert_eq!(viewer.editor.message(), Some("2 of 2 for needle"));
    viewer.press("C-s");
    assert_eq!(viewer.editor.message(), Some("1 of 2 for needle"));
    viewer.press("C-r");
    assert_eq!(viewer.editor.message(), Some("2 of 2 for needle"));
}

#[test]
fn the_outline_jumps_to_its_headings() {
    let mut viewer = viewer("");
    viewer.open();

    viewer.press("o");
    typed(&mut viewer.editor, &mut viewer.ox, "Details");
    viewer.press("RET");

    assert_eq!(viewer.go(), Some(Go::Page { page: 2, y: None }));
}

#[test]
fn dragged_text_is_copied_and_links_are_followed() {
    let mut viewer = viewer("");
    viewer.open();

    viewer.press("M-w");
    assert_eq!(viewer.editor.message(), Some("Drag over some text first"));
    viewer.call(1, &[Arg::Text("Hello".to_owned())]);
    viewer.press("M-w");
    assert_eq!(viewer.editor.message(), Some("Copied"));

    viewer.call(2, &[Arg::Int(3)]);
    assert_eq!(viewer.go(), Some(Go::Page { page: 3, y: None }));
    viewer.call(2, &[Arg::Text("file:///etc/passwd".to_owned())]);
    assert_eq!(viewer.editor.message(), Some("Link: file:///etc/passwd"));
}

#[test]
fn documents_pdfium_cannot_read_say_so_in_the_view() {
    let mut viewer = viewer("");
    let elsewhere = viewer.papers.path().join("unread.pdf");
    std::fs::write(&elsewhere, "%PDF-1.7").unwrap();

    assert!(viewer.editor.open_file(&elsewhere));
    settle(&mut viewer.editor, &mut viewer.ox);

    let id = viewer.editor.buffer_named("unread.pdf").unwrap();
    let shown = format!(
        "{:?}",
        viewer
            .editor
            .special_buffer::<ViewBuffer>(id)
            .unwrap()
            .1
            .node
    );
    assert!(shown.contains("PDFium"), "{shown}");
}
