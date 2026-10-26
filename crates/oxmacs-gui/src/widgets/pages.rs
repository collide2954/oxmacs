use std::sync::Arc;

use iced::advanced::image::{self, Image};
use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Span, Text};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::alignment::Vertical;
use iced::font::{Family, Style, Weight};
use iced::widget::image::Handle;
use iced::{Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme};
use oxmacs_core::theme::{Highlight, Theme as Faces};
use oxmacs_ox::book::{At, Block, Kind, Pages, Run, Spread, Typeface};
use oxmacs_ox::views::{Arg, Press};

use crate::style;
use crate::widgets::ox_view;

const MEASURE: f32 = 32.0;
const LINE: f32 = 1.55;
const TOP: f32 = 3.2;
const BOTTOM: f32 = 3.2;
const SIDE: f32 = 2.0;
const GUTTER: f32 = 4.0;
const EPSILON: f32 = 0.5;
const SMALL: f32 = 0.72;
const MARK_ALPHA: f32 = 0.45;
const SELECTION_ALPHA: f32 = 0.6;
const TALLEST_IMAGE: f32 = 0.92;
const INDENT: &str = "\u{2003}";

pub type Anchor = (usize, usize);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    pub y: f32,
    pub height: f32,
    pub line: Option<f32>,
    pub keep_with_next: bool,
}

pub fn paginate(pieces: &[Piece], page: f32) -> Vec<f32> {
    let mut starts = vec![0.0];
    let mut start = 0.0;
    for (index, piece) in pieces.iter().enumerate() {
        let mut top = piece.y;
        let bottom = piece.y + piece.height;
        while bottom > start + page + EPSILON {
            let room = start + page - top;
            let mut next = match piece.line {
                Some(line) if room >= line => top + (room / line + EPSILON / line).floor() * line,
                _ => top,
            };
            if next <= start + EPSILON {
                next = match piece.line {
                    Some(line) => top + (page / line).floor().max(1.0) * line,
                    None => top + page,
                };
            }
            if (next - piece.y).abs() < EPSILON
                && let Some(before) = index.checked_sub(1).map(|at| pieces[at])
                && before.keep_with_next
                && before.y > start + EPSILON
            {
                next = before.y;
            }
            starts.push(next);
            start = next;
            top = next;
        }
    }
    starts
}

pub fn page_of(starts: &[f32], flow: f32) -> usize {
    starts
        .partition_point(|start| *start <= flow + EPSILON)
        .saturating_sub(1)
}

fn byte_of(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(byte, _)| byte)
}

fn chars_of(text: &str, byte: usize) -> usize {
    text.get(..byte.min(text.len()))
        .map_or(0, |before| before.chars().count())
}

struct Shaped<P> {
    paragraph: P,
    text: String,
    prefix: usize,
    links: Vec<(std::ops::Range<usize>, String)>,
    line: f32,
}

enum Look<P> {
    Text(Shaped<P>),
    Image(Handle),
    Rule,
}

struct Laid<P> {
    look: Look<P>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Key {
    document: usize,
    width: u32,
    height: u32,
    typeface: Typeface,
    size: u32,
    spread: Spread,
}

struct Typeset<P> {
    key: Key,
    page: Size,
    spread: bool,
    blocks: Vec<Laid<P>>,
    starts: Vec<f32>,
}

impl<P: text::Paragraph> Typeset<P> {
    fn block_at(&self, flow: f32) -> Option<usize> {
        let index = self
            .blocks
            .partition_point(|laid| laid.y <= flow + EPSILON)
            .checked_sub(1)?;
        Some(index)
    }

