use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use iced::mouse::{self, Interaction};
use iced::widget::canvas::{self, Canvas};
use iced::widget::image::Handle;
use iced::widget::scrollable::{AbsoluteOffset, Direction, Scrollbar};
use iced::widget::{
    Column, Stack, center, container, image, mouse_area, operation, responsive, scrollable, sensor,
    space, text,
};
use iced::{Alignment, Color, Element, Length, Point, Rectangle, Renderer, Task, Theme};
use oxmacs_core::applet::Payload;
use oxmacs_core::editor::{Editor, Pane, WindowId};
use oxmacs_core::theme::Theme as Faces;
use oxmacs_gui::applet::Context;
use oxmacs_gui::{style, units};

use crate::engine::{self, Bitmap, Ink};
use crate::layout::{GAP, MARGIN, Placed, Size};
use crate::model::Rect;
use crate::text::{self as words, Gesture, IndexDocument, Indexed, Pointer};
use crate::view::{
    DocId, Loading, Move, OpenDocument, Opened, PdfView, RENDER_SCALE, RenderPages, ScrollPdf,
    Viewport,
};

const BUDGET: usize = 384 * 1024 * 1024;
const STOPPED: &str = "PDFium stopped";
const MATCH_ALPHA: f32 = 0.4;
const CURRENT_ALPHA: f32 = 0.6;
const REGION_ALPHA: f32 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rendered {
    pub doc: DocId,
    pub page: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Key {
    doc: DocId,
    page: usize,
    width: usize,
    ink: Option<Ink>,
}

#[derive(Debug)]
struct Entry {
    key: Key,
    handle: Handle,
    bytes: usize,
    used: u64,
}

#[derive(Debug)]
pub(crate) struct Cache {
    entries: Vec<Entry>,
    pending: Vec<Key>,
    failed: Vec<Key>,
    clock: u64,
    budget: usize,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            pending: Vec::new(),
            failed: Vec::new(),
            clock: 0,
            budget: BUDGET,
        }
    }
}

impl Cache {
    fn knows(&self, key: &Key) -> bool {
        self.pending.contains(key)
            || self.failed.contains(key)
            || self.entries.iter().any(|entry| entry.key == *key)
    }

    fn find(&mut self, key: Key) -> Option<Handle> {
        self.clock += 1;
        let clock = self.clock;
        let index = self
            .entries
            .iter()
            .position(|entry| entry.key == key)
            .or_else(|| {
                self.entries.iter().position(|entry| {
                    entry.key.doc == key.doc
                        && entry.key.page == key.page
                        && entry.key.ink == key.ink
                })
            })?;
        let entry = &mut self.entries[index];
        entry.used = clock;
        Some(entry.handle.clone())
    }

    fn insert(&mut self, key: Key, bitmap: Bitmap) {
        self.pending.retain(|known| *known != key);
        self.clock += 1;
        let bytes = bitmap.pixels.len();
        self.entries.push(Entry {
            key,
            handle: Handle::from_rgba(bitmap.width, bitmap.height, bitmap.pixels),
            bytes,
            used: self.clock,
        });
        while self.entries.iter().map(|entry| entry.bytes).sum::<usize>() > self.budget
            && self.entries.len() > 1
        {
            let oldest = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(index, _)| index);
            if let Some(oldest) = oldest {
                self.entries.swap_remove(oldest);
            }
        }
    }

    fn fail(&mut self, key: Key) {
        self.pending.retain(|known| *known != key);
        self.failed.push(key);
    }
}

pub(crate) fn lock(cache: &Mutex<Cache>) -> MutexGuard<'_, Cache> {
    cache.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn ink(faces: &Faces) -> Ink {
    let rgb = |color: oxmacs_core::theme::Color| [color.r, color.g, color.b];
    Ink {
        paper: rgb(faces.default.background),
        text: rgb(faces.default.foreground),
    }
}

pub(crate) fn pdf_id(window: WindowId) -> iced::widget::Id {
    iced::widget::Id::from(format!("pdf-{window:?}"))
}

