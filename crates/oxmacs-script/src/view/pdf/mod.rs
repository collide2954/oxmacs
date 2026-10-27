use std::path::PathBuf;

use ox_lang::heap::Heap;
use ox_lang::native::{count, expected, field, given, text};
use ox_lang::portable::{Failure, Portable};
use ox_lang::value::Value;
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::Editor;
use oxmacs_core::external::managed::PDFIUM;
use oxmacs_core::view::Node;
use oxmacs_core::view::pdf::{Go, MAX_SCALE, MIN_SCALE, Mark, View, Zoom};
use oxmacs_pdf::{Document, Rect, open, texts, use_library, words};

const MODULE: &str = "ox/pdf";
const UI: &str = "ox/ui";
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "page coordinates are small numbers"
)]
fn number(heap: &Heap, name: &str, value: Value) -> Result<f32, Error> {
    match value {
        Value::Int(number) => Ok(number as f32),
        Value::Float(number) => Ok(number as f32),
        _ => Err(expected(heap, name, "a number", value)),
    }
}

fn rect(heap: &Heap, value: Value) -> Result<Rect, Error> {
    match heap.elements(value).as_deref() {
        Some([left, top, right, bottom]) => Ok(Rect {
            left: number(heap, "ui/pdf", *left)?,
            top: number(heap, "ui/pdf", *top)?,
            right: number(heap, "ui/pdf", *right)?,
            bottom: number(heap, "ui/pdf", *bottom)?,
        }),
        _ => Err(expected(
            heap,
            "ui/pdf",
            "a mark's :rect as [left top right bottom]",
            value,
        )),
    }
}

fn marks(heap: &Heap, value: Value) -> Result<Vec<Mark>, Error> {
    heap.elements(value)
        .ok_or_else(|| expected(heap, "ui/pdf", "a vector of :marks", value))?
        .into_iter()
        .map(|item| {
            Ok(Mark {
                page: count(
                    heap,
                    "ui/pdf",
                    field(heap, item, "page").unwrap_or_default(),
                )?,
                rect: rect(heap, field(heap, item, "rect").unwrap_or_default())?,
                current: field(heap, item, "current").unwrap_or_default().truthy(),
            })
        })
        .collect()
}

fn zoom(heap: &Heap, value: Value) -> Result<Zoom, Error> {
    match value {
        Value::Keyword(name) if name.name() == "fit-width" => Ok(Zoom::FitWidth),
        Value::Keyword(name) if name.name() == "fit-page" => Ok(Zoom::FitPage),
        Value::Int(_) | Value::Float(_) => Ok(Zoom::Scale(
            number(heap, "ui/pdf", value)?.clamp(MIN_SCALE, MAX_SCALE),
        )),
        _ => Err(expected(
            heap,
            "ui/pdf",
            ":zoom as :fit-width, :fit-page or a scale",
            value,
        )),
    }
}

fn go(heap: &Heap, value: Value) -> Result<Option<(i64, Go)>, Error> {
    if value == Value::Nil {
        return Ok(None);
    }
    let Value::Int(id) = field(heap, value, "id").unwrap_or_default() else {
        return Err(expected(heap, "ui/pdf", ":go with a whole :id", value));
    };
    let page = field(heap, value, "page").unwrap_or_default();
    let lines = field(heap, value, "lines").unwrap_or_default();
    let screens = field(heap, value, "screens").unwrap_or_default();
    let to = if page != Value::Nil {
        let y = field(heap, value, "y").unwrap_or_default();
        Go::Page {
            page: count(heap, "ui/pdf", page)?,
            y: (y != Value::Nil)
                .then(|| number(heap, "ui/pdf", y))
                .transpose()?,
        }
    } else if lines != Value::Nil {
        Go::Lines(number(heap, "ui/pdf", lines)?)
    } else if screens != Value::Nil {
        Go::Screens(number(heap, "ui/pdf", screens)?)
    } else {
        return Err(expected(
            heap,
            "ui/pdf",
            ":go with a :page, :lines or :screens",
            value,
        ));
    };
    Ok(Some((id, to)))
}