    fn anchor_at(&self, flow: f32) -> Anchor {
        let Some(index) = self.block_at(flow) else {
            return (0, 0);
        };
        let laid = &self.blocks[index];
        if flow > laid.y + laid.height + EPSILON {
            return (index + 1, 0);
        }
        match &laid.look {
            Look::Text(shaped) => {
                let point = Point::new(0.0, (flow - laid.y + shaped.line / 2.0).max(0.0));
                let byte = match shaped.paragraph.hit_test(point) {
                    Some(text::Hit::CharOffset(byte)) => byte,
                    None => 0,
                };
                (
                    index,
                    chars_of(&shaped.text, byte).saturating_sub(shaped.prefix),
                )
            }
            Look::Image(_) | Look::Rule => (index, 0),
        }
    }

    fn flow_of(&self, (block, offset): Anchor) -> f32 {
        let Some(laid) = self.blocks.get(block) else {
            return self.blocks.last().map_or(0.0, |laid| laid.y + laid.height);
        };
        match &laid.look {
            Look::Text(shaped) => {
                let byte = byte_of(&shaped.text, offset + shaped.prefix);
                shaped
                    .paragraph
                    .grapheme_position(0, byte)
                    .map_or(laid.y, |point| laid.y + point.y)
            }
            Look::Image(_) | Look::Rule => laid.y,
        }
    }

    fn anchors(&self) -> Vec<Anchor> {
        self.starts
            .iter()
            .map(|start| self.anchor_at(*start))
            .collect()
    }

    fn per_turn(&self) -> usize {
        if self.spread { 2 } else { 1 }
    }

    fn first_shown(&self, at: At) -> usize {
        let page = match at {
            At::End => self.starts.len().saturating_sub(1),
            At::Anchor(block, offset) => page_of(&self.starts, self.flow_of((block, offset))),
        };
        if self.spread { page - page % 2 } else { page }
    }

    fn band(&self, page: usize) -> Option<(f32, f32)> {
        let start = *self.starts.get(page)?;
        let end = self.starts.get(page + 1).copied().unwrap_or_else(|| {
            self.blocks
                .last()
                .map_or(start, |laid| laid.y + laid.height)
        });
        Some((start, end.max(start)))
    }
}

struct State<P> {
    typeset: Option<Typeset<P>>,
    reported: Option<(Vec<Anchor>, usize)>,
    pressed: Option<Anchor>,
    selection: Option<(Anchor, Anchor)>,
    dragged: bool,
    wheel: f32,
}

pub struct PagesView<'a, Message> {
    pages: &'a Pages,
    faces: Faces,
    mono: Font,
    on_call: Box<dyn Fn(usize, Vec<Arg>) -> Message + 'a>,
}

impl<Message> std::fmt::Debug for PagesView<'_, Message> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PagesView")
            .field("at", &self.pages.at)
            .finish_non_exhaustive()
    }
}

impl<'a, Message> PagesView<'a, Message> {
    pub fn new(
        pages: &'a Pages,
        faces: Faces,
        mono: Font,
        on_call: impl Fn(usize, Vec<Arg>) -> Message + 'a,
    ) -> Self {
        Self {
            pages,
            faces,
            mono,
            on_call: Box::new(on_call),
        }
    }

    fn base(&self) -> Font {
        match self.pages.typeface {
            Typeface::Serif => Font {
                family: Family::Serif,
                ..Font::DEFAULT
            },
            Typeface::Sans => Font {
                family: Family::SansSerif,
                ..Font::DEFAULT
            },
            Typeface::Mono => self.mono,
        }
    }

    fn call(&self, press: Option<&Press>, args: Vec<Arg>) -> Option<Message> {
        match press {
            Some(Press::Slot(slot)) => Some((self.on_call)(*slot, args)),
            _ => None,
        }
    }

    fn key(&self, bounds: Size) -> Key {
        Key {
            document: Arc::as_ptr(&self.pages.document.0).addr(),
            width: bounds.width.to_bits(),
            height: bounds.height.to_bits(),
            typeface: self.pages.typeface,
            size: self.pages.size.to_bits(),
            spread: self.pages.spread,
        }
    }

