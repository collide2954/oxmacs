use std::borrow::Cow;
use std::mem;

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse::{self, Click, ScrollDelta, click};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Paragraph as _, Text};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::alignment::Vertical;
use iced::time::{Duration, Instant};
use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme, Vector,
    keyboard, window,
};
use oxmacs_core::display::{self, LineStyle};
use oxmacs_core::editor::{Cell, Clicks, Gutter, Pane, ScreenRow, WindowId};
use oxmacs_core::external::diagnostics::Severity;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::{Highlight, Theme as Faces};

use crate::animation::{self, Glide, Scroll};
use crate::style;
use crate::units::{floor_to_isize, floor_to_usize, signed_to_f32, to_f32, trunc_to_isize};
use crate::widgets::popup::Popup;

const WIDE_SAMPLE: &str = "MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM";
const NARROW_SAMPLE: &str = "iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiii";
const WHEEL_ROWS: f32 = 3.0;
const CONTINUATION: &str = "\\";
const HEADER_RAISE: f32 = 0.06;
const PILL_ALPHA: f32 = 0.22;
const PILL_INSET: f32 = 0.08;
const PILL_PADDING: f32 = 0.35;
const LINE_CURSOR: f32 = 0.18;
const LINE_CURSOR_UNFOCUSED: f32 = 0.07;
const LINE_CURSOR_EDGE: f32 = 3.0;
const DIMMED: f32 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewEvent {
    Resized {
        rows: usize,
        columns: usize,
    },
    FontNotMonospaced,
    Pressed {
        cell: Cell,
        clicks: Clicks,
        extend: bool,
    },
    Dragged(Cell),
    Released(Cell),
    NumberPressed {
        row: usize,
        extend: bool,
    },
    NumbersDragged(usize),
    MiddleClicked(Cell),
    Scrolled(isize),
}

#[derive(Debug, Clone, Copy)]
pub struct Motion {
    pub enabled: bool,
    pub last_input: Instant,
    pub blink_interval: Duration,
}

#[derive(Debug)]
pub struct BufferView<'a, Message> {
    pane: Pane<'a>,
    window: WindowId,
    faces: Faces,
    font: Font,
    size: Pixels,
    motion: Motion,
    on_event: fn(WindowId, ViewEvent) -> Message,
}

#[derive(Debug, Default)]
struct Metrics {
    measured_for: Option<(Font, Pixels)>,
    cell: Size,
    monospaced: bool,
    font_reported: bool,
}

#[derive(Debug, Default)]
struct State {
    metrics: Metrics,
    cursor: Option<Glide>,
    scroll: Option<Scroll>,
    frame: Option<Instant>,
    last_click: Option<Click>,
    dragging: bool,
    numbering: bool,
    wheel_carry: f32,
    modifiers: keyboard::Modifiers,
    shown: Option<BufferId>,
    pointed: bool,
}

impl State {
    fn follow(&mut self, buffer: BufferId) {
        if self.shown.replace(buffer) != Some(buffer) {
            self.cursor = None;
            self.scroll = None;
        }
    }

    fn shift(&self) -> f32 {
        match (&self.scroll, self.frame) {
            (Some(scroll), Some(frame)) => scroll.shift(frame),
            _ => 0.0,
        }
    }
}

impl<'a, Message> BufferView<'a, Message> {
    pub fn new(
        pane: Pane<'a>,
        window: WindowId,
        faces: Faces,
        font: Font,
        size: impl Into<Pixels>,
        motion: Motion,
        on_event: fn(WindowId, ViewEvent) -> Message,
    ) -> Self {
        Self {
            pane,
            window,
            faces,
            font,
            size: size.into(),
            motion,
            on_event,
        }
    }

    fn text<C>(&self, content: C, bounds: Size) -> Text<C, Font> {
        Text {
            content,
            bounds,
            size: self.size,
            line_height: text::LineHeight::default(),
            font: self.font,
            align_x: text::Alignment::Left,
            align_y: Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::None,
        }
    }

    fn measure<Renderer>(&self) -> Metrics
    where
        Renderer: text::Renderer<Font = Font>,
    {
        let bounds =
            |sample| Renderer::Paragraph::with_text(self.text(sample, Size::INFINITE)).min_bounds();
        let wide = bounds(WIDE_SAMPLE);
        let narrow = bounds(NARROW_SAMPLE);
        Metrics {
            measured_for: Some((self.font, self.size)),
            cell: Size::new(wide.width / to_f32(WIDE_SAMPLE.len()), wide.height),
            monospaced: is_monospaced(narrow.width, wide.width),
            font_reported: false,
        }
    }

    fn gutter_width(&self, cell_width: f32) -> f32 {
        self.pane.gutter().map_or(0.0, |gutter| {
            to_f32(gutter_columns(gutter.width(), gutter.has_separator())) * cell_width
        })
    }

