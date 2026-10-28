use std::path::{Path, PathBuf};

use oxmacs_core::editor::Editor;
use oxmacs_core::view::UiEvent;

use crate::Ox;
use crate::testing::{Chapter, Epub, PIXEL, press, settle, typed, write_epub};
use oxmacs_core::view::book::{At, Block, Kind, Pages};
use oxmacs_core::view::{Arg, Node, ViewBuffer};

const SAMPLE: Epub<'_> = Epub {
    title: "A Sample Voyage",
    author: "Ada Lovelace",
    chapters: &[
        Chapter {
            title: "Chapter One",
            body: "<p>Call me <em>Ishmael</em>. Some years ago I went to sea.</p><p>A <a href=\"3.xhtml#end\">whale</a> was seen.</p><p>Third.</p><p>Fourth.</p><p>Fifth.</p>",
            anchor: None,
        },
        Chapter {
            title: "Chapter Two",
            body: "<p>The sea was calm.</p><p>No whale today.</p>",
            anchor: None,
        },
        Chapter {
            title: "The End",
            body: "<p>Almost done.</p><p id=\"end\">Fin.</p>",
            anchor: Some("end"),
        },
    ],
    cover: PIXEL,
};

fn sample(folder: &Path) -> PathBuf {
    let path = folder.join("sample.epub");
    write_epub(&path, &SAMPLE);
    path
}

struct Reader {
    home: tempfile::TempDir,
    books: tempfile::TempDir,
    editor: Editor,
    ox: Ox,
    book: PathBuf,
}

fn reader(config: &str) -> Reader {
    let home = tempfile::tempdir().unwrap();
    let books = tempfile::tempdir().unwrap();
    let book = sample(books.path());
    std::fs::write(
        home.path().join("init.ox"),
        format!("(use-plugin epub{config})"),
    )
    .unwrap();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(home.path()));
    settle(&mut editor, &mut ox);
    Reader {
        home,
        books,
        editor,
        ox,
        book,
    }
}

impl Reader {
    fn open(&mut self) {
        let book = self.book.clone();
        assert!(self.editor.open_file(&book));
        settle(&mut self.editor, &mut self.ox);
    }

    fn pages(&self) -> Pages<usize> {
        let id = self.editor.buffer_named("sample.epub").unwrap();
        match &self.editor.special_buffer::<ViewBuffer>(id).unwrap().1.node {
            Node::Pages(pages) => (**pages).clone(),
            other => panic!("not pages: {other:?}"),
        }
    }

