use std::borrow::Cow;
use std::mem;

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse::{self, Click, ScrollDelta, click};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Paragraph as _, Text};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::alignment::Vertical;
use iced::time::{Duration, Instant};
use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme, keyboard,
    window,
};
use mx_core::display::{self, LineStyle};
use mx_core::editor::{Cell, Clicks, Pane, ScreenRow};
use mx_core::text::BufferId;
use mx_core::theme::{Highlight, Theme as Faces};

use crate::animation::{self, Glide, Scroll};
use crate::style;
use crate::units::{floor_to_isize, floor_to_usize, signed_to_f32, to_f32, trunc_to_isize};

const WIDE_SAMPLE: &str = "MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM";
const NARROW_SAMPLE: &str = "iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiii";
const WHEEL_ROWS: f32 = 3.0;
const CONTINUATION: &str = "\\";

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
    faces: Faces,
    font: Font,
    size: Pixels,
    motion: Motion,
    on_event: fn(ViewEvent) -> Message,
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
        faces: Faces,
        font: Font,
        size: impl Into<Pixels>,
        motion: Motion,
        on_event: fn(ViewEvent) -> Message,
    ) -> Self {
        Self {
            pane,
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

    fn cursor_cell(&self) -> (usize, usize) {
        let (column, row) = self.pane.cursor_screen();
        (column, usize::try_from(row).unwrap_or(0))
    }

    fn blinking(&self) -> bool {
        self.motion.enabled && !self.pane.minibuffer_active()
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
        if cursor.is_animating(now) || scrolling {
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
        let view_event = match event {
            mouse::Event::ButtonPressed(button) => {
                let Some(position) = cursor.position_in(bounds) else {
                    return;
                };
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
                ViewEvent::Dragged(cell_at(position, cell_size, top))
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                state.dragging = false;
                return;
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
        shell.publish((self.on_event)(view_event));
        shell.capture_event();
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
        if origin.y + cell.height <= bounds.y || origin.y >= bounds.y + bounds.height {
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
            if self.pane.minibuffer_active() {
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
            Event::Mouse(event) => self.handle_mouse(state, *event, layout.bounds(), cursor, shell),
            _ => {}
        }
        let metrics = &mut state.metrics;
        if metrics.measured_for.is_some() && !metrics.monospaced && !metrics.font_reported {
            metrics.font_reported = true;
            shell.publish((self.on_event)(ViewEvent::FontNotMonospaced));
        }
        let rows = rows_in(layout.bounds().height, metrics.cell.height);
        let columns = columns_in(layout.bounds().width, metrics.cell.width);
        let viewport = self.pane.viewport();
        if (rows, columns) != (viewport.rows(), viewport.columns()) {
            shell.publish((self.on_event)(ViewEvent::Resized { rows, columns }));
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
        let state = tree.state.downcast_ref::<State>();
        let cell = state.metrics.cell;
        let bounds = layout.bounds();
        let (lines, rows) = self.lay_out_rows(state, bounds);
        let line_of = |number: usize| lines.iter().find(|line| line.number == number);
        let cursor = self.cursor_block(state, bounds);
        let opacity = self.cursor_opacity(state);
        let behind = !self.pane.minibuffer_active();
        renderer.with_layer(bounds, |renderer| {
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
                let selection = line
                    .highlight
                    .map(|columns| (columns, style::color(self.faces.region)));
                let marks = line
                    .marks
                    .iter()
                    .map(|(columns, mark)| (*columns, style::color(self.faces.highlight(*mark))));
                for (columns, color) in selection.into_iter().chain(marks) {
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
                    self.draw_row_text(renderer, line, row, *rectangle, cell.width, bounds);
                }
            }
        });
        if let Some(cursor) = cursor.filter(|cursor| !cursor.gliding || !behind) {
            self.draw_cursor(renderer, cursor, opacity, bounds);
        }
    }
}

struct DrawnLine<'a> {
    number: usize,
    text: Cow<'a, str>,
    style: LineStyle,
    highlight: Option<(usize, Option<usize>)>,
    marks: Vec<((usize, Option<usize>), Highlight)>,
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
        let selection = buffer.selection();
        let mut lines: Vec<DrawnLine<'a>> = Vec::new();
        let mut rows = Vec::new();
        for row in self
            .pane
            .screen_rows(floor_to_isize(-shift), self.pane.viewport().rows() + 2)
        {
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
                lines.push(DrawnLine {
                    number: row.line,
                    highlight: selection.as_ref().and_then(|selection| {
                        display::highlight_columns(&text, span, selection, tab_width)
                    }),
                    marks,
                    style: self.pane.line_style(row.line),
                    text,
                });
            }
            let y = bounds.y + (signed_to_f32(row.offset) + shift) * cell.height;
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
        for segment in
            display::segments_in(&line.text, row.chars.clone(), &line.style.spans, tab_width)
        {
            let color = segment
                .face
                .map_or(foreground, |face| style::color(self.faces.highlight(face)));
            let x = to_f32(segment.column - row.column) * cell_width;
            renderer.fill_text(
                self.text(segment.text, Size::new(f32::INFINITY, rectangle.height)),
                Point::new(rectangle.x + x, rectangle.y),
                color,
                bounds,
            );
        }
        let columns = self.pane.viewport().columns();
        if row.end_column.is_some() && columns > 0 {
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

pub fn is_monospaced(narrow_width: f32, wide_width: f32) -> bool {
    (narrow_width - wide_width).abs() <= wide_width * 0.01
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
    use mx_core::text::Buffer;

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
}
