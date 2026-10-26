mod engine;
#[cfg(test)]
mod sample;
pub mod words;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use ox_lang::heap::Heap;
use ox_lang::portable::{Failure, Portable};
use ox_lang::symbol::Sym;
use ox_lang::value::{Value, print};
use ox_lang::{Error, Isolate};
use oxmacs_core::editor::Editor;
use oxmacs_core::external::managed::PDFIUM;

use crate::view::views::{Node, Press};
use engine::Engine;

const MODULE: &str = "ox/pdf";
const UI: &str = "ox/ui";
pub const MISSING: &str = "PDFium isn't installed; M-x list-tools installs it";

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        (self.left..=self.right).contains(&x) && (self.top..=self.bottom).contains(&y)
    }

    pub fn distance(&self, x: f32, y: f32) -> f32 {
        let across = (self.left - x).max(x - self.right).max(0.0);
        let down = (self.top - y).max(y - self.bottom).max(0.0);
        across * across + down * down
    }

    pub fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Page(usize),
    Uri(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub rect: Rect,
    pub target: Target,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageText {
    pub text: Vec<char>,
    pub boxes: Vec<Rect>,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub title: String,
    pub page: usize,
    pub depth: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub pages: Vec<PageSize>,
    pub outline: Vec<Heading>,
    pub title: Option<String>,
    pub author: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ink {
    pub paper: [u8; 3],
    pub text: [u8; 3],
}

pub fn recolor(pixels: &mut [u8], ink: Ink) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let light =
            (u32::from(pixel[0]) * 54 + u32::from(pixel[1]) * 183 + u32::from(pixel[2]) * 19) / 256;
        for (value, (text, paper)) in pixel.iter_mut().zip(ink.text.iter().zip(ink.paper)) {
            let mixed = (u32::from(*text) * (255 - light) + u32::from(paper) * light) / 255;
            *value = u8::try_from(mixed).unwrap_or(u8::MAX);
        }
    }
}

#[derive(Debug)]
struct Loaded {
    revision: u64,
    stamp: Option<SystemTime>,
    document: Arc<Document>,
    texts: Option<Arc<Vec<PageText>>>,
    indexing: bool,
}

#[derive(Debug, Default)]
struct Shared {
    library: Option<PathBuf>,
    engine: Option<Engine>,
    loaded: HashMap<PathBuf, Loaded>,
    revisions: u64,
}

static SHARED: LazyLock<Mutex<Shared>> = LazyLock::new(Mutex::default);

fn shared() -> MutexGuard<'static, Shared> {
    SHARED.lock().unwrap_or_else(PoisonError::into_inner)
}

fn stamp(path: &Path) -> Option<SystemTime> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

fn use_library(library: Option<PathBuf>) {
    let mut shared = shared();
    if shared.library != library {
        shared.library = library;
        shared.engine = None;
    }
}

fn engine() -> Result<Engine, String> {
    let library = {
        let shared = shared();
        if let Some(engine) = &shared.engine {
            return Ok(engine.clone());
        }
        shared.library.clone()
    };
    let engine = Engine::start(&library.ok_or_else(|| MISSING.to_owned())?)?;
    shared().engine = Some(engine.clone());
    Ok(engine)
}

pub fn open(path: &Path) -> Result<(u64, Arc<Document>), String> {
    let now = stamp(path);
    if let Some(loaded) = shared().loaded.get(path)
        && loaded.stamp == now
    {
        return Ok((loaded.revision, Arc::clone(&loaded.document)));
    }
    let document = Arc::new(engine()?.open(path)?);
    let mut shared = shared();
    shared.revisions += 1;
    let revision = shared.revisions;
    shared.loaded.insert(
        path.to_owned(),
        Loaded {
            revision,
            stamp: now,
            document: Arc::clone(&document),
            texts: None,
            indexing: false,
        },
    );
    Ok((revision, document))
}

pub fn preload(path: &Path, document: Document, texts: Vec<PageText>) -> u64 {
    let mut shared = shared();
    shared.revisions += 1;
    let revision = shared.revisions;
    shared.loaded.insert(
        path.to_owned(),
        Loaded {
            revision,
            stamp: stamp(path),
            document: Arc::new(document),
            texts: Some(Arc::new(texts)),
            indexing: false,
        },
    );
    revision
}

pub fn document(path: &Path) -> Option<(u64, Arc<Document>)> {
    shared()
        .loaded
        .get(path)
        .map(|loaded| (loaded.revision, Arc::clone(&loaded.document)))
}

pub fn texts(path: &Path) -> Result<Arc<Vec<PageText>>, String> {
    if let Some(texts) = shared()
        .loaded
        .get(path)
        .and_then(|loaded| loaded.texts.clone())
    {
        return Ok(texts);
    }
    let texts = Arc::new(engine()?.index(path)?);
    if let Some(loaded) = shared().loaded.get_mut(path) {
        loaded.texts = Some(Arc::clone(&texts));
    }
    Ok(texts)
}