    fn call(&mut self, slot: usize, args: &[Arg]) {
        let id = self.editor.buffer_named("sample.epub").unwrap();
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

    fn paged(&mut self, starts: &[(i64, i64)]) {
        let starts = starts
            .iter()
            .map(|(block, offset)| Arg::List(vec![Arg::Int(*block), Arg::Int(*offset)]))
            .collect();
        self.call(0, &[Arg::List(starts), Arg::Int(1)]);
    }

    fn first_text(&self) -> String {
        self.pages()
            .document
            .0
            .blocks
            .iter()
            .find_map(|block| match block {
                Block::Text { .. } => Some(block.text()),
                _ => None,
            })
            .unwrap_or_default()
    }
}

#[test]
fn opening_an_epub_shows_its_cover_then_its_chapters_in_pages() {
    let mut reader = reader("");
    reader.open();

    let pages = reader.pages();
    assert!(matches!(pages.document.0.blocks[..], [Block::Image { .. }]));
    assert_eq!(pages.header.0, "A Sample Voyage");
    assert_eq!(reader.editor.mode_line_segments(), ["1/4 · 0%"]);
    assert_eq!(
        reader.editor.message(),
        Some("A Sample Voyage by Ada Lovelace")
    );

    reader.paged(&[(0, 0)]);
    press(&mut reader.editor, &mut reader.ox, "SPC");
    let pages = reader.pages();
    assert_eq!(pages.header.1, "Chapter One");
    assert_eq!(reader.first_text(), "Chapter One");
    assert!(matches!(
        &pages.document.0.blocks[1],
        Block::Text { kind: Kind::Body, runs } if runs[1].italic && runs[1].text == "Ishmael"
    ));
}

#[test]
fn a_closed_book_opens_again_and_one_of_the_same_name_gets_its_own_buffer() {
    let mut reader = reader("");
    reader.open();
    press(&mut reader.editor, &mut reader.ox, "C-x k RET");
    assert!(reader.editor.buffer_named("sample.epub").is_none());

    reader.open();
    assert_eq!(reader.editor.buffer().name(), "sample.epub");
    assert_eq!(reader.pages().header.0, "A Sample Voyage");

    let other = reader.books.path().join("more");
    std::fs::create_dir_all(&other).unwrap();
    let second = sample(&other);
    assert!(reader.editor.open_file(&second));
    settle(&mut reader.editor, &mut reader.ox);

    let first = format!(
        "sample.epub<{}>",
        reader.books.path().file_name().unwrap().to_string_lossy()
    );
    assert_eq!(reader.editor.buffer().name(), "sample.epub<more>");
    assert!(reader.editor.buffer_named(&first).is_some());
    assert!(
        reader
            .editor
            .open_file(&reader.books.path().join("sample.epub"))
    );
    assert_eq!(reader.editor.buffer().name(), first);
}

#[test]
fn pages_turn_within_chapters_and_across_them() {
    let mut reader = reader("");
    reader.open();
    press(&mut reader.editor, &mut reader.ox, "]");
    reader.paged(&[(0, 0), (2, 0), (4, 0)]);

    press(&mut reader.editor, &mut reader.ox, "SPC");
    assert_eq!(reader.pages().at, At::Anchor(2, 0));
    press(&mut reader.editor, &mut reader.ox, "SPC SPC");
    assert_eq!(reader.first_text(), "Chapter Two");
    assert_eq!(reader.editor.mode_line_segments(), ["3/4 · 50%"]);

    reader.paged(&[(0, 0)]);
    press(&mut reader.editor, &mut reader.ox, "DEL");
    assert_eq!(reader.first_text(), "Chapter One");
    assert_eq!(reader.pages().at, At::End);
    reader.paged(&[(0, 0), (2, 0), (4, 0)]);
    assert_eq!(reader.pages().at, At::Anchor(4, 0));
}

#[test]
fn contents_and_links_jump_to_their_place() {
    let mut reader = reader("");
    reader.open();

    press(&mut reader.editor, &mut reader.ox, "o");
    typed(&mut reader.editor, &mut reader.ox, "Chapter Two");
    press(&mut reader.editor, &mut reader.ox, "RET");
    assert_eq!(reader.first_text(), "Chapter Two");

    reader.call(3, &[Arg::Text("OEBPS/text/3.xhtml#end".to_owned())]);
    assert_eq!(reader.first_text(), "The End");
    assert_eq!(reader.pages().at, At::Anchor(2, 0));
}

#[test]
fn search_finds_every_match_in_the_book() {
    let mut reader = reader("");
    reader.open();

    press(&mut reader.editor, &mut reader.ox, "s");
    typed(&mut reader.editor, &mut reader.ox, "whale");
    press(&mut reader.editor, &mut reader.ox, "RET");

    assert_eq!(reader.editor.message(), Some("1 of 2 for whale"));
    assert_eq!(reader.first_text(), "Chapter One");
    assert_eq!(reader.pages().marks.len(), 1);
    press(&mut reader.editor, &mut reader.ox, "C-s");
    assert_eq!(reader.editor.message(), Some("2 of 2 for whale"));
    assert_eq!(reader.first_text(), "Chapter Two");
}

#[test]
fn highlights_and_places_are_remembered() {
    let mut reader = reader("");
    reader.open();
    press(&mut reader.editor, &mut reader.ox, "]");
    reader.paged(&[(0, 0), (2, 0)]);
    reader.call(
        2,
        &[
            Arg::List(vec![Arg::Int(1), Arg::Int(0)]),
            Arg::List(vec![Arg::Int(1), Arg::Int(7)]),
        ],
    );

    press(&mut reader.editor, &mut reader.ox, "h");
    assert_eq!(reader.editor.message(), Some("Highlighted"));
    assert_eq!(reader.pages().marks.len(), 1);
    press(&mut reader.editor, &mut reader.ox, "H");
    let notes = reader.editor.buffer().contents();
    assert!(notes.contains("Chapter One\n  Call me\n"), "{notes}");

    let saved = std::fs::read_to_string(
        reader
            .home
            .path()
            .join("data")
            .join("epub")
            .join("books.json"),
    )
    .unwrap();
    assert!(saved.contains("\"text\":\"Call me\""), "{saved}");

    let home = reader.home.path().to_owned();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(&home));
    editor.open_file(&reader.book);
    settle(&mut editor, &mut ox);
    let id = editor.buffer_named("sample.epub").unwrap();
    let Node::Pages(pages) = &editor.special_buffer::<ViewBuffer>(id).unwrap().1.node else {
        panic!("not pages");
    };
    assert_eq!(pages.header.1, "Chapter One");
    assert_eq!(pages.marks.len(), 1);
}