pub(crate) fn open(request: &OpenDocument) -> Task<Payload> {
    let doc = request.doc;
    match engine::shared(&request.library) {
        Ok(engine) => Task::perform(engine.open(doc, request.path.clone()), move |result| {
            Payload::new(Opened {
                doc,
                result: result.unwrap_or_else(|_| Err(STOPPED.to_owned())),
            })
        }),
        Err(error) => Task::done(Payload::new(Opened {
            doc,
            result: Err(error),
        })),
    }
}

pub(crate) fn render(cache: &Arc<Mutex<Cache>>, request: &RenderPages, ink: Ink) -> Task<Payload> {
    let Some(engine) = engine::running() else {
        return Task::none();
    };
    let ink = request.dark.then_some(ink);
    let mut tasks = Vec::new();
    let mut known = lock(cache);
    for wanted in &request.pages {
        let key = Key {
            doc: request.doc,
            page: wanted.page,
            width: wanted.width,
            ink,
        };
        if known.knows(&key) {
            continue;
        }
        known.pending.push(key);
        let shared = Arc::clone(cache);
        tasks.push(Task::perform(
            engine.render(key.doc, key.page, key.width, key.ink),
            move |result| {
                let mut cache = lock(&shared);
                match result {
                    Ok(Ok(bitmap)) => cache.insert(key, bitmap),
                    _ => cache.fail(key),
                }
                Payload::new(Rendered {
                    doc: key.doc,
                    page: key.page,
                })
            },
        ));
    }
    Task::batch(tasks)
}

pub(crate) fn index(request: IndexDocument) -> Task<Payload> {
    let doc = request.doc;
    let Some(engine) = engine::running() else {
        return Task::none();
    };
    Task::perform(engine.index(doc), move |result| {
        Payload::new(Indexed {
            doc,
            result: result.unwrap_or_else(|_| Err(STOPPED.to_owned())),
        })
    })
}

struct Marks {
    rects: Vec<(Rect, Color)>,
    scale: f32,
}

impl canvas::Program<Payload> for Marks {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        for (rect, color) in &self.rects {
            frame.fill_rectangle(
                Point::new(rect.left * self.scale, rect.top * self.scale),
                iced::Size::new(
                    (rect.right - rect.left) * self.scale,
                    (rect.bottom - rect.top) * self.scale,
                ),
                *color,
            );
        }
        vec![frame.into_geometry()]
    }
}

fn tinted(color: oxmacs_core::theme::Color, alpha: f32) -> Color {
    Color {
        a: alpha,
        ..style::color(color)
    }
}

fn marks(
    editor: &Editor,
    pane: Pane<'_>,
    view: &PdfView,
    faces: &Faces,
) -> Vec<(usize, Rect, Color)> {
    let mut marks = Vec::new();
    let buffer = pane.buffer();
    if buffer.mark_active()
        && let Some(mark) = buffer.mark()
    {
        let point = buffer.point();
        let region = tinted(faces.region, REGION_ALPHA);
        marks.extend(
            view.index
                .rects(mark.min(point)..mark.max(point))
                .into_iter()
                .map(|(page, rect)| (page, rect, region)),
        );
    }
    if editor.buffer().id() == buffer.id()
        && let Some(search) = editor.search()
    {
        let found = search.found();
        for range in search.matches() {
            let color = if found == Some(range) {
                tinted(faces.search_current, CURRENT_ALPHA)
            } else {
                tinted(faces.search_match, MATCH_ALPHA)
            };
            marks.extend(
                view.index
                    .rects(range.clone())
                    .into_iter()
                    .map(|(page, rect)| (page, rect, color)),
            );
        }
    }
    marks
}

pub(crate) fn scroll(scroll: &ScrollPdf) -> Task<Payload> {
    let id = pdf_id(scroll.window);
    match scroll.to {
        Move::By(y) => operation::scroll_by(id, AbsoluteOffset { x: 0.0, y }),
        Move::To(y) => operation::scroll_to(
            id,
            AbsoluteOffset {
                x: None,
                y: Some(y),
            },
        ),
    }
}