    fn geometry(&self, bounds: Size) -> (Size, bool) {
        let em = self.pages.size;
        let usable = (bounds.width - 2.0 * SIDE * em).max(em * 8.0);
        let measure = em * MEASURE;
        let double = match self.pages.spread {
            Spread::Single => false,
            Spread::Double => true,
            Spread::Auto => usable >= 2.0 * measure + GUTTER * em,
        };
        let width = if double {
            ((usable - GUTTER * em) / 2.0).min(measure)
        } else {
            usable.min(measure)
        };
        let height = (bounds.height - (TOP + BOTTOM) * em).max(em * LINE * 3.0);
        (Size::new(width, height), double)
    }

    fn spans(&self, runs: &[Run], font: Font, size: f32) -> Vec<Span<'static, (), Font>> {
        let ink = style::color(self.faces.default.foreground);
        let link = style::color(self.faces.link);
        runs.iter()
            .map(|run| {
                let mut chosen = if run.code { self.mono } else { font };
                if run.bold {
                    chosen.weight = Weight::Bold;
                }
                if run.italic {
                    chosen.style = Style::Italic;
                }
                let size = if run.code { size * 0.9 } else { size };
                Span::new(run.text.clone())
                    .font(chosen)
                    .size(size)
                    .color(if run.link.is_some() { link } else { ink })
            })
            .collect()
    }

    fn typeset<P: text::Paragraph<Font = Font>>(&self, bounds: Size) -> Typeset<P> {
        let (page, spread) = self.geometry(bounds);
        let em = self.pages.size;
        let base = self.base();
        let mut blocks = Vec::new();
        let mut y = 0.0;
        let mut previous: Option<&Block> = None;
        for block in &self.pages.document.0.blocks {
            let laid = match block {
                Block::Text { kind, runs } => {
                    let follows_body = matches!(
                        previous,
                        Some(Block::Text {
                            kind: Kind::Body,
                            ..
                        })
                    );
                    let (x, size, font, align, above, below, prefix) = match kind {
                        Kind::Body => (
                            0.0,
                            em,
                            base,
                            text::Alignment::Justified,
                            0.0,
                            0.0,
                            if follows_body {
                                INDENT.to_owned()
                            } else {
                                String::new()
                            },
                        ),
                        Kind::Heading(level) => {
                            let scale = [1.7, 1.4, 1.2, 1.1, 1.0, 1.0]
                                [usize::from(level.saturating_sub(1)).min(5)];
                            let mut bold = base;
                            bold.weight = Weight::Bold;
                            let align = if *level <= 2 {
                                text::Alignment::Center
                            } else {
                                text::Alignment::Left
                            };
                            (
                                0.0,
                                em * scale,
                                bold,
                                align,
                                em * 1.4,
                                em * 0.8,
                                String::new(),
                            )
                        }
                        Kind::Quote => (
                            em * 1.6,
                            em * 0.95,
                            base,
                            text::Alignment::Justified,
                            em * 0.5,
                            em * 0.5,
                            String::new(),
                        ),
                        Kind::Item { marker, depth } => (
                            em * 1.4 * f32::from(*depth),
                            em,
                            base,
                            text::Alignment::Left,
                            em * 0.15,
                            em * 0.15,
                            format!("{marker} "),
                        ),
                        Kind::Pre => (
                            em,
                            em * 0.85,
                            self.mono,
                            text::Alignment::Left,
                            0.0,
                            0.0,
                            String::new(),
                        ),
                        Kind::Center => (
                            0.0,
                            em,
                            base,
                            text::Alignment::Center,
                            em * 0.5,
                            em * 0.5,
                            String::new(),
                        ),
                    };
                    let after_body = follows_body && !matches!(kind, Kind::Body);
                    y += above + if after_body { em * 0.4 } else { 0.0 };
                    let inset = if matches!(kind, Kind::Quote) {
                        2.0 * x
                    } else {
                        x
                    };
                    let width = (page.width - inset).max(em * 4.0);
                    let mut spans = Vec::new();
                    if !prefix.is_empty() {
                        spans.push(
                            Span::new(prefix.clone())
                                .font(base)
                                .size(size)
                                .color(style::color(self.faces.default.foreground)),
                        );
                    }
                    spans.extend(self.spans(runs, font, size));
                    let paragraph = P::with_spans(Text {
                        content: spans.as_slice(),
                        bounds: Size::new(width, f32::INFINITY),
                        size: Pixels(size),
                        line_height: text::LineHeight::Relative(LINE),
                        font,
                        align_x: align,
                        align_y: Vertical::Top,
                        shaping: text::Shaping::Advanced,
                        wrapping: text::Wrapping::WordOrGlyph,
                    });
                    let line = size * LINE;
                    let height = paragraph.min_bounds().height.max(line);
                    let mut text = prefix.clone();
                    let mut links = Vec::new();
                    let mut at = prefix.chars().count();
                    for run in runs {
                        let length = run.text.chars().count();
                        if let Some(target) = &run.link {
                            links.push((at..at + length, target.clone()));
                        }
                        at += length;
                        text.push_str(&run.text);
                    }
                    let laid = Laid {
                        look: Look::Text(Shaped {
                            paragraph,
                            prefix: prefix.chars().count(),
                            text,
                            links,
                            line,
                        }),
                        x,
                        y,
                        width,
                        height,
                    };
                    y += height + below;
                    laid
                }
                Block::Image { picture, .. } => {
                    let (natural_width, natural_height) = picture.size();
                    let (natural_width, natural_height) = (
                        crate::units::to_f32(usize::try_from(natural_width).unwrap_or(1)),
                        crate::units::to_f32(usize::try_from(natural_height).unwrap_or(1)),
                    );
                    let scale = (page.width / natural_width)
                        .min(page.height * TALLEST_IMAGE / natural_height)
                        .min(2.0);
                    let (width, height) = (natural_width * scale, natural_height * scale);
                    y += em * 0.5;
                    let laid = Laid {
                        look: Look::Image(ox_view::handle(picture)),
                        x: (page.width - width) / 2.0,
                        y,
                        width,
                        height,
                    };
                    y += height + em * 0.5;
                    laid
                }
                Block::Rule => {
                    y += em * 0.6;
                    let laid = Laid {
                        look: Look::Rule,
                        x: 0.0,
                        y,
                        width: page.width,
                        height: em * LINE,
                    };
                    y += em * LINE + em * 0.6;
                    laid
                }
            };
            blocks.push(laid);
            previous = Some(block);
        }
        let pieces: Vec<Piece> = blocks
            .iter()
            .map(|laid| Piece {
                y: laid.y,
                height: laid.height,
                line: match &laid.look {
                    Look::Text(shaped) => Some(shaped.line),
                    _ => None,
                },
                keep_with_next: false,
            })
            .zip(&self.pages.document.0.blocks)
            .map(|(piece, block)| Piece {
                keep_with_next: matches!(
                    block,
                    Block::Text {
                        kind: Kind::Heading(_),
                        ..
                    }
                ),
                ..piece
            })
            .collect();
        let starts = paginate(&pieces, page.height);
        Typeset {
            key: self.key(bounds),
            page,
            spread,
            blocks,
            starts,
        }
    }

    fn page_origins(&self, typeset_page: Size, spread: bool, bounds: Rectangle) -> [Point; 2] {
        let em = self.pages.size;
        let total = if spread {
            typeset_page.width * 2.0 + GUTTER * em
        } else {
            typeset_page.width
        };
        let left = bounds.x + ((bounds.width - total) / 2.0).max(0.0);
        let top = bounds.y + TOP * em;
        [
            Point::new(left, top),
            Point::new(left + typeset_page.width + GUTTER * em, top),
        ]
    }
}