    fn body(&self, bounds: Rectangle, cell_height: f32) -> Rectangle {
        let header = (to_f32(self.pane.header_lines().len()) * cell_height).min(bounds.height);
        Rectangle::new(
            Point::new(bounds.x, bounds.y + header),
            Size::new(bounds.width, bounds.height - header),
        )
    }

    fn draw_header<Renderer>(&self, renderer: &mut Renderer, bounds: Rectangle, cell: Size)
    where
        Renderer: text::Renderer<Font = Font>,
    {
        let lines = self.pane.header_lines();
        if lines.is_empty() {
            return;
        }
        let strip = Rectangle::new(
            bounds.position(),
            Size::new(bounds.width, to_f32(lines.len()) * cell.height),
        );
        let surface = self
            .faces
            .default
            .background
            .mix(self.faces.default.foreground, HEADER_RAISE);
        renderer.fill_quad(
            Quad {
                bounds: strip,
                ..Quad::default()
            },
            style::color(surface),
        );
        let foreground = style::color(self.faces.default.foreground);
        let tab_width = self.pane.tab_width();
        let left = self.text_bounds(bounds, cell.width).x;
        for (row, (text, line_style)) in lines.iter().enumerate() {
            let y = strip.y + to_f32(row) * cell.height;
            for segment in display::segments(text, &line_style.spans, tab_width) {
                let color = segment
                    .face
                    .map_or(foreground, |face| style::color(self.faces.highlight(face)));
                renderer.fill_text(
                    self.text(segment.text, Size::new(f32::INFINITY, cell.height)),
                    Point::new(left + to_f32(segment.column) * cell.width, y),
                    color,
                    strip,
                );
            }
        }
    }

    fn text_bounds(&self, bounds: Rectangle, cell_width: f32) -> Rectangle {
        let gutter = self.gutter_width(cell_width).min(bounds.width);
        Rectangle::new(
            Point::new(bounds.x + gutter, bounds.y),
            Size::new(bounds.width - gutter, bounds.height),
        )
    }

    fn draw_gutter<Renderer>(
        &self,
        renderer: &mut Renderer,
        gutter: Gutter,
        rows: &[(ScreenRow, Rectangle)],
        bounds: Rectangle,
        cell_width: f32,
    ) where
        Renderer: text::Renderer<Font = Font>,
    {
        for (row, rectangle) in rows.iter().filter(|(row, _)| row.column == 0) {
            let face = if gutter.is_current(row.line) {
                self.faces.default.foreground
            } else {
                self.faces.shadow
            };
            renderer.fill_text(
                self.text(
                    gutter.label(row.line),
                    Size::new(f32::INFINITY, rectangle.height),
                ),
                Point::new(bounds.x, rectangle.y),
                style::color(face),
                bounds,
            );
        }
        if !gutter.has_separator() {
            return;
        }
        let x = (bounds.x + to_f32(gutter.width() + 1) * cell_width).round();
        renderer.fill_quad(
            Quad {
                bounds: Rectangle::new(Point::new(x, bounds.y), Size::new(1.0, bounds.height)),
                ..Quad::default()
            },
            style::color(self.faces.mode_line_border),
        );
    }

    fn cursor_cell(&self) -> (usize, usize) {
        let (column, row) = self.pane.cursor_screen();
        (column, usize::try_from(row).unwrap_or(0))
    }

    fn focused(&self) -> bool {
        self.pane.is_selected() && !self.pane.minibuffer_active()
    }

    fn blinking(&self) -> bool {
        self.motion.enabled && self.focused()
    }

    fn animate<M>(&self, state: &mut State, now: Instant, shell: &mut Shell<'_, M>) {
        state.frame = Some(now);
        state.follow(self.pane.buffer().id());
        let viewport = self.pane.viewport();
        let (top, rows) = (viewport.top(), viewport.rows());
        let scroll = state.scroll.get_or_insert_with(|| Scroll::new(top));
        let moved = self.pane.screen_distance(scroll.target(), top, rows);
        let scrolled = scroll.retarget(top, moved, rows, now, self.motion.enabled);
        let scrolling = scroll.is_animating(now);
        let pointed = mem::take(&mut state.pointed);
        let target = self.cursor_cell();
        let cursor = state.cursor.get_or_insert_with(|| Glide::new(target));
        cursor.retarget(target, now, self.motion.enabled && !scrolled && !pointed);
        if cursor.is_animating(now) || scrolling || self.pane.flashing(now) {
            shell.request_redraw();
            return;
        }
        let idle = now.saturating_duration_since(self.motion.last_input);
        if self.blinking()
            && let Some(wait) = animation::next_blink_frame(idle, self.motion.blink_interval)
        {
            shell.request_redraw_at(now + wait);
        }
    }