pub fn texts_now(path: &Path) -> Option<Arc<Vec<PageText>>> {
    let engine = {
        let mut guard = shared();
        let shared = &mut *guard;
        let engine = shared.engine.clone();
        let loaded = shared.loaded.get_mut(path)?;
        if let Some(texts) = &loaded.texts {
            return Some(Arc::clone(texts));
        }
        if loaded.indexing {
            return None;
        }
        let engine = engine?;
        loaded.indexing = true;
        engine
    };
    let key = path.to_owned();
    engine.index_then(path, move |result| {
        if let Some(loaded) = shared().loaded.get_mut(&key) {
            loaded.indexing = false;
            loaded.texts = Some(Arc::new(result.unwrap_or_default()));
        }
    });
    None
}

pub fn render(
    path: &Path,
    page: usize,
    width: usize,
    ink: Option<Ink>,
    done: impl FnOnce(Result<Bitmap, String>) + Send + 'static,
) -> bool {
    let engine = shared().engine.clone();
    engine.is_some_and(|engine| engine.render_then(path, page, width, ink, done))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zoom {
    FitWidth,
    FitPage,
    Scale(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Go {
    Page { page: usize, y: Option<f32> },
    Lines(f32),
    Screens(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mark {
    pub page: usize,
    pub rect: Rect,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub path: PathBuf,
    pub revision: u64,
    pub zoom: Zoom,
    pub continuous: bool,
    pub dark: bool,
    pub marks: Vec<Mark>,
    pub go: Option<(i64, Go)>,
    pub on_view: Option<Press>,
    pub on_select: Option<Press>,
    pub on_link: Option<Press>,
}

impl View {
    pub(crate) fn presses(&mut self) -> [&mut Option<Press>; 3] {
        [&mut self.on_view, &mut self.on_select, &mut self.on_link]
    }
}

fn wrong(heap: &Heap, name: &str, wanted: &str, value: Value) -> Error {
    Error::new(format!(
        "{name} expects {wanted}, got {}",
        print::brief(heap, value)
    ))
}

fn text_of<'a>(heap: &'a Heap, name: &str, value: Value) -> Result<&'a str, Error> {
    heap.text(value)
        .ok_or_else(|| wrong(heap, name, "a string", value))
}

fn field(heap: &Heap, map: Value, name: &str) -> Value {
    heap.entries(map)
        .unwrap_or_default()
        .into_iter()
        .find(|(key, _)| matches!(key, Value::Keyword(found) if found.name() == name))
        .map_or(Value::Nil, |(_, value)| value)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "page coordinates are small numbers"
)]
fn number(heap: &Heap, name: &str, value: Value) -> Result<f32, Error> {
    match value {
        Value::Int(number) => Ok(number as f32),
        Value::Float(number) => Ok(number as f32),
        _ => Err(wrong(heap, name, "a number", value)),
    }
}

fn whole(heap: &Heap, name: &str, value: Value) -> Result<usize, Error> {
    match value {
        Value::Int(number) => {
            usize::try_from(number).map_err(|_| wrong(heap, name, "a whole number", value))
        }
        _ => Err(wrong(heap, name, "a whole number", value)),
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
        _ => Err(wrong(
            heap,
            "ui/pdf",
            "a mark's :rect as [left top right bottom]",
            value,
        )),
    }
}

fn marks(heap: &Heap, value: Value) -> Result<Vec<Mark>, Error> {
    heap.elements(value)
        .ok_or_else(|| wrong(heap, "ui/pdf", "a vector of :marks", value))?
        .into_iter()
        .map(|item| {
            Ok(Mark {
                page: whole(heap, "ui/pdf", field(heap, item, "page"))?,
                rect: rect(heap, field(heap, item, "rect"))?,
                current: field(heap, item, "current").truthy(),
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
        _ => Err(wrong(
            heap,
            "ui/pdf",
            ":zoom as :fit-width, :fit-page or a scale",
            value,
        )),
    }
}

pub const MIN_SCALE: f32 = 0.25;
pub const MAX_SCALE: f32 = 8.0;

fn go(heap: &Heap, value: Value) -> Result<Option<(i64, Go)>, Error> {
    if value == Value::Nil {
        return Ok(None);
    }
    let Value::Int(id) = field(heap, value, "id") else {
        return Err(wrong(heap, "ui/pdf", ":go with a whole :id", value));
    };
    let page = field(heap, value, "page");
    let lines = field(heap, value, "lines");
    let screens = field(heap, value, "screens");
    let to = if page != Value::Nil {
        let y = field(heap, value, "y");
        Go::Page {
            page: whole(heap, "ui/pdf", page)?,
            y: (y != Value::Nil)
                .then(|| number(heap, "ui/pdf", y))
                .transpose()?,
        }
    } else if lines != Value::Nil {
        Go::Lines(number(heap, "ui/pdf", lines)?)
    } else if screens != Value::Nil {
        Go::Screens(number(heap, "ui/pdf", screens)?)
    } else {
        return Err(wrong(
            heap,
            "ui/pdf",
            ":go with a :page, :lines or :screens",
            value,
        ));
    };
    Ok(Some((id, to)))
}

const OPTIONS: &str = "ui/pdf takes :revision, :zoom, :continuous, :dark, :marks, :go, :on-view, :on-select and :on-link";

fn view(heap: &mut Heap, arguments: &[Value]) -> Result<Value, Error> {
    let path = PathBuf::from(text_of(heap, "ui/pdf", arguments[0])?);
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
    for pair in arguments[1..].chunks(2) {
        let [Value::Keyword(option), value] = pair else {
            return Err(Error::new(OPTIONS));
        };
        let value = *value;
        match option.name() {
            "revision" => {
                shown.revision = u64::try_from(whole(heap, "ui/pdf", value)?).unwrap_or_default();
            }
            "zoom" => shown.zoom = zoom(heap, value)?,
            "continuous" => shown.continuous = value.truthy(),
            "dark" => shown.dark = value.truthy(),
            "marks" => shown.marks = marks(heap, value)?,
            "go" => shown.go = go(heap, value)?,
            "on-view" => shown.on_view = Some(Press::Held(heap.hold(value))),
            "on-select" => shown.on_select = Some(Press::Held(heap.hold(value))),
            "on-link" => shown.on_link = Some(Press::Held(heap.hold(value))),
            _ => return Err(Error::new(OPTIONS)),
        }
    }
    heap.foreign("pdf", Node::Pdf(Box::new(shown)))
        .map_err(|full| Error::new(full.to_string()))
}

fn key(name: &str) -> Portable {
    Portable::Keyword(Sym::intern(name))
}

fn count(number: usize) -> Portable {
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
                    key("title"),
                    Portable::String(heading.title.as_str().into()),
                ),
                (key("page"), count(heading.page)),
                (key("depth"), count(heading.depth)),
            ])
        })
        .collect();
    Portable::Map(vec![
        (key("pages"), count(document.pages.len())),
        (
            key("revision"),
            i64::try_from(revision).map_or(Portable::Nil, Portable::Int),
        ),
        (key("title"), words_of(document.title.as_ref())),
        (key("author"), words_of(document.author.as_ref())),
        (key("outline"), Portable::Vector(outline)),
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
                    (key("page"), count(found.page)),
                    (key("rects"), Portable::Vector(rects)),
                ])
            })
            .collect(),
    )
}