fn located<P: text::Paragraph>(
    typeset: &Typeset<P>,
    origins: [Point; 2],
    first: usize,
    point: Point,
) -> Option<(Anchor, Option<String>)> {
    let pages = if typeset.spread { 2 } else { 1 };
    for (side, origin) in origins.iter().take(pages).enumerate() {
        let page = first + side;
        let (start, end) = typeset.band(page)?;
        let local = Point::new(point.x - origin.x, point.y - origin.y);
        if local.x < -typeset.page.width * 0.1
            || local.x > typeset.page.width * 1.1
            || local.y < 0.0
            || local.y > end - start
        {
            continue;
        }
        let flow = start + local.y;
        let index = typeset.block_at(flow)?;
        let laid = &typeset.blocks[index];
        let Look::Text(shaped) = &laid.look else {
            return Some(((index, 0), None));
        };
        let inside = Point::new(local.x - laid.x, flow - laid.y);
        let byte = match shaped.paragraph.hit_test(inside) {
            Some(text::Hit::CharOffset(byte)) => byte,
            None => 0,
        };
        let chars = chars_of(&shaped.text, byte);
        let link = shaped.paragraph.hit_span(inside).and_then(|_| {
            shaped
                .links
                .iter()
                .find(|(range, _)| range.contains(&chars))
                .map(|(_, target)| target.clone())
        });
        return Some(((index, chars.saturating_sub(shaped.prefix)), link));
    }
    None
}