    fn handle_mouse(
        &self,
        state: &mut State,
        event: mouse::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
        shell: &mut Shell<'_, Message>,
    ) {
        let cell_size = state.metrics.cell;
        let top = -state.shift();
        let gutter = self.gutter_width(cell_size.width);
        let view_event = match event {
            mouse::Event::ButtonPressed(mouse::Button::Left)
                if cursor
                    .position_in(bounds)
                    .is_some_and(|position| position.x < gutter) =>
            {
                let Some(position) = cursor.position_in(bounds) else {
                    return;
                };
                state.dragging = true;
                state.numbering = true;
                ViewEvent::NumberPressed {
                    row: cell_at(position, cell_size, top).row,
                    extend: state.modifiers.shift(),
                }
            }
            mouse::Event::ButtonPressed(button) => {
                let Some(position) = cursor.position_in(bounds) else {
                    return;
                };
                let position = Point::new(position.x - gutter, position.y);
                let cell = cell_at(position, cell_size, top);
                match button {
                    mouse::Button::Left => {
                        let click = Click::new(position, button, state.last_click);
                        state.last_click = Some(click);
                        state.dragging = true;
                        ViewEvent::Pressed {
                            cell,
                            clicks: clicks(click.kind()),
                            extend: state.modifiers.shift(),
                        }
                    }
                    mouse::Button::Middle => ViewEvent::MiddleClicked(cell),
                    _ => return,
                }
            }
            mouse::Event::CursorMoved { .. } if state.dragging => {
                let Some(position) = cursor.position_from(bounds.position()) else {
                    return;
                };
                if state.numbering {
                    ViewEvent::NumbersDragged(cell_at(position, cell_size, top).row)
                } else {
                    let position = Point::new(position.x - gutter, position.y);
                    ViewEvent::Dragged(cell_at(position, cell_size, top))
                }
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                let pressed = state.dragging && !state.numbering;
                state.dragging = false;
                state.numbering = false;
                let Some(position) = cursor.position_from(bounds.position()).filter(|_| pressed)
                else {
                    return;
                };
                let position = Point::new(position.x - gutter, position.y);
                ViewEvent::Released(cell_at(position, cell_size, top))
            }
            mouse::Event::WheelScrolled { delta } if cursor.is_over(bounds) => {
                let (rows, carry) = wheel_rows(delta, cell_size.height, state.wheel_carry);
                state.wheel_carry = carry;
                if rows == 0 {
                    return;
                }
                ViewEvent::Scrolled(rows)
            }
            _ => return,
        };
        state.pointed = !matches!(view_event, ViewEvent::Scrolled(_));
        shell.publish((self.on_event)(self.window, view_event));
        shell.capture_event();
    }

    fn draw_line_cursor<Renderer>(&self, renderer: &mut Renderer, row: Rectangle, bounds: Rectangle)
    where
        Renderer: text::Renderer<Font = Font>,
    {
        let color = style::color(self.faces.cursor);
        let alpha = if self.focused() {
            LINE_CURSOR
        } else {
            LINE_CURSOR_UNFOCUSED
        };
        renderer.fill_quad(
            Quad {
                bounds: Rectangle::new(
                    Point::new(bounds.x, row.y),
                    Size::new(bounds.width, row.height),
                ),
                ..Quad::default()
            },
            color.scale_alpha(alpha),
        );
        if self.focused() {
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle::new(
                        Point::new(bounds.x, row.y),
                        Size::new(LINE_CURSOR_EDGE, row.height),
                    ),
                    ..Quad::default()
                },
                color,
            );
        }
    }

    fn cursor_block(&self, state: &State, bounds: Rectangle) -> Option<CursorBlock> {
        let cell = state.metrics.cell;
        let target = self.cursor_cell();
        let (column, row, gliding) = match (&state.cursor, state.frame) {
            (Some(glide), Some(frame)) => {
                let (column, row) = glide.position(frame);
                (column, row, glide.is_animating(frame))
            }
            _ => (to_f32(target.0), to_f32(target.1), false),
        };
        let origin = Point::new(
            bounds.x + column * cell.width,
            bounds.y + (row + state.shift()) * cell.height,
        );
        if !within(origin.y, cell.height, bounds) {
            return None;
        }
        let (glyph, width) = self.pane.cursor_glyph();
        Some(CursorBlock {
            bounds: Rectangle::new(origin, Size::new(cell.width * to_f32(width), cell.height)),
            glyph,
            gliding,
        })
    }

    fn cursor_opacity(&self, state: &State) -> f32 {
        match state.frame {
            Some(frame) if self.blinking() => animation::cursor_opacity(
                frame.saturating_duration_since(self.motion.last_input),
                self.motion.blink_interval,
            ),
            _ => 1.0,
        }
    }

    fn draw_cursor<Renderer>(
        &self,
        renderer: &mut Renderer,
        cursor: CursorBlock,
        opacity: f32,
        bounds: Rectangle,
    ) where
        Renderer: text::Renderer<Font = Font>,
    {
        let color = style::color(self.faces.cursor);
        renderer.with_layer(bounds, |renderer| {
            if !self.focused() {
                let hollow = Quad {
                    bounds: cursor.bounds,
                    border: Border {
                        color,
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..Quad::default()
                };
                renderer.fill_quad(hollow, Color::TRANSPARENT);
                return;
            }
            renderer.fill_quad(
                Quad {
                    bounds: cursor.bounds,
                    ..Quad::default()
                },
                color.scale_alpha(opacity),
            );
            renderer.fill_text(
                self.text(cursor.glyph, Size::new(f32::INFINITY, cursor.bounds.height)),
                cursor.bounds.position(),
                style::color(self.faces.default.background).scale_alpha(opacity),
                cursor.bounds,
            );
        });
    }
}

