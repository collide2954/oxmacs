use std::path::{Path, PathBuf};

use oxmacs_core::applet::AppletBuffer;
use oxmacs_core::command::ScopeId;
use oxmacs_core::editor::{Editor, WindowId};
use oxmacs_core::text::BufferId;
use oxmacs_gui::units;

use crate::PDFIUM;
use crate::layout::{self, GAP, Placed, Size, Zoom};
use crate::model::Document;
use crate::text::{IndexDocument, Pointing, TextIndex};

pub(crate) const ZOOM_STEP: f32 = 1.25;
pub(crate) const RENDER_SCALE: f32 = 2.0;
const LINE: f32 = 40.0;
const SCREEN_SHARE: f32 = 0.9;
const MISSING: &str = "PDFium is not installed; M-x list-tools installs it";

pub type DocId = u64;

#[derive(Debug)]
pub(crate) struct PdfState {
    scope: ScopeId,
    next: DocId,
}

impl PdfState {
    pub(crate) fn new(scope: ScopeId) -> Self {
        Self { scope, next: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    Continuous,
    Single,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Loading {
    Waiting,
    Ready(Document),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub window: WindowId,
    pub offset: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenDocument {
    pub doc: DocId,
    pub path: PathBuf,
    pub library: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Opened {
    pub doc: DocId,
    pub result: Result<Document, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Wanted {
    pub page: usize,
    pub width: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderPages {
    pub doc: DocId,
    pub pages: Vec<Wanted>,
    pub dark: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Move {
    By(f32),
    To(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollPdf {
    pub window: WindowId,
    pub to: Move,
}

#[derive(Debug)]
pub(crate) struct PdfView {
    scope: ScopeId,
    pub(crate) doc: DocId,
    pub(crate) path: PathBuf,
    pub(crate) loading: Loading,
    pub(crate) zoom: Zoom,
    pub(crate) layout: Layout,
    pub(crate) dark: bool,
    pub(crate) page: usize,
    viewports: Vec<Viewport>,
    pub(crate) index: TextIndex,
    pub(crate) pointing: Pointing,
    pub(crate) revealed: Option<usize>,
}

impl PdfView {
    pub(crate) fn document(&self) -> Option<&Document> {
        match &self.loading {
            Loading::Ready(document) => Some(document),
            Loading::Waiting | Loading::Failed(_) => None,
        }
    }

    pub(crate) fn shown(&self) -> Vec<(usize, Size)> {
        let Some(document) = self.document() else {
            return Vec::new();
        };
        match self.layout {
            Layout::Continuous => document.pages.iter().copied().enumerate().collect(),
            Layout::Single => document
                .pages
                .get(self.page)
                .map(|size| vec![(self.page, *size)])
                .unwrap_or_default(),
        }
    }

    pub(crate) fn placed(&self, area: Size) -> (Vec<Placed>, f32) {
        let sizes: Vec<Size> = self.shown().into_iter().map(|(_, size)| size).collect();
        layout::place(&sizes, self.zoom, area)
    }

    pub(crate) fn viewport(&self, window: WindowId) -> Option<Viewport> {
        self.viewports
            .iter()
            .find(|viewport| viewport.window == window)
            .copied()
    }

    pub(crate) fn pages(&self) -> usize {
        self.document().map_or(0, |document| document.pages.len())
    }

    fn top_of(&self, page: usize, area: Size) -> f32 {
        match self.layout {
            Layout::Continuous => self
                .placed(area)
                .0
                .get(page)
                .map_or(0.0, |placed| placed.top - GAP),
            Layout::Single => 0.0,
        }
    }
}

impl AppletBuffer for PdfView {
    fn scope(&self) -> ScopeId {
        self.scope
    }
}

pub(crate) fn area(viewport: Viewport) -> Size {
    Size {
        width: viewport.width,
        height: viewport.height,
    }
}

pub(crate) fn fallback(window: WindowId) -> Viewport {
    Viewport {
        window,
        offset: 0.0,
        width: 800.0,
        height: 600.0,
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

pub(crate) fn open(editor: &mut Editor, path: &Path) -> bool {
    let path = editor.absolute(path);
    if let Some(id) = editor.find_applet_buffer::<PdfView>(|view| view.path == path) {
        editor.show_buffer(id);
        return true;
    }
    let Some(scope) = editor.applet_state::<PdfState>().map(|state| state.scope) else {
        return false;
    };
    let name = file_name(&path);
    let id = editor.add_applet_buffer_with(&name, |buffer| {
        buffer.set_read_only(true);
        PdfView {
            scope,
            doc: 0,
            path,
            loading: Loading::Waiting,
            zoom: Zoom::FitWidth,
            layout: Layout::Continuous,
            dark: false,
            page: 0,
            viewports: Vec::new(),
            index: TextIndex::default(),
            pointing: Pointing::default(),
            revealed: None,
        }
    });
    editor.show_buffer(id);
    load(editor, id);
    true
}

fn load(editor: &mut Editor, id: BufferId) {
    let library = editor.installed_tool(PDFIUM).map(|(_, path)| path);
    let Some(doc) = editor.applet_state_mut::<PdfState>().map(|state| {
        let doc = state.next;
        state.next += 1;
        doc
    }) else {
        return;
    };
    let Some((_, view)) = editor.applet_buffer_mut::<PdfView>(id) else {
        return;
    };
    view.doc = doc;
    if let Some(library) = library {
        view.loading = Loading::Waiting;
        let path = view.path.clone();
        editor.request(OpenDocument { doc, path, library });
    } else {
        view.loading = Loading::Failed(MISSING.to_owned());
        editor.show_message(MISSING);
    }
}

pub(crate) fn find(editor: &Editor, doc: DocId) -> Option<BufferId> {
    editor.find_applet_buffer::<PdfView>(|view| view.doc == doc)
}

pub(crate) fn opened(editor: &mut Editor, opened: &Opened) {
    let Some(id) = find(editor, opened.doc) else {
        return;
    };
    if let Err(error) = &opened.result {
        let name = editor
            .buffer_by_id(id)
            .map(|buffer| buffer.name().to_owned())
            .unwrap_or_default();
        editor.show_message(format!("{name}: {error}"));
    }
    if let Some((_, view)) = editor.applet_buffer_mut::<PdfView>(id) {
        view.loading = match &opened.result {
            Ok(document) => Loading::Ready(document.clone()),
            Err(error) => Loading::Failed(error.clone()),
        };
        view.page = view.page.min(view.pages().saturating_sub(1));
    }
    if opened.result.is_ok() {
        editor.request(IndexDocument { doc: opened.doc });
    }
    request_pages(editor, id);
}

pub(crate) fn showing(editor: &Editor, window: WindowId) -> Option<BufferId> {
    let pane = editor.pane_of(window)?;
    pane.applet_buffer::<PdfView>()?;
    Some(pane.buffer().id())
}

pub(crate) fn viewed(editor: &mut Editor, seen: &Viewport) {
    let Some(id) = showing(editor, seen.window) else {
        return;
    };
    if let Some((_, view)) = editor.applet_buffer_mut::<PdfView>(id) {
        view.viewports.retain(|known| known.window != seen.window);
        view.viewports.push(*seen);
        if view.layout == Layout::Continuous {
            let (placed, _) = view.placed(area(*seen));
            view.page = layout::page_at(&placed, seen.offset, seen.height);
        }
    }
    crate::text::sync_point(editor, id, *seen);
    request_pages(editor, id);
}

pub(crate) fn request_pages(editor: &mut Editor, id: BufferId) {
    let windows: Vec<WindowId> = editor
        .tiling()
        .windows()
        .into_iter()
        .filter(|window| showing(editor, *window) == Some(id))
        .collect();
    let Some((_, view)) = editor.applet_buffer::<PdfView>(id) else {
        return;
    };
    let shown = view.shown();
    let mut pages: Vec<Wanted> = Vec::new();
    for viewport in windows.iter().filter_map(|window| view.viewport(*window)) {
        let (placed, _) = view.placed(area(viewport));
        let near = layout::visible(
            &placed,
            viewport.offset - viewport.height,
            viewport.height * 3.0,
        );
        for index in near {
            let wanted = Wanted {
                page: shown[index].0,
                width: units::floor_to_usize(placed[index].size.width * RENDER_SCALE),
            };
            if !pages.contains(&wanted) {
                pages.push(wanted);
            }
        }
    }
    if pages.is_empty() {
        return;
    }
    let request = RenderPages {
        doc: view.doc,
        pages,
        dark: view.dark,
    };
    editor.request(request);
}

pub(crate) fn page_counter(editor: &Editor) -> Option<String> {
    let view = editor.pane().applet_buffer::<PdfView>()?;
    match &view.loading {
        Loading::Waiting => Some("loading".to_owned()),
        Loading::Ready(document) => Some(format!("{}/{}", view.page + 1, document.pages.len())),
        Loading::Failed(_) => None,
    }
}

pub(crate) fn current(editor: &Editor) -> Option<(BufferId, Viewport)> {
    let window = editor.selected_window();
    let id = showing(editor, window)?;
    let (_, view) = editor.applet_buffer::<PdfView>(id)?;
    Some((
        id,
        view.viewport(window).unwrap_or_else(|| fallback(window)),
    ))
}

pub(crate) fn scroll(editor: &mut Editor, window: WindowId, to: Move) {
    editor.request(ScrollPdf { window, to });
}

pub(crate) fn show_page(editor: &mut Editor, page: usize) {
    let Some((id, viewport)) = current(editor) else {
        return;
    };
    let Some((_, view)) = editor.applet_buffer_mut::<PdfView>(id) else {
        return;
    };
    let page = page.min(view.pages().saturating_sub(1));
    view.page = page;
    let top = view.top_of(page, area(viewport));
    scroll(editor, viewport.window, Move::To(top));
    crate::text::sync_point(editor, id, viewport);
    request_pages(editor, id);
}

fn current_page(editor: &Editor) -> Option<usize> {
    editor
        .pane()
        .applet_buffer::<PdfView>()
        .map(|view| view.page)
}

pub(crate) fn step_page(editor: &mut Editor, step: isize) {
    if let Some(page) = current_page(editor) {
        show_page(editor, page.saturating_add_signed(step));
    }
}

pub(crate) fn scroll_lines(editor: &mut Editor, lines: f32) {
    if let Some((_, viewport)) = current(editor) {
        scroll(editor, viewport.window, Move::By(lines * LINE));
    }
}

pub(crate) fn scroll_screens(editor: &mut Editor, screens: f32) {
    if let Some((_, viewport)) = current(editor) {
        let by = screens * viewport.height * SCREEN_SHARE;
        scroll(editor, viewport.window, Move::By(by));
    }
}

pub(crate) fn first_page(editor: &mut Editor) {
    show_page(editor, 0);
}

pub(crate) fn last_page(editor: &mut Editor) {
    let Some((id, viewport)) = current(editor) else {
        return;
    };
    let Some((_, view)) = editor.applet_buffer_mut::<PdfView>(id) else {
        return;
    };
    view.page = view.pages().saturating_sub(1);
    let to = match view.layout {
        Layout::Continuous => view.placed(area(viewport)).1,
        Layout::Single => 0.0,
    };
    scroll(editor, viewport.window, Move::To(to));
    request_pages(editor, id);
}

pub(crate) fn goto_page(editor: &mut Editor) {
    if current(editor).is_none() {
        return;
    }
    editor.prompt("Go to page: ", |editor, answer| {
        match answer.trim().parse::<usize>() {
            Ok(page) if page > 0 => show_page(editor, page - 1),
            _ => editor.show_message(format!("No page {answer}")),
        }
    });
}

fn change_zoom(editor: &mut Editor, zoom: impl FnOnce(&PdfView, Viewport) -> Zoom) {
    let Some((id, viewport)) = current(editor) else {
        return;
    };
    let Some((_, view)) = editor.applet_buffer_mut::<PdfView>(id) else {
        return;
    };
    view.zoom = zoom(view, viewport);
    let message = match view.zoom {
        Zoom::FitWidth => "Fit width".to_owned(),
        Zoom::FitPage => "Fit page".to_owned(),
        Zoom::Scale(scale) => format!("Zoom {:.0}%", scale * 100.0),
    };
    let page = view.page;
    editor.show_message(message);
    show_page(editor, page);
}

pub(crate) fn zoom_by(editor: &mut Editor, factor: f32) {
    change_zoom(editor, |view, viewport| {
        let page = view
            .document()
            .and_then(|document| document.pages.get(view.page))
            .copied()
            .unwrap_or(Size {
                width: 612.0,
                height: 792.0,
            });
        let current = layout::scale(view.zoom, page, area(viewport));
        layout::zoomed(current, factor)
    });
}

pub(crate) fn set_zoom(editor: &mut Editor, zoom: Zoom) {
    change_zoom(editor, |_, _| zoom);
}

pub(crate) fn toggle_layout(editor: &mut Editor) {
    let Some((id, _)) = current(editor) else {
        return;
    };
    let Some((_, view)) = editor.applet_buffer_mut::<PdfView>(id) else {
        return;
    };
    view.layout = match view.layout {
        Layout::Continuous => Layout::Single,
        Layout::Single => Layout::Continuous,
    };
    let message = match view.layout {
        Layout::Continuous => "Scrolling pages",
        Layout::Single => "One page at a time",
    };
    let page = view.page;
    editor.show_message(message);
    show_page(editor, page);
}

pub(crate) fn toggle_dark(editor: &mut Editor) {
    let Some((id, _)) = current(editor) else {
        return;
    };
    if let Some((_, view)) = editor.applet_buffer_mut::<PdfView>(id) {
        view.dark = !view.dark;
    }
    request_pages(editor, id);
}

pub(crate) fn reload(editor: &mut Editor) {
    if let Some((id, _)) = current(editor) {
        load(editor, id);
    }
}
#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use oxmacs_core::editor::Editor;

    use super::*;
    use crate::testing::{deliver, press, requests, type_text};
    use crate::{PDFIUM, Pdf};

    const LETTER: Size = Size {
        width: 612.0,
        height: 792.0,
    };

    fn editor_in(root: &Path, installed: bool) -> Editor {
        let mut editor = Editor::new();
        editor.set_tools_root(Some(root.join("tools")));
        if installed {
            let folder = root.join("tools").join("pdfium").join("8111");
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join(PDFIUM.binary()), "library").unwrap();
        }
        editor.install(&Pdf::default());
        editor
    }

    fn library(root: &Path) -> PathBuf {
        root.join("tools")
            .join("pdfium")
            .join("8111")
            .join(PDFIUM.binary())
    }

    fn opened(pages: usize) -> (tempfile::TempDir, Editor) {
        let root = tempfile::tempdir().unwrap();
        let mut editor = editor_in(root.path(), true);
        editor.open_file(&root.path().join("paper.pdf"));
        editor.take_effects();
        deliver(
            &mut editor,
            Opened {
                doc: 0,
                result: Ok(Document {
                    pages: vec![LETTER; pages],
                    outline: Vec::new(),
                }),
            },
        );
        let window = editor.selected_window();
        deliver(
            &mut editor,
            Viewport {
                window,
                offset: 0.0,
                width: 644.0,
                height: 500.0,
            },
        );
        editor.take_effects();
        (root, editor)
    }

    fn scrolls(editor: &mut Editor) -> Vec<Move> {
        requests::<ScrollPdf>(editor)
            .into_iter()
            .map(|scroll| scroll.to)
            .collect()
    }

    fn view(editor: &Editor) -> &PdfView {
        editor.pane().applet_buffer::<PdfView>().unwrap()
    }

    #[test]
    fn opening_a_pdf_shows_a_viewer_and_asks_for_the_document() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = editor_in(root.path(), true);
        let path = root.path().join("paper.pdf");

        assert!(editor.open_file(&path));

        assert_eq!(editor.buffer().name(), "paper.pdf");
        assert!(editor.buffer().is_read_only());
        assert_eq!(
            requests::<OpenDocument>(&mut editor),
            [OpenDocument {
                doc: 0,
                path,
                library: library(root.path()),
            }]
        );
        assert_eq!(page_counter(&editor).as_deref(), Some("loading"));
    }

    #[test]
    fn without_pdfium_the_viewer_says_how_to_get_it() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = editor_in(root.path(), false);

        editor.open_file(&root.path().join("paper.pdf"));

        assert_eq!(editor.message(), Some(MISSING));
        assert_eq!(requests::<OpenDocument>(&mut editor), []);
        assert_eq!(view(&editor).loading, Loading::Failed(MISSING.to_owned()));
    }

    #[test]
    fn opening_the_same_file_again_returns_to_its_viewer() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = editor_in(root.path(), true);
        let path = root.path().join("paper.pdf");
        editor.open_file(&path);
        editor.open_file(&root.path().join("other.pdf"));

        editor.open_file(&path);

        assert_eq!(editor.buffer().name(), "paper.pdf");
        assert_eq!(view(&editor).doc, 0);
    }

    #[test]
    fn documents_that_fail_to_open_say_why() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = editor_in(root.path(), true);
        editor.open_file(&root.path().join("paper.pdf"));

        deliver(
            &mut editor,
            Opened {
                doc: 0,
                result: Err("not a PDF".to_owned()),
            },
        );

        assert_eq!(editor.message(), Some("paper.pdf: not a PDF"));
        assert_eq!(
            view(&editor).loading,
            Loading::Failed("not a PDF".to_owned())
        );
    }

    #[test]
    fn the_pages_in_and_near_view_are_rendered_at_twice_their_width() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = editor_in(root.path(), true);
        editor.open_file(&root.path().join("paper.pdf"));
        deliver(
            &mut editor,
            Opened {
                doc: 0,
                result: Ok(Document {
                    pages: vec![LETTER; 5],
                    outline: Vec::new(),
                }),
            },
        );
        let window = editor.selected_window();

        deliver(
            &mut editor,
            Viewport {
                window,
                offset: 0.0,
                width: 644.0,
                height: 500.0,
            },
        );

        assert_eq!(
            requests::<RenderPages>(&mut editor),
            [RenderPages {
                doc: 0,
                pages: vec![
                    Wanted {
                        page: 0,
                        width: 1224
                    },
                    Wanted {
                        page: 1,
                        width: 1224
                    },
                ],
                dark: false,
            }]
        );
        assert_eq!(page_counter(&editor).as_deref(), Some("1/5"));
    }

    #[test]
    fn n_and_p_scroll_to_the_top_of_the_next_and_previous_page() {
        let (_root, mut editor) = opened(5);

        press(&mut editor, "n");
        assert_eq!(scrolls(&mut editor), [Move::To(804.0)]);

        let window = editor.selected_window();
        deliver(
            &mut editor,
            Viewport {
                window,
                offset: 804.0,
                width: 644.0,
                height: 500.0,
            },
        );
        assert_eq!(page_counter(&editor).as_deref(), Some("2/5"));
        press(&mut editor, "p p");
        assert_eq!(scrolls(&mut editor), [Move::To(0.0), Move::To(0.0)]);
    }

    #[test]
    fn scrolling_keys_move_by_lines_screens_or_to_the_ends() {
        let (_root, mut editor) = opened(5);

        for (keys, to) in [
            ("C-n", Move::By(LINE)),
            ("<up>", Move::By(-LINE)),
            ("SPC", Move::By(450.0)),
            ("M-v", Move::By(-450.0)),
            ("M-<", Move::To(0.0)),
            ("M->", Move::To(4032.0)),
        ] {
            press(&mut editor, keys);
            assert_eq!(scrolls(&mut editor), [to], "{keys}");
        }
    }

    #[test]
    fn goto_page_asks_for_a_number() {
        let (_root, mut editor) = opened(5);

        press(&mut editor, "M-g g");
        type_text(&mut editor, "3");
        press(&mut editor, "RET");

        assert_eq!(scrolls(&mut editor), [Move::To(1608.0)]);
    }

    #[test]
    fn zoom_keys_change_the_scale_and_say_so() {
        let (_root, mut editor) = opened(5);

        press(&mut editor, "+");
        assert_eq!(view(&editor).zoom, Zoom::Scale(0.94));
        assert_eq!(editor.message(), Some("Zoom 94%"));
        assert_ne!(requests::<RenderPages>(&mut editor), []);

        press(&mut editor, "=");
        assert_eq!(view(&editor).zoom, Zoom::Scale(1.0));
        assert_eq!(editor.message(), Some("Zoom 100%"));

        press(&mut editor, "H");
        assert_eq!(view(&editor).zoom, Zoom::FitPage);
        assert_eq!(editor.message(), Some("Fit page"));

        press(&mut editor, "W");
        assert_eq!(view(&editor).zoom, Zoom::FitWidth);
        assert_eq!(editor.message(), Some("Fit width"));
    }

    #[test]
    fn single_page_layout_flips_whole_pages() {
        let (_root, mut editor) = opened(5);

        press(&mut editor, "c");
        assert_eq!(view(&editor).layout, Layout::Single);
        assert_eq!(editor.message(), Some("One page at a time"));
        editor.take_effects();

        press(&mut editor, "n");

        assert_eq!(page_counter(&editor).as_deref(), Some("2/5"));
        let rendered = requests::<RenderPages>(&mut editor);
        assert_eq!(rendered[0].pages[0].page, 1);
        press(&mut editor, "M->");
        assert_eq!(page_counter(&editor).as_deref(), Some("5/5"));
        press(&mut editor, "c");
        assert_eq!(editor.message(), Some("Scrolling pages"));
    }

    #[test]
    fn dark_mode_redraws_the_pages() {
        let (_root, mut editor) = opened(2);

        press(&mut editor, "m");

        assert!(view(&editor).dark);
        assert!(requests::<RenderPages>(&mut editor)[0].dark);
    }

    #[test]
    fn reloading_asks_for_the_document_again() {
        let (root, mut editor) = opened(2);

        press(&mut editor, "g");

        assert_eq!(
            requests::<OpenDocument>(&mut editor),
            [OpenDocument {
                doc: 1,
                path: root.path().join("paper.pdf"),
                library: library(root.path()),
            }]
        );
    }
}