fn anchor_arg((block, offset): Anchor) -> Arg {
    Arg::List(vec![
        Arg::Int(i64::try_from(block).unwrap_or(i64::MAX)),
        Arg::Int(i64::try_from(offset).unwrap_or(i64::MAX)),
    ])
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for PagesView<'_, Message>
where
    Renderer: text::Renderer<Font = Font> + image::Renderer<Handle = Handle>,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Renderer::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<Renderer::Paragraph> {
            typeset: None,
            reported: None,
            pressed: None,
            selection: None,
            dragged: false,
            wheel: 0.0,
        })
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
        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();
        if state
            .typeset
            .as_ref()
            .is_none_or(|typeset| typeset.key != self.key(size))
        {
            state.typeset = Some(self.typeset(size));
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
        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();
        let Some(typeset) = &state.typeset else {
            return;
        };
        let report = (typeset.anchors(), typeset.per_turn());
        if state.reported.as_ref() != Some(&report) {
            let args = vec![
                Arg::List(report.0.iter().copied().map(anchor_arg).collect()),
                Arg::Int(i64::try_from(report.1).unwrap_or(1)),
            ];
            if let Some(message) = self.call(self.pages.on_pages.as_ref(), args) {
                shell.publish(message);
            }
            state.reported = Some(report);
        }
        let bounds = layout.bounds();
        let origins = self.page_origins(typeset.page, typeset.spread, bounds);
        let first = typeset.first_shown(self.pages.at);
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(point) = cursor.position_over(bounds) else {
                    return;
                };
                if let Some((anchor, link)) = located(typeset, origins, first, point) {
                    if let Some(target) = link {
                        if let Some(message) =
                            self.call(self.pages.on_link.as_ref(), vec![Arg::Text(target)])
                        {
                            shell.publish(message);
                        }
                        shell.capture_event();
                        return;
                    }
                    state.pressed = Some(anchor);
                    state.dragged = false;
                    if state.selection.take().is_some()
                        && let Some(message) =
                            self.call(self.pages.on_select.as_ref(), vec![Arg::Nil])
                    {
                        shell.publish(message);
                    }
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(start) = state.pressed
                    && let Some((anchor, _)) = located(typeset, origins, first, *position)
                    && anchor != start
                {
                    state.dragged = true;
                    state.selection = Some((start.min(anchor), start.max(anchor)));
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.pressed.take().is_some()
                    && state.dragged
                    && let Some((from, to)) = state.selection
                    && let Some(message) = self.call(
                        self.pages.on_select.as_ref(),
                        vec![anchor_arg(from), anchor_arg(to)],
                    )
                {
                    shell.publish(message);
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let lines = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => *y / 40.0,
                };
                state.wheel += lines;
                if state.wheel.abs() >= 1.0 {
                    let by = if state.wheel > 0.0 { -1 } else { 1 };
                    state.wheel = 0.0;
                    if let Some(message) =
                        self.call(self.pages.on_turn.as_ref(), vec![Arg::Int(by)])
                    {
                        shell.publish(message);
                    }
                }
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
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
        let state = tree.state.downcast_ref::<State<Renderer::Paragraph>>();
        let Some(typeset) = &state.typeset else {
            return;
        };
        let bounds = layout.bounds();
        let em = self.pages.size;
        let origins = self.page_origins(typeset.page, typeset.spread, bounds);
        let first = typeset.first_shown(self.pages.at);
        let pages = if typeset.spread { 2 } else { 1 };
        let ink = style::color(self.faces.default.foreground);
        let faint = style::color(self.faces.shadow);
        if typeset.spread {
            let x = origins[1].x - GUTTER * em / 2.0;
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle::new(
                        Point::new(x.round(), bounds.y + TOP * em * 0.6),
                        Size::new(1.0, bounds.height - (TOP + BOTTOM) * em * 0.6),
                    ),
                    ..Quad::default()
                },
                style::color(self.faces.mode_line_border).scale_alpha(0.6),
            );
        }
        let mut marks: Vec<(Anchor, Anchor, Color)> = self
            .pages
            .marks
            .iter()
            .map(|mark| {
                (
                    mark.from,
                    mark.to,
                    style::color(self.faces.highlight(mark.face)).scale_alpha(MARK_ALPHA),
                )
            })
            .collect();
        if let Some((from, to)) = state.selection {
            marks.push((
                from,
                to,
                style::color(self.faces.highlight(Highlight::Region)).scale_alpha(SELECTION_ALPHA),
            ));
        }
        for (side, origin) in origins.iter().take(pages).enumerate() {
            let page = first + side;
            let Some((start, end)) = typeset.band(page) else {
                continue;
            };
            let clip = Rectangle::new(
                Point::new(origin.x - em, origin.y),
                Size::new(typeset.page.width + 2.0 * em, end - start + 1.0),
            );
            let header = if side == 0 {
                &self.pages.header.0
            } else {
                &self.pages.header.1
            };
            let header_text = |content: String, x: f32, y: f32, align: text::Alignment| {
                (
                    Text {
                        content,
                        bounds: Size::new(typeset.page.width, em * 2.0),
                        size: Pixels(em * SMALL),
                        line_height: text::LineHeight::default(),
                        font: self.base(),
                        align_x: align,
                        align_y: Vertical::Top,
                        shaping: text::Shaping::Advanced,
                        wrapping: text::Wrapping::None,
                    },
                    Point::new(x, y),
                )
            };
            if !header.is_empty() {
                let (text, at) = header_text(
                    header.clone(),
                    origin.x,
                    bounds.y + TOP * em * 0.4,
                    if side == 0 && typeset.spread {
                        text::Alignment::Left
                    } else {
                        text::Alignment::Right
                    },
                );
                renderer.fill_text(text, at, faint, bounds);
            }
            let (number, at) = header_text(
                format!("{}", page + 1),
                origin.x,
                origin.y + typeset.page.height + BOTTOM * em * 0.35,
                text::Alignment::Center,
            );
            renderer.fill_text(number, at, faint, bounds);
            for (index, laid) in typeset.blocks.iter().enumerate() {
                if laid.y + laid.height < start - EPSILON || laid.y > end + EPSILON {
                    continue;
                }
                let top_left = Point::new(origin.x + laid.x, origin.y + laid.y - start);
                match &laid.look {
                    Look::Text(shaped) => {
                        for (from, to, color) in &marks {
                            if index < from.0 || index > to.0 {
                                continue;
                            }
                            let first_char = if index == from.0 { from.1 } else { 0 };
                            let last_char = if index == to.0 {
                                to.1
                            } else {
                                shaped.text.chars().count()
                            };
                            for rectangle in covered(shaped, first_char, last_char, laid.width) {
                                renderer.fill_quad(
                                    Quad {
                                        bounds: Rectangle::new(
                                            Point::new(
                                                top_left.x + rectangle.x,
                                                top_left.y + rectangle.y,
                                            ),
                                            rectangle.size(),
                                        ),
                                        ..Quad::default()
                                    },
                                    *color,
                                );
                            }
                        }
                        renderer.fill_paragraph(&shaped.paragraph, top_left, ink, clip);
                    }
                    Look::Image(handle) => {
                        renderer.draw_image(
                            Image::new(handle.clone()),
                            Rectangle::new(top_left, Size::new(laid.width, laid.height)),
                            clip,
                        );
                    }
                    Look::Rule => {
                        let (text, at) = header_text(
                            "⁂".to_owned(),
                            origin.x,
                            top_left.y,
                            text::Alignment::Center,
                        );
                        renderer.fill_text(text, at, faint, clip);
                    }
                }
            }
        }
    }
}