struct CursorBlock {
    bounds: Rectangle,
    glyph: String,
    gliding: bool,
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for BufferView<'_, Message>
where
    Renderer: text::Renderer<Font = Font>,
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
        let state = tree.state.downcast_mut::<State>();
        if state.metrics.measured_for != Some((self.font, self.size)) {
            state.metrics = self.measure::<Renderer>();
        }
        layout::Node::new(limits.resolve(Length::Fill, Length::Fill, Size::ZERO))
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
        match event {
            Event::Window(window::Event::RedrawRequested(now)) => self.animate(state, *now, shell),
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            Event::Mouse(event) => {
                let body = self.body(layout.bounds(), state.metrics.cell.height);
                self.handle_mouse(state, *event, body, cursor, shell);
            }
            _ => {}
        }
        let metrics = &mut state.metrics;
        if metrics.measured_for.is_some() && !metrics.monospaced && !metrics.font_reported {
            metrics.font_reported = true;
            shell.publish((self.on_event)(self.window, ViewEvent::FontNotMonospaced));
        }
        let body = self.body(layout.bounds(), metrics.cell.height);
        let rows = rows_in(body.height, metrics.cell.height);
        let text = self.text_bounds(body, metrics.cell.width);
        let columns = columns_in(text.width, metrics.cell.width);
        let viewport = self.pane.viewport();
        if (rows, columns) != (viewport.rows(), viewport.columns()) {
            shell.publish((self.on_event)(
                self.window,
                ViewEvent::Resized { rows, columns },
            ));
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
        let cell = tree.state.downcast_ref::<State>().metrics.cell;
        let body = self.body(layout.bounds(), cell.height);
        if cursor.is_over(self.text_bounds(body, cell.width)) {
            mouse::Interaction::Text
        } else if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Idle
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
        let cell = state.metrics.cell;
        self.draw_header(renderer, layout.bounds(), cell);
        let bounds = self.body(layout.bounds(), cell.height);
        let text_bounds = self.text_bounds(bounds, cell.width);
        let (lines, rows) = self.lay_out_rows(state, text_bounds);
        let line_of = |number: usize| lines.iter().find(|line| line.number == number);
        let line_cursor = self.pane.line_cursor();
        let cursor = self
            .cursor_block(state, text_bounds)
            .filter(|_| !line_cursor);
        let opacity = self.cursor_opacity(state);
        let behind = self.focused();
        let point_line = self.pane.buffer().line_of(self.pane.point());
        renderer.with_layer(bounds, |renderer| {
            if let Some(gutter) = self.pane.gutter() {
                self.draw_gutter(renderer, gutter, &rows, bounds, cell.width);
            }
            for (row, rectangle) in &rows {
                let Some(line) = line_of(row.line) else {
                    continue;
                };
                if let Some(background) = line.style.background {
                    renderer.fill_quad(
                        Quad {
                            bounds: *rectangle,
                            ..Quad::default()
                        },
                        style::color(self.faces.highlight(background)),
                    );
                }
                if line_cursor && row.line == point_line {
                    self.draw_line_cursor(renderer, *rectangle, bounds);
                }
                let selection = line
                    .highlight
                    .map(|columns| (columns, style::color(self.faces.region)));
                let marks = line
                    .marks
                    .iter()
                    .map(|(columns, mark)| (*columns, style::color(self.faces.highlight(*mark))));
                for (columns, pill) in &line.pills {
                    if let Some(columns) =
                        display::row_highlight(*columns, row.column, row.end_column)
                    {
                        let color =
                            style::color(self.faces.highlight(*pill)).scale_alpha(PILL_ALPHA);
                        draw_pill(renderer, columns, *rectangle, cell.width, color);
                    }
                }
                let flashes = line.flashes.iter().map(|(columns, face, strength)| {
                    let color = style::color(self.faces.highlight(*face)).scale_alpha(*strength);
                    (*columns, color)
                });
                for (columns, color) in selection.into_iter().chain(marks).chain(flashes) {
                    if let Some(columns) =
                        display::row_highlight(columns, row.column, row.end_column)
                    {
                        draw_highlight(renderer, columns, *rectangle, cell.width, color);
                    }
                }
            }
            if let Some(cursor) = cursor.as_ref().filter(|cursor| cursor.gliding && behind) {
                renderer.fill_quad(
                    Quad {
                        bounds: cursor.bounds,
                        ..Quad::default()
                    },
                    style::color(self.faces.cursor).scale_alpha(opacity),
                );
            }
            for (row, rectangle) in &rows {
                if let Some(line) = line_of(row.line) {
                    self.draw_row_text(renderer, line, row, *rectangle, cell.width, text_bounds);
                    for (columns, severity) in &line.diagnostics {
                        if let Some(columns) =
                            display::row_highlight(*columns, row.column, row.end_column)
                        {
                            let color = match severity {
                                Severity::Error => self.faces.error,
                                Severity::Warning => self.faces.warning,
                            };
                            draw_wave(
                                renderer,
                                columns,
                                *rectangle,
                                cell.width,
                                style::color(color),
                            );
                        }
                    }
                }
            }
        });
        if let Some(cursor) = cursor.filter(|cursor| !cursor.gliding || !behind) {
            self.draw_cursor(renderer, cursor, opacity, text_bounds);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let view = self.pane.popup()?;
        let state = tree.state.downcast_ref::<State>();
        let cell = state.metrics.cell;
        let text = self.text_bounds(self.body(layout.bounds(), cell.height), cell.width);
        let anchor = Point::new(
            text.x + to_f32(view.column) * cell.width,
            text.y + (signed_to_f32(view.row) + state.shift()) * cell.height,
        ) + translation;
        Some(overlay::Element::new(Box::new(Popup::new(
            view, anchor, cell, self.faces, self.font, self.size,
        ))))
    }
}

struct DrawnLine<'a> {
    number: usize,
    text: Cow<'a, str>,
    style: LineStyle,
    highlight: Option<(usize, Option<usize>)>,
    marks: Vec<((usize, Option<usize>), Highlight)>,
    pills: Vec<((usize, Option<usize>), Highlight)>,
    flashes: Vec<((usize, Option<usize>), Highlight, f32)>,
    diagnostics: Vec<((usize, Option<usize>), Severity)>,
    note: Option<(String, Highlight)>,
    dimmed: bool,
}