fn failure(message: String) -> Failure {
    Failure {
        value: None,
        message,
    }
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
                    .map_err(|full| Error::new(full.to_string())),
                None => Ok(Value::Nil),
            }
        });
    isolate
        .native(MODULE, "open")
        .arity(1, Some(1))
        .outside()
        .function(|heap, arguments| {
            let path = PathBuf::from(text_of(heap, "pdf/open", arguments[0])?);
            heap.background("ox pdf/open", move || {
                open(&path)
                    .map(|(revision, document)| opened(revision, &document))
                    .map_err(failure)
            })
        });
    isolate
        .native(MODULE, "search")
        .arity(2, Some(2))
        .outside()
        .function(|heap, arguments| {
            let path = PathBuf::from(text_of(heap, "pdf/search", arguments[0])?);
            let query = text_of(heap, "pdf/search", arguments[1])?.to_owned();
            heap.background("ox pdf/search", move || {
                texts(&path)
                    .map(|pages| found(words::search(&pages, &query)))
                    .map_err(failure)
            })
        });
    isolate.native(UI, "pdf").arity(1, None).function(view);
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;
    use oxmacs_core::external::managed::PDFIUM;

    use super::{Document, Go, Heading, Mark, PageSize, PageText, Rect, Zoom};
    use crate::Ox;
    use crate::testing::settle;
    use crate::view::views::{Node, Press, ViewBuffer};

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
        let revision = super::preload(
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
        let Node::Pdf(view) = &editor.plugin_buffer::<ViewBuffer>(id).unwrap().1.node else {
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
        assert_eq!(view.on_view, Some(Press::Slot(0)));
        assert_eq!(view.on_select, None);
    }

    #[test]
    fn pages_take_on_the_themes_colors() {
        let ink = super::Ink {
            paper: [30, 30, 30],
            text: [220, 220, 210],
        };
        let mut pixels = vec![255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 255];

        super::recolor(&mut pixels, ink);

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
        std::fs::write(&path, super::sample::pdf()).unwrap();
        super::use_library(Some(library));

        let (revision, document) = super::open(&path).unwrap();
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
        assert_eq!(super::open(&path).unwrap().0, revision);

        let pages = super::texts(&path).unwrap();
        assert_eq!(pages.len(), 3);
        assert!(pages[1].text.starts_with(&['P', 'a', 'g', 'e', ' ', '2']));
        assert_eq!(pages[1].text.len(), pages[1].boxes.len());
        let found = super::words::search(&pages, "lazy dog");
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].rects.len(), 1, "{:?}", &pages[0].boxes[..12]);

        let (sender, receiver) = std::sync::mpsc::channel();
        assert!(super::render(&path, 0, 306, None, move |drawn| {
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