fn covered<P: text::Paragraph>(
    shaped: &Shaped<P>,
    from: usize,
    to: usize,
    width: f32,
) -> Vec<Rectangle> {
    if to <= from {
        return Vec::new();
    }
    let position = |chars: usize| {
        let byte = byte_of(&shaped.text, chars + shaped.prefix);
        shaped.paragraph.grapheme_position(0, byte)
    };
    let bounds = shaped.paragraph.min_bounds();
    let start = position(from).unwrap_or(Point::ORIGIN);
    let end = position(to)
        .unwrap_or_else(|| Point::new(bounds.width, (bounds.height - shaped.line).max(0.0)));
    let line = shaped.line;
    if (start.y - end.y).abs() < line / 2.0 {
        return vec![Rectangle::new(
            start,
            Size::new((end.x - start.x).max(0.0), line),
        )];
    }
    let mut rectangles = vec![Rectangle::new(
        start,
        Size::new((width - start.x).max(0.0), line),
    )];
    let middle = end.y - start.y - line;
    if middle > EPSILON {
        rectangles.push(Rectangle::new(
            Point::new(0.0, start.y + line),
            Size::new(width, middle),
        ));
    }
    rectangles.push(Rectangle::new(
        Point::new(0.0, end.y),
        Size::new(end.x, line),
    ));
    rectangles
}

