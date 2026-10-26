mod engine;
pub mod words;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use engine::Engine;

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

pub fn use_library(library: Option<PathBuf>) {
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