#[test]
fn the_library_shows_the_books_in_a_folder() {
    let mut reader = reader("");
    let folder = reader.books.path().display().to_string();
    reader
        .ox
        .eval(
            &mut reader.editor,
            &format!("(configure-plugin 'epub {{library: {folder:?}}})"),
        )
        .unwrap();

    press(&mut reader.editor, &mut reader.ox, "M-x");
    typed(&mut reader.editor, &mut reader.ox, "epub-library");
    press(&mut reader.editor, &mut reader.ox, "RET");

    assert_eq!(reader.editor.buffer().name(), "*epub library*");
    let id = reader.editor.buffer_named("*epub library*").unwrap();
    let shown = format!(
        "{:?}",
        reader
            .editor
            .special_buffer::<ViewBuffer>(id)
            .unwrap()
            .1
            .node
    );
    assert!(shown.contains("A Sample Voyage"), "{shown}");
    assert!(shown.contains("Ada Lovelace"), "{shown}");
}

#[test]
fn the_library_shows_how_far_each_book_was_read_however_its_folder_is_spelled() {
    let mut reader = reader("");
    reader.open();
    press(&mut reader.editor, &mut reader.ox, "]");
    reader.paged(&[(0, 0), (2, 0)]);
    let folder = reader.books.path().display().to_string().replace('\\', "/");
    reader
        .ox
        .eval(
            &mut reader.editor,
            &format!("(configure-plugin 'epub {{library: {folder:?}}})"),
        )
        .unwrap();

    press(&mut reader.editor, &mut reader.ox, "M-x");
    typed(&mut reader.editor, &mut reader.ox, "epub-library");
    press(&mut reader.editor, &mut reader.ox, "RET");

    let id = reader.editor.buffer_named("*epub library*").unwrap();
    let shown = format!(
        "{:?}",
        reader
            .editor
            .special_buffer::<ViewBuffer>(id)
            .unwrap()
            .1
            .node
    );
    assert!(shown.contains("% read"), "{shown}");
}

#[test]
fn files_that_are_not_books_say_so_plainly() {
    let mut reader = reader("");
    let broken = reader.books.path().join("broken.epub");
    std::fs::write(&broken, "not a zip").unwrap();

    assert!(reader.editor.open_file(&broken));
    settle(&mut reader.editor, &mut reader.ox);
    let said = reader.editor.message().unwrap_or_default().to_owned();
    assert!(said.starts_with("Couldn't open broken.epub"), "{said}");
    assert_eq!(reader.editor.buffer_named("broken.epub"), None);

    press(&mut reader.editor, &mut reader.ox, "C-x 2");
    assert_eq!(reader.editor.message(), None);
}
