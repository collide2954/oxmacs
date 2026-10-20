use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError, mpsc};
use std::{fmt, fs, thread};

use iced::futures::channel::oneshot;
use pdfium_render::prelude::{
    PdfBookmark, PdfDocument, PdfLink, PdfPage, PdfPageIndex, PdfRect, PdfRenderConfig, Pdfium,
    Pixels,
};

use crate::layout::Size;
use crate::model::{Document, Heading, Link, PageText, Rect, Target};
use crate::view::DocId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Ink {
    pub(crate) paper: [u8; 3],
    pub(crate) text: [u8; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bitmap {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

type Job = Box<dyn for<'a> FnOnce(&mut Worker<'a>) + Send>;

struct Worker<'a> {
    pdfium: &'a Pdfium,
    documents: HashMap<DocId, PdfDocument<'a>>,
}

#[derive(Clone)]
pub(crate) struct Engine {
    jobs: mpsc::Sender<Job>,
}

impl fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Engine")
    }
}

static SHARED: Mutex<Option<Engine>> = Mutex::new(None);

pub(crate) fn shared(library: &Path) -> Result<Engine, String> {
    let mut shared = SHARED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(engine) = shared.as_ref() {
        return Ok(engine.clone());
    }
    let engine = Engine::start(library)?;
    *shared = Some(engine.clone());
    Ok(engine)
}

pub(crate) fn running() -> Option<Engine> {
    SHARED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

impl Engine {
    fn start(library: &Path) -> Result<Self, String> {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (ready, started) = mpsc::channel();
        let library = library.to_owned();
        thread::Builder::new()
            .name("pdfium".to_owned())
            .spawn(move || serve(&library, &ready, &inbox))
            .map_err(|error| error.to_string())?;
        started
            .recv()
            .map_err(|_| "PDFium stopped while starting".to_owned())??;
        Ok(Self { jobs })
    }

    fn run<T: Send + 'static>(
        &self,
        work: impl for<'a> FnOnce(&mut Worker<'a>) -> T + Send + 'static,
    ) -> oneshot::Receiver<T> {
        let (sender, receiver) = oneshot::channel();
        let _ = self.jobs.send(Box::new(move |worker| {
            let _ = sender.send(work(worker));
        }));
        receiver
    }

    pub(crate) fn open(
        &self,
        doc: DocId,
        path: PathBuf,
    ) -> oneshot::Receiver<Result<Document, String>> {
        self.run(move |worker| worker.open(doc, &path))
    }

    pub(crate) fn index(&self, doc: DocId) -> oneshot::Receiver<Result<Vec<PageText>, String>> {
        self.run(move |worker| worker.index(doc))
    }

    pub(crate) fn render(
        &self,
        doc: DocId,
        page: usize,
        width: usize,
        ink: Option<Ink>,
    ) -> oneshot::Receiver<Result<Bitmap, String>> {
        self.run(move |worker| {
            let mut bitmap = worker.render(doc, page, width)?;
            if let Some(ink) = ink {
                recolor(&mut bitmap.pixels, ink);
            }
            Ok(bitmap)
        })
    }
}

fn serve(library: &Path, ready: &mpsc::Sender<Result<(), String>>, inbox: &mpsc::Receiver<Job>) {
    let bindings = match Pdfium::bind_to_library(library) {
        Ok(bindings) => bindings,
        Err(error) => {
            let _ = ready.send(Err(format!("PDFium could not be loaded: {error}")));
            return;
        }
    };
    let pdfium = Pdfium::new(bindings);
    let _ = ready.send(Ok(()));
    let mut worker = Worker {
        pdfium: &pdfium,
        documents: HashMap::new(),
    };
    for job in inbox {
        job(&mut worker);
    }
}

impl Worker<'_> {
    fn open(&mut self, doc: DocId, path: &Path) -> Result<Document, String> {
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        let document = self
            .pdfium
            .load_pdf_from_byte_vec(bytes, None)
            .map_err(|error| error.to_string())?;
        let pages = document
            .pages()
            .page_sizes()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|rect| Size {
                width: rect.width().value,
                height: rect.height().value,
            })
            .collect();
        let outline = outline(&document);
        self.documents.insert(doc, document);
        Ok(Document { pages, outline })
    }

    fn index(&self, doc: DocId) -> Result<Vec<PageText>, String> {
        let document = self
            .documents
            .get(&doc)
            .ok_or_else(|| "the document is closed".to_owned())?;
        document
            .pages()
            .iter()
            .map(|page| page_text(&page))
            .collect()
    }

    fn render(&self, doc: DocId, page: usize, width: usize) -> Result<Bitmap, String> {
        let document = self
            .documents
            .get(&doc)
            .ok_or_else(|| "the document is closed".to_owned())?;
        let index = PdfPageIndex::try_from(page).map_err(|error| error.to_string())?;
        let width = Pixels::try_from(width).map_err(|error| error.to_string())?;
        let page = document
            .pages()
            .get(index)
            .map_err(|error| error.to_string())?;
        let bitmap = page
            .render_with_config(
                &PdfRenderConfig::new()
                    .set_target_width(width)
                    .render_form_data(true)
                    .render_annotations(true),
            )
            .map_err(|error| error.to_string())?;
        Ok(Bitmap {
            width: u32::try_from(bitmap.width()).unwrap_or_default(),
            height: u32::try_from(bitmap.height()).unwrap_or_default(),
            pixels: bitmap.as_rgba_bytes(),
        })
    }
}

