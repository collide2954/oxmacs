use std::ops::Range;

use oxmacs_core::editor::{Editor, Offer, Picked, WindowId};
use oxmacs_core::text::BufferId;

use crate::layout;

use crate::model::{Link, PageText, Rect, Target};
use crate::view::{
    self, DocId, Layout, Move, PdfView, Viewport, area, current, find, scroll, show_page, showing,
};

const PAGE_BREAK: char = '\u{c}';
const SAFE_SCHEMES: &[&str] = &["http://", "https://", "mailto:"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexDocument {
    pub doc: DocId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Indexed {
    pub doc: DocId,
    pub result: Result<Vec<PageText>, String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    Moved(f32, f32),
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pointer {
    pub window: WindowId,
    pub page: usize,
    pub gesture: Gesture,
}

#[derive(Debug, Default)]
pub(crate) struct TextIndex {
    starts: Vec<usize>,
    pages: Vec<PageText>,
}

fn same_line(last: &Rect, next: &Rect) -> bool {
    let middle = f32::midpoint(next.top, next.bottom);
    let height = (next.bottom - next.top).max(1.0);
    (last.top..=last.bottom).contains(&middle)
        && next.left >= last.left - 1.0
        && next.left - last.right < 2.0 * height
}

impl TextIndex {
    fn build(pages: Vec<PageText>) -> (Self, String) {
        let mut text = String::new();
        let mut starts = Vec::with_capacity(pages.len());
        let mut count = 0;
        for page in &pages {
            starts.push(count);
            text.push_str(&page.text);
            text.push(PAGE_BREAK);
            count += page.text.chars().count() + 1;
        }
        (Self { starts, pages }, text)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub(crate) fn start_of(&self, page: usize) -> Option<usize> {
        self.starts.get(page).copied()
    }

    fn page_of(&self, char: usize) -> Option<(usize, usize)> {
        let page = self
            .starts
            .partition_point(|start| *start <= char)
            .checked_sub(1)?;
        let offset = char - self.starts[page];
        (offset <= self.pages[page].boxes.len()).then_some((page, offset))
    }

    pub(crate) fn locate(&self, char: usize) -> Option<(usize, Rect)> {
        let (page, offset) = self.page_of(char)?;
        let boxes = &self.pages[page].boxes;
        let before = offset.min(boxes.len());
        boxes[..before]
            .iter()
            .chain(boxes.get(before))
            .rev()
            .find(|rect| !rect.is_empty())
            .map(|rect| (page, *rect))
    }

    pub(crate) fn rects(&self, range: Range<usize>) -> Vec<(usize, Rect)> {
        let mut rects: Vec<(usize, Rect)> = Vec::new();
        for char in range {
            let Some((page, rect)) = self
                .page_of(char)
                .and_then(|(page, offset)| Some((page, *self.pages[page].boxes.get(offset)?)))
                .filter(|(_, rect)| !rect.is_empty())
            else {
                continue;
            };
            match rects.last_mut() {
                Some((known, last)) if *known == page && same_line(last, &rect) => {
                    last.left = last.left.min(rect.left);
                    last.right = last.right.max(rect.right);
                    last.top = last.top.min(rect.top);
                    last.bottom = last.bottom.max(rect.bottom);
                }
                _ => rects.push((page, rect)),
            }
        }
        rects
    }

    fn char_at(&self, page: usize, x: f32, y: f32) -> Option<usize> {
        let text = self.pages.get(page)?;
        let (offset, _) = text
            .boxes
            .iter()
            .enumerate()
            .filter(|(_, rect)| !rect.is_empty())
            .min_by(|(_, a), (_, b)| a.distance(x, y).total_cmp(&b.distance(x, y)))?;
        Some(self.starts[page] + offset)
    }

    fn link_at(&self, page: usize, x: f32, y: f32) -> Option<&Link> {
        self.pages
            .get(page)?
            .links
            .iter()
            .find(|link| link.rect.contains(x, y))
    }
}

#[derive(Debug, Default)]
pub(crate) struct Pointing {
    at: Option<(usize, f32, f32)>,
    anchor: Option<usize>,
    dragged: bool,
}

pub(crate) fn over_link(view: &PdfView, page: usize) -> bool {
    view.pointing
        .at
        .filter(|(known, ..)| *known == page)
        .and_then(|(_, x, y)| view.index.link_at(page, x, y))
        .is_some()
}

pub(crate) fn indexed(editor: &mut Editor, indexed: &Indexed) {
    let Some(id) = find(editor, indexed.doc) else {
        return;
    };
    let pages = match &indexed.result {
        Ok(pages) => pages.clone(),
        Err(error) => {
            editor.show_message(format!("Could not read the document's text: {error}"));
            return;
        }
    };
    let (index, text) = TextIndex::build(pages);
    if let Some((buffer, view)) = editor.applet_buffer_mut::<PdfView>(id) {
        buffer.replace_text(&text);
        buffer.set_point(0);
        buffer.mark_saved();
        view.index = index;
        view.revealed = Some(0);
    }
}

pub(crate) fn sync_point(editor: &mut Editor, id: BufferId, viewport: Viewport) {
    let Some((buffer, view)) = editor.applet_buffer_mut::<PdfView>(id) else {
        return;
    };
    let Some((page, _)) = view.index.page_of(buffer.point()) else {
        return;
    };
    let (placed, _) = view.placed(area(viewport));
    let shown = view.shown();
    let visible = layout::visible(&placed, viewport.offset, viewport.height);
    if visible.into_iter().any(|slot| shown[slot].0 == page) {
        return;
    }
    if let Some(start) = view.index.start_of(view.page) {
        buffer.set_point(start);
        view.revealed = Some(start);
    }
}

pub(crate) fn follow_point(editor: &mut Editor) {
    let Some((id, viewport)) = current(editor) else {
        return;
    };
    let Some((buffer, view)) = editor.applet_buffer_mut::<PdfView>(id) else {
        return;
    };
    let point = buffer.point();
    if view.revealed == Some(point) || view.index.is_empty() {
        return;
    }
    view.revealed = Some(point);
    let Some((page, rect)) = view.index.locate(point) else {
        return;
    };
    let flipped = view.layout == Layout::Single && view.page != page;
    view.page = page;
    let (placed, _) = view.placed(area(viewport));
    let slot = match view.layout {
        Layout::Continuous => page,
        Layout::Single => 0,
    };
    let (Some(placement), Some(size)) = (
        placed.get(slot),
        view.document()
            .and_then(|document| document.pages.get(page)),
    ) else {
        return;
    };
    let scale = placement.size.width / size.width.max(1.0);
    let top = placement.top + rect.top * scale;
    let bottom = placement.top + rect.bottom * scale;
    let shown = top >= viewport.offset && bottom <= viewport.offset + viewport.height;
    if shown && !flipped {
        return;
    }
    let to = (top - viewport.height / 3.0).max(0.0);
    scroll(editor, viewport.window, Move::To(to));
    if flipped {
        view::request_pages(editor, id);
    }
}

pub(crate) fn pointed(editor: &mut Editor, pointer: &Pointer) {
    let Some(id) = showing(editor, pointer.window) else {
        return;
    };
    if pointer.gesture == Gesture::Pressed {
        editor.select_window(pointer.window);
    }
    let Some((buffer, view)) = editor.applet_buffer_mut::<PdfView>(id) else {
        return;
    };
    let at = match pointer.gesture {
        Gesture::Moved(x, y) => {
            view.pointing.at = Some((pointer.page, x, y));
            Some((x, y))
        }
        Gesture::Pressed | Gesture::Released => view
            .pointing
            .at
            .filter(|(page, ..)| *page == pointer.page)
            .map(|(_, x, y)| (x, y)),
    };
    let here = at.and_then(|(x, y)| view.index.char_at(pointer.page, x, y));
    match pointer.gesture {
        Gesture::Moved(..) => {
            if let (Some(anchor), Some(here)) = (view.pointing.anchor, here)
                && (here != anchor || view.pointing.dragged)
            {
                view.pointing.dragged = true;
                let (mark, point) = if here >= anchor {
                    (anchor, here + 1)
                } else {
                    (anchor + 1, here)
                };
                buffer.set_mark(mark);
                buffer.set_point(point);
                view.revealed = Some(point);
            }
        }
        Gesture::Pressed => {
            view.pointing.anchor = here;
            view.pointing.dragged = false;
            buffer.deactivate_mark();
        }
        Gesture::Released => {
            let dragged = view.pointing.dragged;
            view.pointing.anchor = None;
            view.pointing.dragged = false;
            if dragged {
                return;
            }
            if let Some(here) = here {
                buffer.set_point(here);
                view.revealed = Some(here);
            }
            let target = at
                .and_then(|(x, y)| view.index.link_at(pointer.page, x, y))
                .map(|link| link.target.clone());
            if let Some(target) = target {
                follow(editor, target);
            }
        }
    }
}

pub(crate) fn outline(editor: &mut Editor) {
    let Some(view) = editor.pane().applet_buffer::<PdfView>() else {
        return;
    };
    let headings = view
        .document()
        .map(|document| document.outline.clone())
        .unwrap_or_default();
    if headings.is_empty() {
        editor.show_message("This document has no outline");
        return;
    }
    let offers: Vec<Offer> = headings
        .iter()
        .map(|heading| {
            Offer::noted(
                format!("{}{}", "  ".repeat(heading.depth), heading.title),
                format!("p. {}", heading.page + 1),
            )
        })
        .collect();
    editor.prompt_pick(
        "Go to heading: ",
        move |_| offers.clone(),
        move |editor, picked| {
            if let Picked::Offered(index) = picked
                && let Some(heading) = headings.get(index)
            {
                show_page(editor, heading.page);
            }
        },
    );
}

fn follow(editor: &mut Editor, target: Target) {
    match target {
        Target::Page(page) => show_page(editor, page),
        Target::Uri(uri)
            if SAFE_SCHEMES
                .iter()
                .any(|scheme| uri.to_ascii_lowercase().starts_with(scheme)) =>
        {
            editor.open_externally(uri);
        }
        Target::Uri(uri) => editor.show_message(format!("Link: {uri}")),
    }
}
#[cfg(test)]
mod tests {
    use std::fs;

    use oxmacs_core::editor::{Editor, Effect};

    use super::*;
    use crate::layout::Size;
    use crate::model::{Document, Heading};
    use crate::testing::{deliver, press, requests, type_text};
    use crate::view::{Move, Opened, PdfView, ScrollPdf, Viewport};
    use crate::{PDFIUM, Pdf};

    const LETTER: Size = Size {
        width: 612.0,
        height: 792.0,
    };

    fn line(text: &str, top: f32) -> PageText {
        let boxes = (0..text.chars().count())
            .map(|index| {
                let left = 72.0 + 6.0 * f32::from(u16::try_from(index).unwrap());
                Rect {
                    left,
                    top,
                    right: left + 5.0,
                    bottom: top + 10.0,
                }
            })
            .collect();
        PageText {
            text: text.to_owned(),
            boxes,
            links: Vec::new(),
        }
    }

    fn viewer(outline: Vec<Heading>) -> (tempfile::TempDir, Editor) {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("tools").join("pdfium").join("8111");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(PDFIUM.binary()), "library").unwrap();
        let mut editor = Editor::new();
        editor.set_tools_root(Some(root.path().join("tools")));
        editor.install(&Pdf::default());
        editor.open_file(&root.path().join("paper.pdf"));
        deliver(
            &mut editor,
            Opened {
                doc: 0,
                result: Ok(Document {
                    pages: vec![LETTER; 5],
                    outline,
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

    fn indexed_viewer(pages: Vec<PageText>) -> (tempfile::TempDir, Editor) {
        let (root, mut editor) = viewer(Vec::new());
        deliver(
            &mut editor,
            Indexed {
                doc: 0,
                result: Ok(pages),
            },
        );
        editor.take_effects();
        (root, editor)
    }

    fn pages() -> Vec<PageText> {
        let mut first = line("Hello world", 100.0);
        first.links.push(Link {
            rect: Rect {
                left: 72.0,
                top: 100.0,
                right: 100.0,
                bottom: 110.0,
            },
            target: Target::Page(3),
        });
        vec![
            first,
            line("second page", 50.0),
            line("third", 50.0),
            line("a needle here", 100.0),
            line("last", 50.0),
        ]
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

    fn point_at(editor: &mut Editor, page: usize, x: f32, y: f32) {
        let window = editor.selected_window();
        deliver(
            editor,
            Pointer {
                window,
                page,
                gesture: Gesture::Moved(x, y),
            },
        );
    }

    fn click(editor: &mut Editor, page: usize) {
        let window = editor.selected_window();
        for gesture in [Gesture::Pressed, Gesture::Released] {
            deliver(
                editor,
                Pointer {
                    window,
                    page,
                    gesture,
                },
            );
        }
    }

    #[test]
    fn opened_documents_ask_for_their_text() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("tools").join("pdfium").join("8111");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(PDFIUM.binary()), "library").unwrap();
        let mut editor = Editor::new();
        editor.set_tools_root(Some(root.path().join("tools")));
        editor.install(&Pdf::default());
        editor.open_file(&root.path().join("paper.pdf"));

        deliver(
            &mut editor,
            Opened {
                doc: 0,
                result: Ok(Document::default()),
            },
        );

        assert_eq!(
            requests::<IndexDocument>(&mut editor),
            [IndexDocument { doc: 0 }]
        );
    }

    #[test]
    fn the_text_fills_the_buffer_one_page_after_another() {
        let (_root, editor) = indexed_viewer(pages());

        assert_eq!(
            editor.buffer().contents(),
            "Hello world\u{c}second page\u{c}third\u{c}a needle here\u{c}last\u{c}"
        );
        assert!(!editor.buffer().is_modified());
        assert_eq!(editor.buffer().point(), 0);
    }

    #[test]
    fn characters_are_found_on_their_pages() {
        let (_root, editor) = indexed_viewer(pages());
        let index = &view(&editor).index;

        assert_eq!(
            index.locate(14),
            Some((
                1,
                Rect {
                    left: 84.0,
                    top: 50.0,
                    right: 89.0,
                    bottom: 60.0
                }
            ))
        );
        assert_eq!(index.locate(11).map(|(page, _)| page), Some(0));
        assert_eq!(index.locate(1000), None);
    }

    #[test]
    fn ranges_become_one_rectangle_per_line_and_page() {
        let (_root, editor) = indexed_viewer(pages());

        assert_eq!(
            view(&editor).index.rects(6..18),
            [
                (
                    0,
                    Rect {
                        left: 108.0,
                        top: 100.0,
                        right: 137.0,
                        bottom: 110.0
                    }
                ),
                (
                    1,
                    Rect {
                        left: 72.0,
                        top: 50.0,
                        right: 107.0,
                        bottom: 60.0
                    }
                ),
            ]
        );
    }

    #[test]
    fn searching_scrolls_the_match_into_view() {
        let (_root, mut editor) = indexed_viewer(pages());

        press(&mut editor, "C-s");
        type_text(&mut editor, "needle");

        let to = scrolls(&mut editor);
        let Some(Move::To(y)) = to.last() else {
            panic!("{to:?}");
        };
        let expected = 2424.0 + 100.0 - 500.0 / 3.0;
        assert!((y - expected).abs() < 0.5, "{y} {expected}");
        assert_eq!(
            editor.search().and_then(|search| search.found().cloned()),
            Some(32..38)
        );
    }

    #[test]
    fn scrolling_away_brings_point_to_the_page_in_view() {
        let (_root, mut editor) = indexed_viewer(pages());
        let window = editor.selected_window();

        deliver(
            &mut editor,
            Viewport {
                window,
                offset: 2424.0,
                width: 644.0,
                height: 500.0,
            },
        );
        press(&mut editor, "C-s");
        type_text(&mut editor, "e");

        assert_eq!(
            editor.search().and_then(|search| search.found().cloned()),
            Some(33..34)
        );
        assert_eq!(scrolls(&mut editor), []);
    }

    #[test]
    fn flipping_single_pages_takes_point_along() {
        let (_root, mut editor) = indexed_viewer(pages());

        press(&mut editor, "c n");

        assert_eq!(editor.buffer().point(), 12);
    }

    #[test]
    fn point_stays_on_a_page_that_is_still_in_view() {
        let (_root, mut editor) = indexed_viewer(pages());
        editor.buffer_mut().set_point(14);
        let window = editor.selected_window();

        deliver(
            &mut editor,
            Viewport {
                window,
                offset: 400.0,
                width: 644.0,
                height: 500.0,
            },
        );

        assert_eq!(editor.buffer().point(), 14);
    }

    #[test]
    fn matches_already_in_view_do_not_scroll() {
        let (_root, mut editor) = indexed_viewer(pages());

        press(&mut editor, "C-s");
        type_text(&mut editor, "world");

        assert_eq!(scrolls(&mut editor), []);
    }

    #[test]
    fn dragging_over_text_selects_it_for_copying() {
        let (_root, mut editor) = indexed_viewer(pages());

        point_at(&mut editor, 0, 74.0, 105.0);
        let window = editor.selected_window();
        deliver(
            &mut editor,
            Pointer {
                window,
                page: 0,
                gesture: Gesture::Pressed,
            },
        );
        point_at(&mut editor, 0, 98.0, 105.0);
        deliver(
            &mut editor,
            Pointer {
                window,
                page: 0,
                gesture: Gesture::Released,
            },
        );

        let buffer = editor.buffer();
        assert!(buffer.mark_active());
        assert_eq!((buffer.mark(), buffer.point()), (Some(0), 5));
        assert_eq!(scrolls(&mut editor), []);
    }

    #[test]
    fn clicking_a_link_goes_to_its_page() {
        let (_root, mut editor) = indexed_viewer(pages());

        point_at(&mut editor, 0, 80.0, 105.0);
        click(&mut editor, 0);

        assert_eq!(scrolls(&mut editor), [Move::To(2424.0 - 12.0)]);
        assert!(!editor.buffer().mark_active());
    }

    fn linked(target: Target) -> (tempfile::TempDir, Editor) {
        let mut pages = pages();
        pages[0].links[0].target = target;
        indexed_viewer(pages)
    }

    #[test]
    fn web_links_open_outside() {
        let (_root, mut editor) = linked(Target::Uri("https://www.r-project.org".to_owned()));

        point_at(&mut editor, 0, 80.0, 105.0);
        click(&mut editor, 0);

        assert_eq!(
            editor.take_effects(),
            [Effect::OpenExternally("https://www.r-project.org".into())]
        );
    }

    #[test]
    fn other_links_are_only_shown() {
        let (_root, mut editor) = linked(Target::Uri("file:///etc/passwd".to_owned()));

        point_at(&mut editor, 0, 80.0, 105.0);
        click(&mut editor, 0);

        assert_eq!(editor.take_effects(), []);
        assert_eq!(editor.message(), Some("Link: file:///etc/passwd"));
    }

    #[test]
    fn the_outline_jumps_to_its_headings() {
        let (_root, mut editor) = viewer(vec![
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
        ]);

        press(&mut editor, "o");
        let labels: Vec<String> = editor
            .completions()
            .into_iter()
            .map(|completion| completion.label)
            .collect();
        assert_eq!(labels, ["Introduction", "  Details"]);
        type_text(&mut editor, "Details");
        press(&mut editor, "RET");

        assert_eq!(scrolls(&mut editor), [Move::To(1608.0)]);
    }

    #[test]
    fn documents_without_an_outline_say_so() {
        let (_root, mut editor) = viewer(Vec::new());

        press(&mut editor, "M-g i");

        assert_eq!(editor.message(), Some("This document has no outline"));
        assert!(editor.minibuffer().is_none());
    }
}