impl<'a, Message: 'a, Renderer> From<PagesView<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: text::Renderer<Font = Font> + image::Renderer<Handle = Handle> + 'a,
{
    fn from(view: PagesView<'a, Message>) -> Self {
        Element::new(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(y: f32, lines: usize) -> Piece {
        Piece {
            y,
            height: crate::units::to_f32(lines) * 10.0,
            line: Some(10.0),
            keep_with_next: false,
        }
    }

    #[test]
    fn pages_break_between_whole_lines() {
        let starts = paginate(&[text(0.0, 3), text(30.0, 10)], 50.0);

        assert_eq!(starts, [0.0, 50.0, 100.0]);
        assert_eq!(page_of(&starts, 49.0), 0);
        assert_eq!(page_of(&starts, 50.0), 1);
        assert_eq!(page_of(&starts, 120.0), 2);
    }

    #[test]
    fn pictures_move_whole_to_the_next_page() {
        let picture = Piece {
            y: 40.0,
            height: 30.0,
            line: None,
            keep_with_next: false,
        };

        assert_eq!(paginate(&[text(0.0, 4), picture], 50.0), [0.0, 40.0]);
    }

    #[test]
    fn headings_stay_with_what_follows_them() {
        let heading = Piece {
            y: 30.0,
            height: 15.0,
            line: Some(15.0),
            keep_with_next: true,
        };

        assert_eq!(
            paginate(&[text(0.0, 3), heading, text(45.0, 3)], 50.0),
            [0.0, 30.0]
        );
    }

    #[test]
    fn characters_and_bytes_convert_both_ways() {
        let text = "a€b";
        assert_eq!(byte_of(text, 2), 4);
        assert_eq!(byte_of(text, 9), 5);
        assert_eq!(chars_of(text, 4), 2);
    }
}