impl<'a, Message> BufferView<'a, Message> {
    fn lay_out_rows(
        &self,
        state: &State,
        bounds: Rectangle,
    ) -> (Vec<DrawnLine<'a>>, Vec<(ScreenRow, Rectangle)>) {
        let buffer = self.pane.buffer();
        let tab_width = self.pane.tab_width();
        let cell = state.metrics.cell;
        let shift = state.shift();
        let now = state.frame.unwrap_or_else(Instant::now);
        let selection = buffer.selection();
        let focus = self.pane.focus_lines();
        let mut lines: Vec<DrawnLine<'a>> = Vec::new();
        let mut rows = Vec::new();
        for row in self
            .pane
            .screen_rows(floor_to_isize(-shift), self.pane.viewport().rows() + 2)
        {
            let y = bounds.y + (signed_to_f32(row.offset) + shift) * cell.height;
            if !within(y, cell.height, bounds) {
                continue;
            }
            if lines.last().is_none_or(|line| line.number != row.line) {
                let text = buffer.line_text(row.line);
                let span = buffer.line_start(row.line)..buffer.line_end(row.line);
                let marks = self
                    .pane
                    .line_marks(row.line)
                    .into_iter()
                    .filter_map(|(range, mark)| {
                        let columns =
                            display::highlight_columns(&text, 0..span.len(), &range, tab_width)?;
                        Some((columns, mark))
                    })
                    .collect();
                let pills = self
                    .pane
                    .line_pills(row.line)
                    .into_iter()
                    .filter_map(|(range, pill)| {
                        let columns =
                            display::highlight_columns(&text, 0..span.len(), &range, tab_width)?;
                        Some((columns, pill))
                    })
                    .collect();
                let decorations = self.pane.line_decorations(row.line, now);
                let flashes = decorations
                    .shades
                    .into_iter()
                    .filter_map(|(range, face, strength)| {
                        let columns =
                            display::highlight_columns(&text, 0..span.len(), &range, tab_width)?;
                        Some((columns, face, strength))
                    })
                    .collect();
                let diagnostics = decorations
                    .waves
                    .into_iter()
                    .map(|(range, severity)| {
                        let from = display::display_column(&text, range.start, tab_width);
                        let to = display::display_column(&text, range.end, tab_width);
                        ((from, Some(to.max(from + 1))), severity)
                    })
                    .collect();
                lines.push(DrawnLine {
                    number: row.line,
                    diagnostics,
                    note: decorations.note,
                    highlight: selection.as_ref().and_then(|selection| {
                        display::highlight_columns(&text, span, selection, tab_width)
                    }),
                    marks,
                    pills,
                    flashes,
                    style: self.pane.line_style(row.line),
                    text,
                    dimmed: focus
                        .as_ref()
                        .is_some_and(|lines| !lines.contains(&row.line)),
                });
            }
            let rectangle = Rectangle::new(
                Point::new(bounds.x, y),
                Size::new(bounds.width, cell.height),
            );
            rows.push((row, rectangle));
        }
        (lines, rows)
    }

