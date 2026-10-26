use std::io::Write;
use std::path::{Path, PathBuf};

use oxmacs_core::editor::Editor;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::Ox;
use crate::book::{At, Block, Kind, Pages};
use crate::testing::{press, settle, typed};
use crate::views::{Arg, Node, ViewBuffer};

const PIXEL: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn chapter(title: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><!DOCTYPE html><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{title}</title></head><body><h1>{title}</h1>{body}</body></html>"
    )
}

fn sample(folder: &Path) -> PathBuf {
    let path = folder.join("sample.epub");
    let mut zip = ZipWriter::new(std::fs::File::create(&path).unwrap());
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut add = |name: &str, bytes: &[u8], options| {
        zip.start_file(name, options).unwrap();
        zip.write_all(bytes).unwrap();
    };
    add("mimetype", b"application/epub+zip", stored);
    add(
        "META-INF/container.xml",
        br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        deflated,
    );
    add(
        "OEBPS/content.opf",
        br#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>A Sample Voyage</dc:title><dc:creator>Ada Lovelace</dc:creator></metadata><manifest><item id="nav" href="nav.xhtml" properties="nav" media-type="application/xhtml+xml"/><item id="c1" href="text/one.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="text/two.xhtml" media-type="application/xhtml+xml"/><item id="c3" href="text/three.xhtml" media-type="application/xhtml+xml"/><item id="cover" href="images/cover.png" properties="cover-image" media-type="image/png"/></manifest><spine><itemref idref="c1"/><itemref idref="c2"/><itemref idref="c3"/></spine></package>"#,
        deflated,
    );
    add(
        "OEBPS/nav.xhtml",
        br#"<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol><li><a href="text/one.xhtml">Chapter One</a></li><li><a href="text/two.xhtml">Chapter Two</a></li><li><a href="text/three.xhtml#end">The End</a></li></ol></nav></body></html>"#,
        deflated,
    );
    add(
        "OEBPS/text/one.xhtml",
        chapter(
            "Chapter One",
            "<p>Call me <em>Ishmael</em>. Some years ago I went to sea.</p><p>A <a href=\"three.xhtml#end\">whale</a> was seen.</p><p>Third.</p><p>Fourth.</p><p>Fifth.</p>",
        )
        .as_bytes(),
        deflated,
    );
    add(
        "OEBPS/text/two.xhtml",
        chapter(
            "Chapter Two",
            "<p>The sea was calm.</p><p>No whale today.</p>",
        )
        .as_bytes(),
        deflated,
    );
    add(
        "OEBPS/text/three.xhtml",
        chapter("The End", "<p>Almost done.</p><p id=\"end\">Fin.</p>").as_bytes(),
        deflated,
    );
    add("OEBPS/images/cover.png", PIXEL, stored);
    zip.finish().unwrap();
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

    fn pages(&self) -> Pages {
        let id = self.editor.buffer_named("sample.epub").unwrap();
        match &self.editor.plugin_buffer::<ViewBuffer>(id).unwrap().1.node {
            Node::Pages(pages) => (**pages).clone(),
            other => panic!("not pages: {other:?}"),
        }
    }

    fn call(&mut self, slot: usize, args: &[Arg]) {
        let id = self.editor.buffer_named("sample.epub").unwrap();
        self.ox.call(&mut self.editor, id, slot, args);
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

    reader.call(3, &[Arg::Text("OEBPS/text/three.xhtml#end".to_owned())]);
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

    let saved =
        std::fs::read_to_string(reader.home.path().join("epub").join("books.json")).unwrap();
    assert!(saved.contains("\"text\":\"Call me\""), "{saved}");

    let home = reader.home.path().to_owned();
    let mut editor = Editor::new();
    let mut ox = Ox::new();
    ox.start(&mut editor, Some(&home));
    editor.open_file(&reader.book);
    settle(&mut editor, &mut ox);
    let id = editor.buffer_named("sample.epub").unwrap();
    let Node::Pages(pages) = &editor.plugin_buffer::<ViewBuffer>(id).unwrap().1.node else {
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
            .plugin_buffer::<ViewBuffer>(id)
            .unwrap()
            .1
            .node
    );
    assert!(shown.contains("A Sample Voyage"), "{shown}");
    assert!(shown.contains("Ada Lovelace"), "{shown}");
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