fn notice(message: &str, context: &Context) -> Element<'static, Payload> {
    center(
        text(message.to_owned())
            .size(context.size)
            .color(style::color(context.faces.shadow)),
    )
    .into()
}

pub(crate) fn body<'a>(
    editor: &'a Editor,
    cache: &Arc<Mutex<Cache>>,
    theme_ink: Ink,
    pane: Pane<'a>,
    window: WindowId,
    context: &Context,
) -> Option<Element<'a, Payload>> {
    let view = pane.applet_buffer::<PdfView>()?;
    Some(match &view.loading {
        Loading::Waiting => notice("Opening…", context),
        Loading::Failed(error) => notice(error, context),
        Loading::Ready(_) => {
            let marks = marks(editor, pane, view, &context.faces);
            pages(view, Arc::clone(cache), theme_ink, window, marks)
        }
    })
}

#[derive(Debug, Clone, Copy)]
struct Sheet {
    page: usize,
    placed: Placed,
    size: Size,
    ink: Option<Ink>,
    paper: Color,
}

fn sheet_view<'a>(
    view: &'a PdfView,
    cache: &mut Cache,
    sheet: Sheet,
    window: WindowId,
    marks: &[(usize, Rect, Color)],
) -> Element<'a, Payload> {
    let Sheet {
        page,
        placed,
        size,
        ink,
        paper,
    } = sheet;
    let key = Key {
        doc: view.doc,
        page,
        width: units::floor_to_usize(placed.size.width * RENDER_SCALE),
        ink,
    };
    let drawn: Element<'a, Payload> = match cache.find(key) {
        Some(handle) => image(handle)
            .width(placed.size.width)
            .height(placed.size.height)
            .into(),
        None => container(space())
            .width(placed.size.width)
            .height(placed.size.height)
            .style(move |_| container::Style {
                background: Some(paper.into()),
                ..container::Style::default()
            })
            .into(),
    };
    let scale = placed.size.width / size.width.max(1.0);
    let rects: Vec<(Rect, Color)> = marks
        .iter()
        .filter(|(marked, ..)| *marked == page)
        .map(|(_, rect, color)| (*rect, *color))
        .collect();
    let content: Element<'a, Payload> = if rects.is_empty() {
        drawn
    } else {
        Stack::new()
            .push(drawn)
            .push(
                Canvas::new(Marks { rects, scale })
                    .width(placed.size.width)
                    .height(placed.size.height),
            )
            .into()
    };
    let interaction = if words::over_link(view, page) {
        Interaction::Pointer
    } else {
        Interaction::Text
    };
    let pointer = move |gesture| {
        Payload::new(Pointer {
            window,
            page,
            gesture,
        })
    };
    mouse_area(content)
        .interaction(interaction)
        .on_move(move |at| pointer(Gesture::Moved(at.x / scale, at.y / scale)))
        .on_press(pointer(Gesture::Pressed))
        .on_release(pointer(Gesture::Released))
        .into()
}

fn pages(
    view: &PdfView,
    cache: Arc<Mutex<Cache>>,
    theme_ink: Ink,
    window: WindowId,
    marks: Vec<(usize, Rect, Color)>,
) -> Element<'_, Payload> {
    let ink = view.dark.then_some(theme_ink);
    let paper = ink.map_or(Color::WHITE, |ink| {
        Color::from_rgb8(ink.paper[0], ink.paper[1], ink.paper[2])
    });
    let offset = view.viewport(window).map_or(0.0, |seen| seen.offset);
    responsive(move |size| {
        let area = Size {
            width: size.width,
            height: size.height,
        };
        let (placed, _) = view.placed(area);
        let shown = view.shown();
        let widest = placed
            .iter()
            .map(|page| page.size.width)
            .fold(0.0_f32, f32::max);
        let wide = widest + 2.0 * MARGIN > size.width;
        let mut known = lock(&cache);
        let pages = placed.iter().zip(&shown).map(|(placed, (page, size))| {
            let sheet = Sheet {
                page: *page,
                placed: *placed,
                size: *size,
                ink,
                paper,
            };
            sheet_view(view, &mut known, sheet, window, &marks)
        });
        let column = Column::with_children(pages)
            .spacing(GAP)
            .padding([GAP, if wide { MARGIN } else { 0.0 }])
            .align_x(Alignment::Center);
        let column = if wide {
            column
        } else {
            column.width(Length::Fill)
        };
        let direction = if wide {
            Direction::Both {
                vertical: Scrollbar::default(),
                horizontal: Scrollbar::default(),
            }
        } else {
            Direction::Vertical(Scrollbar::default())
        };
        let scroller = scrollable(column)
            .id(pdf_id(window))
            .direction(direction)
            .width(Length::Fill)
            .height(Length::Fill)
            .on_scroll(move |seen| {
                Payload::new(Viewport {
                    window,
                    offset: seen.absolute_offset().y,
                    width: seen.bounds().width,
                    height: seen.bounds().height,
                })
            });
        let measured = move |size: iced::Size| {
            Payload::new(Viewport {
                window,
                offset,
                width: size.width,
                height: size.height,
            })
        };
        sensor(scroller)
            .on_show(measured)
            .on_resize(measured)
            .into()
    })
    .into()
}

