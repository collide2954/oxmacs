use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

use iced::advanced::image::{self, Image};
use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer::{self, Quad};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::widget::image::Handle;
use iced::{Border, Color, Element, Event, Length, Point, Rectangle, Size, Theme, Vector, window};
use oxmacs_core::theme::Theme as Faces;
use oxmacs_pdf::{self as pdf, Bitmap, Document, Ink, PageSize, Target, words};
use oxmacs_script::view::pdf::{Go, View, Zoom};
use oxmacs_script::view::views::{Arg, Press};

use crate::{style, units};

pub const GAP: f32 = 12.0;
pub const MARGIN: f32 = 16.0;
const PIXELS_PER_POINT: f32 = 96.0 / 72.0;
const LINE: f32 = 48.0;
const RENDER_SCALE: f32 = 2.0;
const WIDEST_RENDER: f32 = 4096.0;
const BUDGET: usize = 384 * 1024 * 1024;
const MATCH_ALPHA: f32 = 0.35;
const CURRENT_ALPHA: f32 = 0.6;
const REGION_ALPHA: f32 = 0.35;
const BAR: f32 = 6.0;

pub fn scale(zoom: Zoom, page: PageSize, viewport: Size) -> f32 {
    let across = (viewport.width - 2.0 * MARGIN).max(1.0) / page.width.max(1.0);
    match zoom {
        Zoom::FitWidth => across,
        Zoom::FitPage => across.min((viewport.height - 2.0 * GAP).max(1.0) / page.height.max(1.0)),
        Zoom::Scale(scale) => scale * PIXELS_PER_POINT,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sheet {
    pub page: usize,
    pub top: f32,
    pub size: Size,
    pub scale: f32,
}

pub fn sheets(
    pages: &[PageSize],
    single: Option<usize>,
    zoom: Zoom,
    viewport: Size,
) -> (Vec<Sheet>, Size) {
    let shown: Vec<usize> = match single {
        Some(page) if !pages.is_empty() => vec![page.min(pages.len() - 1)],
        Some(_) => Vec::new(),
        None => (0..pages.len()).collect(),
    };
    let mut top = GAP;
    let mut widest: f32 = 0.0;
    let placed = shown
        .into_iter()
        .map(|page| {
            let size = pages[page];
            let scale = scale(zoom, size, viewport);
            let sheet = Sheet {
                page,
                top,
                size: Size::new(size.width * scale, size.height * scale),
                scale,
            };
            top += sheet.size.height + GAP;
            widest = widest.max(sheet.size.width);
            sheet
        })
        .collect();
    (
        placed,
        Size::new((widest + 2.0 * MARGIN).max(viewport.width), top),
    )
}

pub fn page_at(sheets: &[Sheet], offset: f32, height: f32) -> usize {
    let line = offset + height / 3.0;
    sheets
        .iter()
        .rfind(|sheet| sheet.top <= line)
        .or(sheets.first())
        .map_or(0, |sheet| sheet.page)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    path: PathBuf,
    revision: u64,
    page: usize,
    width: usize,
    ink: Option<Ink>,
}

impl Key {
    fn same_page(&self, other: &Self) -> bool {
        self.path == other.path
            && self.revision == other.revision
            && self.page == other.page
            && self.ink == other.ink
    }
}

#[derive(Debug)]
struct Entry {
    key: Key,
    handle: Handle,
    bytes: usize,
    used: u64,
}

#[derive(Debug)]
struct Cache {
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

    fn find(&mut self, key: &Key) -> Option<(bool, Handle)> {
        self.clock += 1;
        let clock = self.clock;
        let index = self
            .entries
            .iter()
            .position(|entry| entry.key == *key)
            .or_else(|| {
                self.entries
                    .iter()
                    .position(|entry| entry.key.same_page(key))
            })?;
        let entry = &mut self.entries[index];
        entry.used = clock;
        Some((entry.key == *key, entry.handle.clone()))
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

    fn waiting(&self, path: &Path) -> bool {
        self.pending.iter().any(|key| key.path == path)
    }
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Mutex::default);

fn cache() -> MutexGuard<'static, Cache> {
    CACHE.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn drawing() -> bool {
    !cache().pending.is_empty()
}

fn drawn(key: &Key) -> Option<Handle> {
    let mut known = cache();
    let found = known.find(key);
    if found.as_ref().is_none_or(|(exact, _)| !exact) && !known.knows(key) {
        known.pending.push(key.clone());
        drop(known);
        let stored = key.clone();
        let asked = pdf::render(&key.path, key.page, key.width, key.ink, move |result| {
            let mut known = cache();
            match result {
                Ok(bitmap) => known.insert(stored, bitmap),
                Err(_) => known.fail(stored),
            }
        });
        if !asked {
            cache().fail(key.clone());
        }
    }
    found.map(|(_, handle)| handle)
}

fn ink(faces: &Faces) -> Ink {
    let rgb = |color: oxmacs_core::theme::Color| [color.r, color.g, color.b];
    Ink {
        paper: rgb(faces.default.background),
        text: rgb(faces.default.foreground),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Arrangement {
    zoom: Zoom,
    continuous: bool,
    size: Size,
    revision: u64,
}

type Place = (usize, usize);

#[derive(Debug, Default)]
struct State {
    offset: Vector,
    page: usize,
    size: Size,
    applied: Option<i64>,
    arranged: Option<Arrangement>,
    anchor: Option<(usize, f32)>,
    reported: Option<(usize, i64)>,
    pressed: Option<Place>,
    selection: Option<(Place, Place)>,
    dragged: bool,
}

pub struct PdfView<'a, Message> {
    view: &'a View,
    faces: Faces,
    on_call: Box<dyn Fn(usize, Vec<Arg>) -> Message + 'a>,
}

impl<Message> std::fmt::Debug for PdfView<'_, Message> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PdfView")
            .field("path", &self.view.path)
            .finish_non_exhaustive()
    }
}

impl<'a, Message> PdfView<'a, Message> {
    pub fn new(
        view: &'a View,
        faces: Faces,
        on_call: impl Fn(usize, Vec<Arg>) -> Message + 'a,
    ) -> Self {
        Self {
            view,
            faces,
            on_call: Box::new(on_call),
        }
    }

    fn call(&self, press: Option<&Press>, args: Vec<Arg>) -> Option<Message> {
        match press {
            Some(Press::Slot(slot)) => Some((self.on_call)(*slot, args)),
            _ => None,
        }
    }

    fn arrange(&self, document: &Document, state: &State) -> (Vec<Sheet>, Size) {
        sheets(
            &document.pages,
            (!self.view.continuous).then_some(state.page),
            self.view.zoom,
            state.size,
        )
    }

    fn settle(&self, state: &mut State, document: &Document, revision: u64) {
        if document.pages.is_empty() {
            return;
        }
        state.page = state.page.min(document.pages.len() - 1);
        let arrangement = Arrangement {
            zoom: self.view.zoom,
            continuous: self.view.continuous,
            size: state.size,
            revision,
        };
        if state.arranged != Some(arrangement) {
            if !self.view.continuous
                && let Some((page, _)) = state.anchor
                && state.arranged.is_some_and(|before| before.continuous)
            {
                state.page = page;
            }
            let (placed, _) = self.arrange(document, state);
            if let Some((page, fraction)) = state.anchor
                && let Some(sheet) = placed.iter().find(|sheet| sheet.page == page)
            {
                state.offset.y = sheet.top + fraction * sheet.size.height;
            }
            state.arranged = Some(arrangement);
        }
        if let Some((id, to)) = self.view.go
            && state.applied != Some(id)
        {
            state.applied = Some(id);
            self.go(state, document, to);
        }
        self.clamp(state, document);
    }

    fn go(&self, state: &mut State, document: &Document, to: Go) {
        match to {
            Go::Page { page, y } => {
                let page = page.min(document.pages.len().saturating_sub(1));
                let flipped = !self.view.continuous && state.page != page;
                state.page = page;
                let (placed, _) = self.arrange(document, state);
                let Some(sheet) = placed.iter().find(|sheet| sheet.page == page) else {
                    return;
                };
                let height = state.size.height;
                match y {
                    Some(y) => {
                        let target = sheet.top + y * sheet.scale;
                        if flipped
                            || target < state.offset.y + GAP
                            || target > state.offset.y + height - LINE
                        {
                            state.offset.y = target - height / 3.0;
                        }
                    }
                    None => state.offset.y = sheet.top - GAP,
                }
            }
            Go::Lines(lines) => self.scroll(state, document, lines * LINE),
            Go::Screens(screens) => {
                let screen = (state.size.height - LINE).max(LINE);
                self.scroll(state, document, screens * screen);
            }
        }
    }

    fn scroll(&self, state: &mut State, document: &Document, by: f32) {
        let (_, content) = self.arrange(document, state);
        let most = (content.height - state.size.height).max(0.0);
        if !self.view.continuous {
            if by > 0.0 && state.offset.y >= most - 0.5 && state.page + 1 < document.pages.len() {
                state.page += 1;
                state.offset.y = 0.0;
                return;
            }
            if by < 0.0 && state.offset.y <= 0.5 && state.page > 0 {
                state.page -= 1;
                let (_, content) = self.arrange(document, state);
                state.offset.y = (content.height - state.size.height).max(0.0);
                return;
            }
        }
        state.offset.y = (state.offset.y + by).clamp(0.0, most);
    }

    fn clamp(&self, state: &mut State, document: &Document) {
        let (placed, content) = self.arrange(document, state);
        state.offset.y = state
            .offset
            .y
            .clamp(0.0, (content.height - state.size.height).max(0.0));
        state.offset.x = state
            .offset
            .x
            .clamp(0.0, (content.width - state.size.width).max(0.0));
        let page = self.current(state, &placed);
        state.anchor = placed.iter().find(|sheet| sheet.page == page).map(|sheet| {
            (
                page,
                (state.offset.y - sheet.top) / sheet.size.height.max(1.0),
            )
        });
    }

    fn current(&self, state: &State, placed: &[Sheet]) -> usize {
        if self.view.continuous {
            page_at(placed, state.offset.y, state.size.height)
        } else {
            state.page
        }
    }

    fn report(&self, state: &State, document: &Document) -> Option<(usize, i64)> {
        let (placed, _) = self.arrange(document, state);
        let page = self.current(state, &placed);
        let sheet = placed.iter().find(|sheet| sheet.page == page)?;
        let percent = units::floor_to_usize(sheet.scale / PIXELS_PER_POINT * 100.0 + 0.5);
        Some((page, i64::try_from(percent).unwrap_or(100)))
    }

    fn sheet_bounds(sheet: &Sheet, content: Size, bounds: Rectangle, offset: Vector) -> Rectangle {
        Rectangle::new(
            Point::new(
                bounds.x + (content.width - sheet.size.width) / 2.0 - offset.x,
                bounds.y + sheet.top - offset.y,
            ),
            sheet.size,
        )
    }

    fn locate(
        &self,
        state: &State,
        document: &Document,
        bounds: Rectangle,
        point: Point,
    ) -> Option<(usize, f32, f32)> {
        let (placed, content) = self.arrange(document, state);
        placed.iter().find_map(|sheet| {
            let shown = Self::sheet_bounds(sheet, content, bounds, state.offset);
            if shown.contains(point) {
                Some((
                    sheet.page,
                    (point.x - shown.x) / sheet.scale,
                    (point.y - shown.y) / sheet.scale,
                ))
            } else {
                None
            }
        })
    }

    fn place(
        &self,
        state: &State,
        document: &Document,
        bounds: Rectangle,
        point: Point,
    ) -> Option<Place> {
        let (page, x, y) = self.locate(state, document, bounds, point)?;
        let texts = pdf::texts_now(&self.view.path)?;
        Some((page, words::char_at(texts.get(page)?, x, y)?))
    }

    fn link(
        &self,
        state: &State,
        document: &Document,
        bounds: Rectangle,
        point: Point,
    ) -> Option<Target> {
        let (page, x, y) = self.locate(state, document, bounds, point)?;
        let texts = pdf::texts_now(&self.view.path)?;
        words::link_at(texts.get(page)?, x, y).map(|link| link.target.clone())
    }

    fn press(
        &self,
        state: &mut State,
        document: &Document,
        bounds: Rectangle,
        point: Point,
        shell: &mut Shell<'_, Message>,
    ) {
        if let Some(target) = self.link(state, document, bounds, point) {
            let arg = match target {
                Target::Page(page) => Arg::Int(i64::try_from(page).unwrap_or(0)),
                Target::Uri(uri) => Arg::Text(uri),
            };
            if let Some(message) = self.call(self.view.on_link.as_ref(), vec![arg]) {
                shell.publish(message);
            }
            return;
        }
        state.pressed = self.place(state, document, bounds, point);
        state.dragged = false;
        if state.selection.take().is_some()
            && let Some(message) = self.call(self.view.on_select.as_ref(), vec![Arg::Nil])
        {
            shell.publish(message);
        }
    }

    fn released(&self, state: &mut State, shell: &mut Shell<'_, Message>) {
        if state.pressed.take().is_some()
            && state.dragged
            && let Some((from, to)) = state.selection
            && let Some(texts) = pdf::texts_now(&self.view.path)
            && let Some(message) = self.call(
                self.view.on_select.as_ref(),
                vec![Arg::Text(words::between(&texts, from, to))],
            )
        {
            shell.publish(message);
        }
    }

    fn wheel(&self, state: &mut State, document: &Document, delta: mouse::ScrollDelta) {
        let (across, down) = match delta {
            mouse::ScrollDelta::Lines { x, y } => (x * LINE, y * LINE),
            mouse::ScrollDelta::Pixels { x, y } => (x, y),
        };
        if down != 0.0 {
            self.scroll(state, document, -down);
        }
        state.offset.x -= across;
        self.clamp(state, document);
    }
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for PdfView<'_, Message>
where
    Renderer: image::Renderer<Handle = Handle>,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
        let state = tree.state.downcast_mut::<State>();
        state.size = size;
        if let Some((revision, document)) = pdf::document(&self.view.path) {
            self.settle(state, &document, revision);
        }
        layout::Node::new(size)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let Some((_, document)) = pdf::document(&self.view.path) else {
            return;
        };
        let report = self.report(state, &document);
        if report.is_some() && state.reported != report {
            state.reported = report;
            if let Some((page, percent)) = report
                && let Some(message) = self.call(
                    self.view.on_view.as_ref(),
                    vec![
                        Arg::Int(i64::try_from(page).unwrap_or(0)),
                        Arg::Int(percent),
                    ],
                )
            {
                shell.publish(message);
            }
        }
        let bounds = layout.bounds();
        match event {
            Event::Window(window::Event::RedrawRequested(_)) => {
                if cache().waiting(&self.view.path) {
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                self.wheel(state, &document, *delta);
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(point) = cursor.position_over(bounds) {
                    self.press(state, &document, bounds, point, shell);
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(start) = state.pressed
                    && let Some(here) = self.place(state, &document, bounds, *position)
                    && (here != start || state.dragged)
                {
                    state.dragged = true;
                    state.selection = Some((start.min(here), start.max(here)));
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                self.released(state, shell);
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let (Some(point), Some((_, document))) =
            (cursor.position_over(bounds), pdf::document(&self.view.path))
        else {
            return mouse::Interaction::None;
        };
        if self.link(state, &document, bounds, point).is_some() {
            mouse::Interaction::Pointer
        } else if self.locate(state, &document, bounds, point).is_some() {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        renderer.fill_quad(
            Quad {
                bounds,
                ..Quad::default()
            },
            style::color(self.faces.mode_line_inactive.background),
        );
        let Some((revision, document)) = pdf::document(&self.view.path) else {
            return;
        };
        renderer.with_layer(bounds, |renderer| {
            self.draw_sheets(renderer, state, &document, revision, bounds);
        });
    }
}

impl<Message> PdfView<'_, Message> {
    fn draw_sheets<Renderer: image::Renderer<Handle = Handle>>(
        &self,
        renderer: &mut Renderer,
        state: &State,
        document: &Arc<Document>,
        revision: u64,
        bounds: Rectangle,
    ) {
        let (placed, content) = self.arrange(document, state);
        let ink = self.view.dark.then(|| ink(&self.faces));
        let paper = ink.map_or(Color::WHITE, |ink| {
            Color::from_rgb8(ink.paper[0], ink.paper[1], ink.paper[2])
        });
        let texts = state
            .selection
            .and_then(|_| pdf::texts_now(&self.view.path));
        let mut overlay: Vec<(Rectangle, Color)> = Vec::new();
        for sheet in &placed {
            let shown = Self::sheet_bounds(sheet, content, bounds, state.offset);
            if shown.y > bounds.y + bounds.height || shown.y + shown.height < bounds.y {
                continue;
            }
            renderer.fill_quad(
                Quad {
                    bounds: shown,
                    border: Border {
                        color: style::color(self.faces.mode_line_border),
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..Quad::default()
                },
                paper,
            );
            let key = Key {
                path: self.view.path.clone(),
                revision,
                page: sheet.page,
                width: units::floor_to_usize((sheet.size.width * RENDER_SCALE).min(WIDEST_RENDER)),
                ink,
            };
            if let Some(handle) = drawn(&key) {
                renderer.draw_image(Image::new(handle), shown, bounds);
            }
            let areas = self.areas(
                sheet.page,
                state.selection,
                texts.as_deref().map(Vec::as_slice),
            );
            overlay.extend(areas.into_iter().map(|(rect, color)| {
                (
                    Rectangle::new(
                        Point::new(
                            shown.x + rect.left * sheet.scale,
                            shown.y + rect.top * sheet.scale,
                        ),
                        Size::new(
                            (rect.right - rect.left) * sheet.scale,
                            (rect.bottom - rect.top) * sheet.scale,
                        ),
                    ),
                    color,
                )
            }));
        }
        renderer.with_layer(bounds, |renderer| {
            for (rectangle, color) in overlay {
                renderer.fill_quad(
                    Quad {
                        bounds: rectangle,
                        ..Quad::default()
                    },
                    color,
                );
            }
            self.draw_bar(renderer, state, content, bounds);
        });
    }

    fn areas(
        &self,
        page: usize,
        selection: Option<(Place, Place)>,
        texts: Option<&[pdf::PageText]>,
    ) -> Vec<(pdf::Rect, Color)> {
        let mut areas: Vec<(pdf::Rect, Color)> = self
            .view
            .marks
            .iter()
            .filter(|mark| mark.page == page)
            .map(|mark| {
                let color = if mark.current {
                    style::color(self.faces.search_current).scale_alpha(CURRENT_ALPHA)
                } else {
                    style::color(self.faces.search_match).scale_alpha(MATCH_ALPHA)
                };
                (mark.rect, color)
            })
            .collect();
        if let (Some((from, to)), Some(texts)) = (selection, texts)
            && (from.0..=to.0).contains(&page)
            && let Some(text) = texts.get(page)
        {
            let start = if from.0 == page { from.1 } else { 0 };
            let end = if to.0 == page { to.1 } else { text.text.len() };
            let region = style::color(self.faces.region).scale_alpha(REGION_ALPHA);
            areas.extend(
                words::rects(text, start..end)
                    .into_iter()
                    .map(|rect| (rect, region)),
            );
        }
        areas
    }

    fn draw_bar<Renderer: renderer::Renderer>(
        &self,
        renderer: &mut Renderer,
        state: &State,
        content: Size,
        bounds: Rectangle,
    ) {
        if content.height <= bounds.height + 1.0 {
            return;
        }
        let length = (bounds.height * bounds.height / content.height).max(BAR * 4.0);
        let top = state.offset.y / (content.height - bounds.height) * (bounds.height - length);
        renderer.fill_quad(
            Quad {
                bounds: Rectangle::new(
                    Point::new(bounds.x + bounds.width - BAR - 2.0, bounds.y + top),
                    Size::new(BAR, length),
                ),
                border: Border {
                    radius: (BAR / 2.0).into(),
                    ..Border::default()
                },
                ..Quad::default()
            },
            style::color(self.faces.shadow).scale_alpha(0.6),
        );
    }
}

impl<'a, Message: 'a, Renderer> From<PdfView<'a, Message>> for Element<'a, Message, Theme, Renderer>
where
    Renderer: image::Renderer<Handle = Handle> + 'a,
{
    fn from(view: PdfView<'a, Message>) -> Self {
        Element::new(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LETTER: PageSize = PageSize {
        width: 612.0,
        height: 792.0,
    };
    const WIDE: PageSize = PageSize {
        width: 792.0,
        height: 612.0,
    };
    const WINDOW: Size = Size::new(644.0, 500.0);

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn zooms_fit_the_width_the_whole_page_or_a_scale_at_96_dpi() {
        assert!(close(scale(Zoom::FitWidth, LETTER, WINDOW), 1.0));
        let whole = scale(Zoom::FitPage, LETTER, WINDOW);
        assert!(close(LETTER.height * whole, WINDOW.height - 2.0 * GAP));
        assert!(close(scale(Zoom::Scale(2.0), LETTER, WINDOW), 192.0 / 72.0));
    }

    #[test]
    fn pages_stack_with_gaps_or_show_one_at_a_time() {
        let (placed, content) = sheets(&[LETTER, WIDE], None, Zoom::FitWidth, WINDOW);

        assert_eq!(placed.len(), 2);
        assert!(close(placed[0].top, GAP));
        assert!(close(placed[1].top, GAP + 792.0 + GAP));
        assert!(close(placed[1].size.height, 612.0 * 612.0 / 792.0));
        assert!(close(
            content.height,
            placed[1].top + placed[1].size.height + GAP
        ));
        assert!(close(content.width, WINDOW.width));

        let (single, _) = sheets(&[LETTER, WIDE], Some(5), Zoom::FitWidth, WINDOW);
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].page, 1);
        assert!(close(single[0].top, GAP));
        assert_eq!(
            sheets(&[], Some(0), Zoom::FitWidth, WINDOW).0,
            Vec::<Sheet>::new()
        );
    }

    #[test]
    fn wide_zooms_make_the_content_wider_than_the_window() {
        let (_, content) = sheets(&[LETTER], None, Zoom::Scale(2.0), WINDOW);

        assert!(close(content.width, 612.0 * 192.0 / 72.0 + 2.0 * MARGIN));
    }

    #[test]
    fn the_current_page_is_the_one_under_the_top_third() {
        let (placed, _) = sheets(&[LETTER; 5], None, Zoom::FitWidth, WINDOW);

        assert_eq!(page_at(&placed, 0.0, 600.0), 0);
        assert_eq!(page_at(&placed, 700.0, 600.0), 1);
        assert_eq!(page_at(&placed, 100_000.0, 600.0), 4);
        assert_eq!(page_at(&[], 0.0, 600.0), 0);
    }

    fn key(page: usize, width: usize) -> Key {
        Key {
            path: PathBuf::from("paper.pdf"),
            revision: 1,
            page,
            width,
            ink: None,
        }
    }

    fn bitmap(width: u32) -> Bitmap {
        Bitmap {
            width,
            height: 1,
            pixels: vec![255; usize::try_from(width).unwrap() * 4],
        }
    }

    #[test]
    fn pages_are_found_at_their_width_or_shown_from_another_size_meanwhile() {
        let mut cache = Cache::default();
        cache.pending.push(key(1, 100));
        assert!(cache.waiting(Path::new("paper.pdf")));

        cache.insert(key(1, 100), bitmap(100));

        assert!(!cache.waiting(Path::new("paper.pdf")));
        assert!(matches!(cache.find(&key(1, 100)), Some((true, _))));
        assert!(matches!(cache.find(&key(1, 120)), Some((false, _))));
        assert!(cache.find(&key(2, 100)).is_none());
        let dark = Key {
            ink: Some(Ink {
                paper: [0; 3],
                text: [255; 3],
            }),
            ..key(1, 100)
        };
        assert!(cache.find(&dark).is_none());
    }

    #[test]
    fn failed_pages_are_not_tried_again_and_old_pages_leave_first() {
        let mut cache = Cache {
            budget: 1000,
            ..Cache::default()
        };
        cache.pending.push(key(9, 100));
        cache.fail(key(9, 100));
        assert!(cache.knows(&key(9, 100)));
        assert!(cache.find(&key(9, 100)).is_none());

        cache.insert(key(0, 100), bitmap(100));
        cache.insert(key(1, 100), bitmap(100));
        cache.find(&key(0, 100));
        cache.insert(key(2, 100), bitmap(100));

        assert!(cache.find(&key(0, 100)).is_some());
        assert!(!cache.knows(&key(1, 100)));
        assert!(cache.knows(&key(2, 100)));
    }
}