fn page_text(page: &PdfPage<'_>) -> Result<PageText, String> {
    let height = page.height().value;
    let flip = |rect: PdfRect| Rect {
        left: rect.left().value,
        top: height - rect.top().value,
        right: rect.right().value,
        bottom: height - rect.bottom().value,
    };
    let text = page.text().map_err(|error| error.to_string())?;
    let mut string = String::new();
    let mut boxes = Vec::new();
    for character in text.chars().iter() {
        string.push(match character.unicode_char() {
            Some('\r' | '\u{c}') => ' ',
            Some(character) => character,
            None => '\u{fffd}',
        });
        boxes.push(character.loose_bounds().map(flip).unwrap_or_default());
    }
    let links = page
        .links()
        .iter()
        .filter_map(|link| {
            Some(Link {
                rect: flip(link.rect().ok()?),
                target: target(&link)?,
            })
        })
        .collect();
    Ok(PageText {
        text: string,
        boxes,
        links,
    })
}

fn target(link: &PdfLink<'_>) -> Option<Target> {
    if let Some(destination) = link.destination() {
        return Some(Target::Page(
            usize::try_from(destination.page_index().ok()?).ok()?,
        ));
    }
    let action = link.action()?;
    if let Some(uri) = action.as_uri_action() {
        return uri.uri().ok().map(Target::Uri);
    }
    let local = action.as_local_destination_action()?;
    let page = local.destination().ok()?.page_index().ok()?;
    Some(Target::Page(usize::try_from(page).ok()?))
}

const OUTLINE_LIMIT: usize = 10_000;

fn outline(document: &PdfDocument<'_>) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut pending: Vec<(PdfBookmark<'_>, usize)> = document
        .bookmarks()
        .root()
        .map(|root| (root, 0))
        .into_iter()
        .collect();
    let mut seen = 0;
    while let Some((bookmark, depth)) = pending.pop() {
        seen += 1;
        if seen > OUTLINE_LIMIT {
            break;
        }
        if let Some(next) = bookmark.next_sibling() {
            pending.push((next, depth));
        }
        if let Some(child) = bookmark.first_child() {
            pending.push((child, depth + 1));
        }
        if let Some(heading) = heading(&bookmark, depth) {
            headings.push(heading);
        }
    }
    headings
}

fn heading(bookmark: &PdfBookmark<'_>, depth: usize) -> Option<Heading> {
    let title = bookmark.title()?;
    let page = if let Some(destination) = bookmark.destination() {
        destination.page_index().ok()?
    } else {
        let action = bookmark.action()?;
        let local = action.as_local_destination_action()?;
        local.destination().ok()?.page_index().ok()?
    };
    Some(Heading {
        title,
        page: usize::try_from(page).ok()?,
        depth,
    })
}

pub(crate) fn recolor(pixels: &mut [u8], ink: Ink) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let light =
            (u32::from(pixel[0]) * 54 + u32::from(pixel[1]) * 183 + u32::from(pixel[2]) * 19) / 256;
        for (value, (text, paper)) in pixel.iter_mut().zip(ink.text.iter().zip(ink.paper)) {
            let mixed = (u32::from(*text) * (255 - light) + u32::from(paper) * light) / 255;
            *value = u8::try_from(mixed).unwrap_or(u8::MAX);
        }
    }
}

#[cfg(test)]
mod tests {
    use iced::futures::executor::block_on;

    use super::*;

    #[test]
    #[ignore = "needs PDFium; set OXMACS_PDFIUM to the library file"]
    fn pdfium_opens_documents_reads_outlines_and_renders_pages() {
        let library = PathBuf::from(std::env::var_os("OXMACS_PDFIUM").unwrap());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.pdf");
        fs::write(&path, crate::sample::pdf()).unwrap();
        let engine = shared(&library).unwrap();
        assert!(running().is_some());
        assert!(shared(Path::new("elsewhere")).is_ok());

        let document = block_on(engine.open(7, path)).unwrap().unwrap();
        let letter = Size {
            width: 612.0,
            height: 792.0,
        };
        assert_eq!(document.pages, [letter; 3]);
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

        let bitmap = block_on(engine.render(7, 0, 306, None)).unwrap().unwrap();
        assert_eq!((bitmap.width, bitmap.height), (306, 396));
        assert!(
            bitmap
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[0] < 100)
        );
        assert!(block_on(engine.render(8, 0, 306, None)).unwrap().is_err());

        let pages = block_on(engine.index(7)).unwrap().unwrap();
        assert_eq!(pages.len(), 3);
        assert!(pages[1].text.starts_with("Page 2"), "{:?}", pages[1].text);
        assert_eq!(pages[1].text.chars().count(), pages[1].boxes.len());
        let first = pages[1].boxes[0];
        assert!(first.top > 50.0 && first.top < 92.0, "{first:?}");
        assert!(first.left > 70.0 && first.left < 80.0, "{first:?}");
    }

    #[test]
    fn dark_pages_turn_paper_and_ink_into_the_themes_colors() {
        let ink = Ink {
            paper: [30, 30, 30],
            text: [220, 220, 210],
        };
        let mut pixels = vec![255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 255];

        recolor(&mut pixels, ink);

        assert_eq!(pixels[0..4], [30, 30, 30, 255]);
        assert_eq!(pixels[4..8], [220, 220, 210, 255]);
        assert!((100..150).contains(&pixels[8]), "{pixels:?}");
        assert_eq!(pixels[11], 255);
    }
}