const PDF: [&str; 9] = [
    "revision",
    "zoom",
    "continuous",
    "dark",
    "marks",
    "go",
    "on-view",
    "on-select",
    "on-link",
];

fn view(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let path = PathBuf::from(text(heap, "ui/pdf", arguments[0])?);
    let mut shown = View {
        path,
        revision: 0,
        zoom: Zoom::FitWidth,
        continuous: true,
        dark: false,
        marks: Vec::new(),
        go: None,
        on_view: None,
        on_select: None,
        on_link: None,
    };
    for (option, value) in given(&PDF, &arguments[1..]) {
        match option {
            "revision" => {
                shown.revision = u64::try_from(count(heap, "ui/pdf", value)?).unwrap_or_default();
            }
            "zoom" => shown.zoom = zoom(heap, value)?,
            "continuous" => shown.continuous = value.truthy(),
            "dark" => shown.dark = value.truthy(),
            "marks" => shown.marks = marks(heap, value)?,
            "go" => shown.go = go(heap, value)?,
            "on-view" => shown.on_view = Some(heap.hold(value)),
            "on-select" => shown.on_select = Some(heap.hold(value)),
            _ => shown.on_link = Some(heap.hold(value)),
        }
    }
    heap.foreign("pdf", Node::Pdf(Box::new(shown)))
        .map_err(Error::from)
}

fn portable(number: usize) -> Portable {
    i64::try_from(number).map_or(Portable::Nil, Portable::Int)
}

fn words_of(text: Option<&String>) -> Portable {
    text.map_or(Portable::Nil, |text| Portable::String(text.as_str().into()))
}

fn opened(revision: u64, document: &Document) -> Portable {
    let outline = document
        .outline
        .iter()
        .map(|heading| {
            Portable::Map(vec![
                (
                    Portable::keyword("title"),
                    Portable::String(heading.title.as_str().into()),
                ),
                (Portable::keyword("page"), portable(heading.page)),
                (Portable::keyword("depth"), portable(heading.depth)),
            ])
        })
        .collect();
    Portable::Map(vec![
        (Portable::keyword("pages"), portable(document.pages.len())),
        (
            Portable::keyword("revision"),
            i64::try_from(revision).map_or(Portable::Nil, Portable::Int),
        ),
        (
            Portable::keyword("title"),
            words_of(document.title.as_ref()),
        ),
        (
            Portable::keyword("author"),
            words_of(document.author.as_ref()),
        ),
        (Portable::keyword("outline"), Portable::Vector(outline)),
    ])
}

fn found(matches: Vec<words::Found>) -> Portable {
    Portable::Vector(
        matches
            .into_iter()
            .map(|found| {
                let rects = found
                    .rects
                    .iter()
                    .map(|rect| {
                        Portable::Vector(
                            [rect.left, rect.top, rect.right, rect.bottom]
                                .into_iter()
                                .map(|side| Portable::Float(f64::from(side)))
                                .collect(),
                        )
                    })
                    .collect();
                Portable::Map(vec![
                    (Portable::keyword("page"), portable(found.page)),
                    (Portable::keyword("rects"), Portable::Vector(rects)),
                ])
            })
            .collect(),
    )
}