#[cfg(test)]
mod tests {
    use iced::theme::Palette;
    use iced::time::Instant;
    use oxmacs_core::editor::Editor;
    use oxmacs_core::theme::ADWAITA;

    use super::*;
    use crate::Pdf;

    const INK: Ink = Ink {
        paper: [0, 0, 0],
        text: [255, 255, 255],
    };

    fn bitmap(width: u32) -> Bitmap {
        Bitmap {
            width,
            height: 1,
            pixels: vec![255; usize::try_from(width).unwrap() * 4],
        }
    }

    fn key(page: usize, width: usize) -> Key {
        Key {
            doc: 0,
            page,
            width,
            ink: None,
        }
    }

    #[test]
    fn pages_are_found_at_their_width_or_shown_from_another_size() {
        let mut cache = Cache::default();
        cache.pending.push(key(1, 100));
        assert!(cache.knows(&key(1, 100)));

        cache.insert(key(1, 100), bitmap(100));

        assert_eq!(cache.pending, []);
        assert!(cache.find(key(1, 100)).is_some());
        assert!(cache.find(key(1, 120)).is_some());
        assert!(!cache.knows(&key(1, 120)));
        assert!(cache.find(key(2, 100)).is_none());
        assert!(
            cache
                .find(Key {
                    ink: Some(INK),
                    ..key(1, 100)
                })
                .is_none()
        );
    }

    #[test]
    fn failed_pages_are_not_tried_again() {
        let mut cache = Cache::default();
        cache.pending.push(key(0, 100));

        cache.fail(key(0, 100));

        assert!(cache.knows(&key(0, 100)));
        assert!(cache.find(key(0, 100)).is_none());
    }

    #[test]
    fn the_pages_seen_longest_ago_leave_first() {
        let mut cache = Cache {
            budget: 1000,
            ..Cache::default()
        };
        cache.insert(key(0, 100), bitmap(100));
        cache.insert(key(1, 100), bitmap(100));
        cache.find(key(0, 100));

        cache.insert(key(2, 100), bitmap(100));

        assert!(cache.find(key(0, 100)).is_some());
        assert!(!cache.knows(&key(1, 100)));
        assert!(cache.knows(&key(2, 100)));
    }

    #[test]
    fn without_pdfium_the_window_says_how_to_get_it() {
        let root = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        editor.set_tools_root(Some(root.path().to_owned()));
        let pdf = Pdf::default();
        editor.install(&pdf);
        editor.open_file(&root.path().join("paper.pdf"));
        let context = Context {
            faces: ADWAITA,
            palette: Palette::LIGHT,
            font: iced::Font::DEFAULT,
            size: 14.0,
            now: Instant::now(),
            animations: false,
        };

        let drawn = body(
            &editor,
            &pdf.cache,
            pdf.ink,
            editor.pane(),
            editor.selected_window(),
            &context,
        )
        .unwrap();

        let mut ui = iced_test::simulator(drawn);
        assert!(
            ui.find("PDFium is not installed; M-x list-tools installs it")
                .is_ok()
        );
    }
}