    fn draw_row_text<Renderer>(
        &self,
        renderer: &mut Renderer,
        line: &DrawnLine<'_>,
        row: &ScreenRow,
        rectangle: Rectangle,
        cell_width: f32,
        bounds: Rectangle,
    ) where
        Renderer: text::Renderer<Font = Font>,
    {
        let foreground = style::color(self.faces.default.foreground);
        let tab_width = self.pane.tab_width();
        let fade = if line.dimmed { DIMMED } else { 1.0 };
        for segment in
            display::segments_in(&line.text, row.chars.clone(), &line.style.spans, tab_width)
        {
            let color = segment
                .face
                .map_or(foreground, |face| style::color(self.faces.highlight(face)))
                .scale_alpha(fade);
            let x = to_f32(segment.column - row.column) * cell_width;
            renderer.fill_text(
                self.text(segment.text, Size::new(f32::INFINITY, rectangle.height)),
                Point::new(rectangle.x + x, rectangle.y),
                color,
                bounds,
            );
        }
        if row.end_column.is_none()
            && let Some((note, face)) = &line.note
        {
            let end = display::display_column(&line.text, line.text.chars().count(), tab_width);
            let x = to_f32(end.saturating_sub(row.column) + 1) * cell_width;
            renderer.fill_text(
                self.text(note.clone(), Size::new(f32::INFINITY, rectangle.height)),
                Point::new(rectangle.x + x, rectangle.y),
                style::color(self.faces.highlight(*face)),
                bounds,
            );
        }
        let columns = self.pane.viewport().columns();
        if row.end_column.is_some() && columns > 0 && !self.pane.buffer().word_wrap() {
            let x = to_f32(columns - 1) * cell_width;
            renderer.fill_text(
                self.text(CONTINUATION.to_owned(), rectangle.size()),
                Point::new(rectangle.x + x, rectangle.y),
                style::color(self.faces.shadow),
                bounds,
            );
        }
    }
}

impl<'a, Message, Renderer> From<BufferView<'a, Message>> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: text::Renderer<Font = Font> + 'a,
{
    fn from(view: BufferView<'a, Message>) -> Self {
        Element::new(view)
    }
}

fn draw_highlight<Renderer>(
    renderer: &mut Renderer,
    (start, end): (usize, Option<usize>),
    row: Rectangle,
    cell_width: f32,
    color: Color,
) where
    Renderer: text::Renderer<Font = Font>,
{
    let left = row.x + to_f32(start) * cell_width;
    let right = end.map_or(row.x + row.width, |end| row.x + to_f32(end) * cell_width);
    renderer.fill_quad(
        Quad {
            bounds: Rectangle::new(Point::new(left, row.y), Size::new(right - left, row.height)),
            ..Quad::default()
        },
        color,
    );
}

fn draw_pill<Renderer>(
    renderer: &mut Renderer,
    (start, end): (usize, Option<usize>),
    row: Rectangle,
    cell_width: f32,
    color: Color,
) where
    Renderer: text::Renderer<Font = Font>,
{
    let inset = row.height * PILL_INSET;
    let left = row.x + to_f32(start) * cell_width - cell_width * PILL_PADDING;
    let right = end.map_or(row.x + row.width, |end| row.x + to_f32(end) * cell_width)
        + cell_width * PILL_PADDING;
    let height = row.height - 2.0 * inset;
    renderer.fill_quad(
        Quad {
            bounds: Rectangle::new(
                Point::new(left, row.y + inset),
                Size::new(right - left, height),
            ),
            border: Border {
                radius: (height / 2.0).into(),
                ..Border::default()
            },
            ..Quad::default()
        },
        color,
    );
}