pub fn install(isolate: &mut Isolate) {
    isolate
        .native(MODULE, "library")
        .arity(0, Some(1))
        .host::<Editor>(|heap, editor, arguments| {
            editor.add_managed_tool(PDFIUM);
            let configured = arguments
                .first()
                .and_then(|value| heap.text(*value))
                .map(PathBuf::from)
                .filter(|path| path.is_file());
            let library =
                configured.or_else(|| editor.installed_tool(PDFIUM).map(|(_, path)| path));
            use_library(library.clone());
            match library {
                Some(path) => heap
                    .string(&path.display().to_string())
                    .map_err(Error::from),
                None => Ok(Value::Nil),
            }
        });
    isolate
        .native(MODULE, "open")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let path = PathBuf::from(text(heap, "pdf/open", arguments[0])?);
            heap.background("ox pdf/open", move || {
                open(&path)
                    .map(|(revision, document)| opened(revision, &document))
                    .map_err(Failure::from)
            })
        });
    isolate
        .native(MODULE, "search")
        .arity(2, Some(2))
        .outside()
        .function(|heap, arguments| {
            let path = PathBuf::from(text(heap, "pdf/search", arguments[0])?);
            let query = text(heap, "pdf/search", arguments[1])?.to_owned();
            heap.background("ox pdf/search", move || {
                texts(&path)
                    .map(|pages| found(words::search(&pages, &query)))
                    .map_err(Failure::from)
            })
        });
    isolate
        .native(UI, "pdf")
        .arity(1, Some(1))
        .keywords(&PDF)
        .function(view);
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::external::managed::PDFIUM;

    use crate::Ox;
    use crate::testing::settle;
    use oxmacs_core::view::pdf::{Go, Mark, Zoom};
    use oxmacs_core::view::{Node, ViewBuffer};
    use oxmacs_pdf::{Document, Heading, PageSize, PageText, Rect};

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

    fn loaded() -> (tempfile::TempDir, String, u64) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("paper.pdf");
        std::fs::write(&path, "%PDF").unwrap();
        let revision = oxmacs_pdf::preload(
            &path,
            Document {
                pages: vec![LETTER; 3],
                outline: vec![Heading {
                    title: "Methods".to_owned(),
                    page: 1,
                    depth: 0,
                }],
                title: Some("A Paper".to_owned()),
                author: None,
            },
            vec![line("Hello world"), line("a needle"), line("hello again")],
        );
        let shown = path.display().to_string().replace('\\', "/");
        (dir, shown, revision)
    }

    fn evaluate(editor: &mut Editor, ox: &mut Ox, source: &str) -> String {
        ox.eval(
            editor,
            &format!("(module (import (ox pdf) (ox ui))) (def found (atom nil)) (spawn (reset! found {source}))"),
        )
        .unwrap();
        settle(editor, ox);
        ox.eval(editor, "@found").unwrap()
    }

    #[test]
    fn opening_reads_pages_outline_title_and_revision() {
        let (_dir, path, revision) = loaded();
        let (mut editor, mut ox) = (Editor::new(), Ox::new());

        let opened = evaluate(
            &mut editor,
            &mut ox,
            &format!("(await (pdf/open {path:?}))"),
        );

        assert!(opened.contains("pages: 3"), "{opened}");
        assert!(opened.contains("title: \"A Paper\""), "{opened}");
        assert!(
            opened.contains(&format!("revision: {revision}")),
            "{opened}"
        );
        assert!(
            opened.contains("{title: \"Methods\" page: 1 depth: 0}"),
            "{opened}"
        );
    }

    #[test]
    fn searching_finds_the_rectangles_of_each_match() {
        let (_dir, path, _) = loaded();
        let (mut editor, mut ox) = (Editor::new(), Ox::new());

        let found = evaluate(
            &mut editor,
            &mut ox,
            &format!("(map :page (await (pdf/search {path:?} \"hello\")))"),
        );
        let rects = evaluate(
            &mut editor,
            &mut ox,
            &format!("(get (first (await (pdf/search {path:?} \"needle\"))) :rects)"),
        );

        assert_eq!(found, "[0 2]");
        assert_eq!(rects, "[[84.0 100.0 119.0 110.0]]");
    }

    #[test]
    fn documents_that_cannot_be_read_say_how_to_get_pdfium() {
        let (mut editor, mut ox) = (Editor::new(), Ox::new());

        let failed = evaluate(
            &mut editor,
            &mut ox,
            "(try (await (pdf/open \"nowhere/at/all.pdf\")) (catch error e e.message))",
        );

        assert!(
            failed.contains("PDFium") || failed.contains("all.pdf"),
            "{failed}"
        );
    }

    #[test]
    fn the_library_is_offered_in_list_tools_and_found_where_it_was_installed() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("pdfium").join("8111");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(PDFIUM.binary()), "library").unwrap();
        let (mut editor, mut ox) = (Editor::new(), Ox::new());
        editor.set_tools_root(Some(root.path().to_owned()));

        let library = evaluate(&mut editor, &mut ox, "(pdf/library)");

        assert!(library.contains("8111"), "{library}");
        assert!(editor.installed_tool(PDFIUM).is_some());
    }

    #[test]
    fn views_carry_zoom_marks_moves_and_callbacks() {
        let (mut editor, mut ox) = (Editor::new(), Ox::new());
        ox.eval(
            &mut editor,
            r#"(module (import (ox ui)))
(open-view "*pdf*" (fn [] (ui/pdf "paper.pdf" :revision 4 :zoom 1.5 :continuous false :dark true
                                  :marks [{page: 1 rect: [1 2 3 4] current: true}]
                                  :go {id: 7 page: 2 y: 30}
                                  :on-view (fn [page scale] nil))))"#,
        )
        .unwrap();
        settle(&mut editor, &mut ox);

        let id = editor.buffer_named("*pdf*").unwrap();
        let Node::Pdf(view) = &editor.special_buffer::<ViewBuffer>(id).unwrap().1.node else {
            panic!("not a pdf view");
        };
        assert_eq!(view.path.to_str(), Some("paper.pdf"));
        assert_eq!(view.revision, 4);
        assert_eq!(view.zoom, Zoom::Scale(1.5));
        assert!(!view.continuous && view.dark);
        assert_eq!(
            view.marks,
            [Mark {
                page: 1,
                rect: Rect {
                    left: 1.0,
                    top: 2.0,
                    right: 3.0,
                    bottom: 4.0
                },
                current: true
            }]
        );
        assert_eq!(
            view.go,
            Some((
                7,
                Go::Page {
                    page: 2,
                    y: Some(30.0)
                }
            ))
        );
        assert_eq!(view.on_view, Some(0));
        assert_eq!(view.on_select, None);
    }

    #[test]
    fn pages_take_on_the_themes_colors() {
        let ink = oxmacs_pdf::Ink {
            paper: [30, 30, 30],
            text: [220, 220, 210],
        };
        let mut pixels = vec![255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 255];

        oxmacs_pdf::recolor(&mut pixels, ink);

        assert_eq!(pixels[0..4], [30, 30, 30, 255]);
        assert_eq!(pixels[4..8], [220, 220, 210, 255]);
        assert!((100..150).contains(&pixels[8]), "{pixels:?}");
        assert_eq!(pixels[11], 255);
    }

    #[test]
    #[ignore = "needs PDFium; set OXMACS_PDFIUM to the library file"]
    fn pdfium_reads_outlines_text_and_draws_pages() {
        let library = std::path::PathBuf::from(std::env::var_os("OXMACS_PDFIUM").unwrap());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.pdf");
        std::fs::write(&path, crate::testing::sample_pdf()).unwrap();
        oxmacs_pdf::use_library(Some(library));

        let (revision, document) = oxmacs_pdf::open(&path).unwrap();
        assert_eq!(document.pages, [LETTER; 3]);
        let outline: Vec<(&str, usize, usize)> = document
            .outline
            .iter()
            .map(|heading| (heading.title.as_str(), heading.page, heading.depth))
            .collect();
        assert_eq!(
            outline,
            [
                ("Chapter 1", 0, 0),
                ("Section 1.1", 1, 1),
                ("Chapter 2", 2, 0)
            ]
        );
        assert_eq!(oxmacs_pdf::open(&path).unwrap().0, revision);

        let pages = oxmacs_pdf::texts(&path).unwrap();
        assert_eq!(pages.len(), 3);
        assert!(pages[1].text.starts_with(&['P', 'a', 'g', 'e', ' ', '2']));
        assert_eq!(pages[1].text.len(), pages[1].boxes.len());
        let found = oxmacs_pdf::words::search(&pages, "lazy dog");
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].rects.len(), 1, "{:?}", &pages[0].boxes[..12]);

        let (sender, receiver) = std::sync::mpsc::channel();
        assert!(oxmacs_pdf::render(&path, 0, 306, None, move |drawn| {
            let _ = sender.send(drawn);
        }));
        let bitmap = receiver.recv().unwrap().unwrap();
        assert_eq!((bitmap.width, bitmap.height), (306, 396));
        assert!(
            bitmap
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[0] < 100)
        );
    }
}
