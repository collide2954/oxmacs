use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::{fmt, fs, thread};

use pdfium_render::prelude::{
    PdfBookmark, PdfDocument, PdfDocumentMetadataTagType, PdfLink, PdfPage, PdfPageIndex, PdfRect,
    PdfRenderConfig, Pdfium, PdfiumError, PdfiumInternalError, Pixels,
};

use super::{Bitmap, Document, Heading, Ink, Link, PageSize, PageText, Rect, Target, recolor};

const STOPPED: &str = "PDFium stopped";
const OUTLINE_LIMIT: usize = 10_000;

type Job = Box<dyn for<'a> FnOnce(&mut Worker<'a>) + Send>;

struct Worker<'a> {
    pdfium: &'a Pdfium,
    documents: HashMap<PathBuf, PdfDocument<'a>>,
}

#[derive(Clone)]
pub struct Engine {
    jobs: mpsc::Sender<Job>,
}

impl fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Engine")
    }
}

impl Engine {
    pub fn start(library: &Path) -> Result<Self, String> {
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

    fn send(&self, job: impl for<'a> FnOnce(&mut Worker<'a>) + Send + 'static) -> bool {
        self.jobs.send(Box::new(job)).is_ok()
    }

    fn wait<T: Send + 'static>(
        &self,
        work: impl for<'a> FnOnce(&mut Worker<'a>) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let (sender, receiver) = mpsc::channel();
        if !self.send(move |worker| {
            let _ = sender.send(work(worker));
        }) {
            return Err(STOPPED.to_owned());
        }
        receiver.recv().unwrap_or_else(|_| Err(STOPPED.to_owned()))
    }

    pub fn open(&self, path: &Path) -> Result<Document, String> {
        let path = path.to_owned();
        self.wait(move |worker| worker.open(&path))
    }

    pub fn index(&self, path: &Path) -> Result<Vec<PageText>, String> {
        let path = path.to_owned();
        self.wait(move |worker| worker.index(&path))
    }

    pub fn index_then(
        &self,
        path: &Path,
        done: impl FnOnce(Result<Vec<PageText>, String>) + Send + 'static,
    ) -> bool {
        let path = path.to_owned();
        self.send(move |worker| done(worker.index(&path)))
    }

    pub fn render_then(
        &self,
        path: &Path,
        page: usize,
        width: usize,
        ink: Option<Ink>,
        done: impl FnOnce(Result<Bitmap, String>) + Send + 'static,
    ) -> bool {
        let path = path.to_owned();
        self.send(move |worker| {
            done(worker.render(&path, page, width).map(|mut bitmap| {
                if let Some(ink) = ink {
                    recolor(&mut bitmap.pixels, ink);
                }
                bitmap
            }));
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

impl<'a> Worker<'a> {
    fn open(&mut self, path: &Path) -> Result<Document, String> {
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        let document = self
            .pdfium
            .load_pdf_from_byte_vec(bytes, None)
            .map_err(|error| unreadable(&error))?;
        let pages = document
            .pages()
            .page_sizes()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|rect| PageSize {
                width: rect.width().value,
                height: rect.height().value,
            })
            .collect();
        let tag = |kind| {
            document
                .metadata()
                .get(kind)
                .map(|tag| tag.value().trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        let read = Document {
            pages,
            outline: outline(&document),
            title: tag(PdfDocumentMetadataTagType::Title),
            author: tag(PdfDocumentMetadataTagType::Author),
        };
        self.documents.insert(path.to_owned(), document);
        Ok(read)
    }

    fn document(&mut self, path: &Path) -> Result<&PdfDocument<'a>, String> {
        if !self.documents.contains_key(path) {
            self.open(path)?;
        }
        self.documents
            .get(path)
            .ok_or_else(|| "the document is closed".to_owned())
    }

    fn index(&mut self, path: &Path) -> Result<Vec<PageText>, String> {
        let document = self.document(path)?;
        document
            .pages()
            .iter()
            .map(|page| page_text(&page))
            .collect()
    }

    fn render(&mut self, path: &Path, page: usize, width: usize) -> Result<Bitmap, String> {
        let document = self.document(path)?;
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

fn unreadable(error: &PdfiumError) -> String {
    match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            "PDFium can't read this file; it may not be a PDF, or it may be damaged".to_owned()
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            "This PDF is locked with a password".to_owned()
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
            "This PDF's security settings keep PDFium from opening it".to_owned()
        }
        other => format!("PDFium couldn't open this file: {other}"),
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
    let mut chars = Vec::new();
    let mut boxes = Vec::new();
    for character in text.chars().iter() {
        chars.push(match character.unicode_char() {
            Some('\u{c}') => ' ',
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
        text: chars,
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