fn draw_wave<Renderer>(
    renderer: &mut Renderer,
    (start, end): (usize, Option<usize>),
    row: Rectangle,
    cell_width: f32,
    color: Color,
) where
    Renderer: text::Renderer<Font = Font>,
{
    let left = (row.x + to_f32(start) * cell_width).round();
    let right = end.map_or(row.x + row.width, |end| row.x + to_f32(end) * cell_width);
    let base = (row.y + row.height).round() - 1.0;
    for (step, x) in (0..).map(|step| (step, left + to_f32(step))) {
        if x >= right {
            break;
        }
        renderer.fill_quad(
            Quad {
                bounds: Rectangle::new(Point::new(x, base - wave_rise(step)), Size::new(1.0, 1.0)),
                ..Quad::default()
            },
            color,
        );
    }
}

pub fn wave_rise(step: usize) -> f32 {
    match step % 4 {
        1 | 3 => 1.0,
        2 => 2.0,
        _ => 0.0,
    }
}

pub fn cell_at(position: Point, cell: Size, top: f32) -> Cell {
    Cell {
        row: floor_to_usize(position.y / cell.height + top),
        column: floor_to_usize(position.x / cell.width + 0.5),
    }
}

pub fn wheel_rows(delta: ScrollDelta, cell_height: f32, carry: f32) -> (isize, f32) {
    let rows = match delta {
        ScrollDelta::Lines { y, .. } => -y * WHEEL_ROWS,
        ScrollDelta::Pixels { y, .. } => -y / cell_height,
    };
    let total = carry + rows;
    let whole = total.trunc();
    (trunc_to_isize(whole), total - whole)
}

fn clicks(kind: click::Kind) -> Clicks {
    match kind {
        click::Kind::Single => Clicks::Single,
        click::Kind::Double => Clicks::Double,
        click::Kind::Triple => Clicks::Triple,
    }
}

fn within(y: f32, height: f32, bounds: Rectangle) -> bool {
    const SLACK: f32 = 0.5;
    y >= bounds.y - SLACK && y + height <= bounds.y + bounds.height + SLACK
}

pub fn is_monospaced(narrow_width: f32, wide_width: f32) -> bool {
    (narrow_width - wide_width).abs() <= wide_width * 0.01
}

pub fn gutter_columns(digits: usize, separator: bool) -> usize {
    digits + if separator { 2 } else { 1 }
}

pub fn columns_in(width: f32, cell_width: f32) -> usize {
    if cell_width > 0.0 {
        floor_to_usize(width / cell_width)
    } else {
        0
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is finite, non-negative and floored before the cast"
)]
pub fn rows_in(height: f32, line_height: f32) -> usize {
    let rows = (height / line_height).floor();
    if rows.is_finite() && rows >= 1.0 {
        rows as usize
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::{Lasts, Look};
    use oxmacs_core::text::Buffer;

    use super::*;

    #[test]
    fn rows_fit_whole_lines_into_the_height() {
        assert_eq!(rows_in(100.0, 20.0), 5);
        assert_eq!(rows_in(99.0, 20.0), 4);
    }

    #[test]
    fn equal_narrow_and_wide_widths_mean_monospaced() {
        assert!(is_monospaced(160.0, 160.0));
        assert!(is_monospaced(159.9, 160.0));
    }

    #[test]
    fn differing_narrow_and_wide_widths_mean_proportional() {
        assert!(!is_monospaced(64.0, 160.0));
        assert!(!is_monospaced(f32::NAN, 160.0));
    }

    const CELL: Size = Size {
        width: 10.0,
        height: 20.0,
    };

    #[test]
    fn positions_map_to_the_nearest_column_boundary_and_row() {
        assert_eq!(
            cell_at(Point::new(24.0, 45.0), CELL, 0.0),
            Cell { row: 2, column: 2 }
        );
        assert_eq!(
            cell_at(Point::new(26.0, 45.0), CELL, 0.0),
            Cell { row: 2, column: 3 }
        );
    }

    #[test]
    fn positions_account_for_the_scrolled_top() {
        assert_eq!(cell_at(Point::new(0.0, 45.0), CELL, 5.5).row, 7);
    }

    #[test]
    fn positions_left_of_or_above_the_view_clamp_to_zero() {
        assert_eq!(
            cell_at(Point::new(-30.0, -40.0), CELL, 0.0),
            Cell { row: 0, column: 0 }
        );
    }

    #[test]
    fn wheel_notches_scroll_three_rows() {
        assert_eq!(
            wheel_rows(ScrollDelta::Lines { x: 0.0, y: 1.0 }, 20.0, 0.0),
            (-3, 0.0)
        );
        assert_eq!(
            wheel_rows(ScrollDelta::Lines { x: 0.0, y: -1.0 }, 20.0, 0.0),
            (3, 0.0)
        );
    }

    #[test]
    fn pixel_scrolling_carries_partial_rows() {
        let (rows, carry) = wheel_rows(ScrollDelta::Pixels { x: 0.0, y: -30.0 }, 20.0, 0.0);
        assert_eq!(rows, 1);
        assert!((carry - 0.5).abs() < f32::EPSILON);

        let (rows, carry) = wheel_rows(ScrollDelta::Pixels { x: 0.0, y: -10.0 }, 20.0, carry);
        assert_eq!(rows, 1);
        assert!(carry.abs() < f32::EPSILON);
    }

    #[test]
    fn showing_another_buffer_drops_running_animations() {
        let first = Buffer::new("a").id();
        let mut state = State::default();
        state.follow(first);
        state.cursor = Some(Glide::new((3, 4)));
        state.scroll = Some(Scroll::new(display::ScreenLine { line: 10, row: 0 }));

        state.follow(first);
        assert!(state.cursor.is_some() && state.scroll.is_some());

        state.follow(Buffer::new("b").id());
        assert!(state.cursor.is_none() && state.scroll.is_none());
    }

    #[test]
    fn rows_are_never_zero() {
        assert_eq!(rows_in(10.0, 20.0), 1);
        assert_eq!(rows_in(100.0, 0.0), 1);
        assert_eq!(rows_in(f32::NAN, 20.0), 1);
    }

    #[test]
    fn waves_rise_and_fall_every_four_pixels() {
        let rises: Vec<f32> = (0..8).map(wave_rise).collect();

        assert_eq!(rises, [0.0, 1.0, 2.0, 1.0, 0.0, 1.0, 2.0, 1.0]);
    }

    fn ignored(_: WindowId, _: ViewEvent) {}

    #[test]
    fn flashes_are_laid_out_in_display_columns_with_their_strength() {
        let mut buffer = Buffer::with_text("t", "\tab\ncd\n");
        buffer.set_point(0);
        let mut editor = oxmacs_core::editor::Editor::with_buffer(buffer);
        let now = Instant::now();
        let id = editor.buffer().id();
        editor.annotate(
            id,
            1..5,
            Look::Shade(Highlight::Removed),
            Lasts::Fade(now, Duration::from_millis(500)),
            "zap",
        );
        let view = BufferView::new(
            editor.pane(),
            editor.selected_window(),
            oxmacs_core::theme::ADWAITA,
            Font::MONOSPACE,
            16.0,
            Motion {
                enabled: true,
                last_input: now,
                blink_interval: Duration::ZERO,
            },
            ignored,
        );
        let state = State {
            metrics: Metrics {
                cell: CELL,
                ..Metrics::default()
            },
            frame: Some(now),
            ..State::default()
        };

        let (lines, _) = view.lay_out_rows(
            &state,
            Rectangle::new(Point::ORIGIN, Size::new(400.0, 200.0)),
        );

        assert_eq!(lines[0].flashes, [((8, Some(10)), Highlight::Removed, 1.0)]);
        assert_eq!(lines[1].flashes, [((0, Some(1)), Highlight::Removed, 1.0)]);
        assert!(view.pane.flashing(now));
        assert!(!view.pane.flashing(now + Duration::from_millis(500)));
    }

    #[test]
    fn only_rows_inside_the_window_are_laid_out_since_some_renderers_do_not_clip_text() {
        let lines: Vec<String> = (0..50).map(|number| format!("line {number}")).collect();
        let text = lines.join("\n") + "\n";
        let mut buffer = Buffer::with_text("t", &text);
        buffer.set_point(0);
        let mut editor = oxmacs_core::editor::Editor::with_buffer(buffer);
        let window = editor.selected_window();
        editor.resize_pane(window, 5, 40);
        let view = BufferView::new(
            editor.pane(),
            window,
            oxmacs_core::theme::ADWAITA,
            Font::MONOSPACE,
            16.0,
            Motion {
                enabled: false,
                last_input: Instant::now(),
                blink_interval: Duration::ZERO,
            },
            ignored,
        );
        let state = State {
            metrics: Metrics {
                cell: CELL,
                ..Metrics::default()
            },
            ..State::default()
        };
        let body = Rectangle::new(Point::new(0.0, 30.0), Size::new(400.0, 5.0 * CELL.height));

        let (_, rows) = view.lay_out_rows(&state, body);

        assert_eq!(rows.len(), 5);
        assert!(
            rows.iter()
                .all(|(_, row)| row.y >= body.y && row.y + row.height <= body.y + body.height),
            "{rows:?}"
        );
    }

    #[test]
    fn the_gutter_leaves_a_column_after_the_numbers_and_each_side_of_a_separator() {
        assert_eq!(gutter_columns(2, false), 3);
        assert_eq!(gutter_columns(5, false), 6);
        assert_eq!(gutter_columns(2, true), 4);
    }
}
