use std::ops::Range;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Paragraph as _, Text};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::alignment::Vertical;
use iced::time::Instant;
use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme, mouse,
    window,
};
use mx_core::display;
use mx_core::editor::Editor;

use crate::animation::{self, Glide};
use crate::style;
use crate::units::to_f32;

const WIDE_SAMPLE: &str = "MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM";
const NARROW_SAMPLE: &str = "iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiii";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewEvent {
    RowsChanged(usize),
    FontNotMonospaced,
}

#[derive(Debug, Clone, Copy)]
pub struct Motion {
    pub enabled: bool,
    pub last_input: Instant,
}

#[derive(Debug)]
pub struct BufferView<'a, Message> {
    editor: &'a Editor,
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
    frame: Option<Instant>,
}

impl<'a, Message> BufferView<'a, Message> {
    pub fn new(
        editor: &'a Editor,
        font: Font,
        size: impl Into<Pixels>,
        motion: Motion,
        on_event: fn(ViewEvent) -> Message,
    ) -> Self {
        Self {
            editor,
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
        let buffer = self.editor.buffer();
        (
            self.editor.point_display_column(),
            buffer.line_of(buffer.point()),
        )
    }

    fn blinking(&self) -> bool {
        self.motion.enabled && self.editor.minibuffer().is_none()
    }

    fn animate<M>(&self, state: &mut State, now: Instant, shell: &mut Shell<'_, M>) {
        state.frame = Some(now);
        let target = self.cursor_cell();
        let cursor = state.cursor.get_or_insert_with(|| Glide::new(target));
        cursor.retarget(target, now, self.motion.enabled);
        if cursor.is_animating(now) {
            shell.request_redraw();
            return;
        }
        let idle = now.saturating_duration_since(self.motion.last_input);
        if self.blinking()
            && let Some(wait) = animation::next_blink_frame(idle)
        {
            shell.request_redraw_at(now + wait);
        }
    }

    fn draw_cursor<Renderer>(&self, renderer: &mut Renderer, state: &State, bounds: Rectangle)
    where
        Renderer: text::Renderer<Font = Font>,
    {
        let cell = state.metrics.cell;
        let target = self.cursor_cell();
        let (column, line, gliding) = match (&state.cursor, state.frame) {
            (Some(glide), Some(frame)) => {
                let (column, line) = glide.position(frame);
                (column, line, glide.is_animating(frame))
            }
            _ => (to_f32(target.0), to_f32(target.1), false),
        };
        let top = to_f32(self.editor.viewport().top());
        let origin = Point::new(
            bounds.x + column * cell.width,
            bounds.y + (line - top) * cell.height,
        );
        if origin.y < bounds.y || origin.y >= bounds.y + bounds.height {
            return;
        }
        let block = Rectangle::new(origin, cell);
        let faces = self.editor.theme();
        if self.editor.minibuffer().is_some() {
            let hollow = Quad {
                bounds: block,
                border: Border {
                    color: style::color(faces.cursor),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Quad::default()
            };
            renderer.with_layer(bounds, |renderer| {
                renderer.fill_quad(hollow, Color::TRANSPARENT);
            });
            return;
        }
        let opacity = match state.frame {
            Some(frame) if self.blinking() => {
                animation::cursor_opacity(frame.saturating_duration_since(self.motion.last_input))
            }
            _ => 1.0,
        };
        let buffer = self.editor.buffer();
        let under_cursor = buffer
            .chars_after(buffer.point())
            .next()
            .filter(|c| !c.is_control() && !gliding);
        renderer.with_layer(bounds, |renderer| {
            renderer.fill_quad(
                Quad {
                    bounds: block,
                    ..Quad::default()
                },
                style::color(faces.cursor).scale_alpha(opacity),
            );
            if let Some(c) = under_cursor {
                renderer.fill_text(
                    self.text(c.to_string(), Size::new(cell.width * 2.0, cell.height)),
                    origin,
                    style::color(faces.default.background).scale_alpha(opacity),
                    block,
                );
            }
        });
    }
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
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            self.animate(state, *now, shell);
        }
        let metrics = &mut state.metrics;
        if metrics.measured_for.is_some() && !metrics.monospaced && !metrics.font_reported {
            metrics.font_reported = true;
            shell.publish((self.on_event)(ViewEvent::FontNotMonospaced));
        }
        let rows = rows_in(layout.bounds().height, metrics.cell.height);
        if rows != self.editor.viewport().rows() {
            shell.publish((self.on_event)(ViewEvent::RowsChanged(rows)));
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
        let buffer = self.editor.buffer();
        let foreground = style::color(self.editor.theme().default.foreground);
        let visible = self
            .editor
            .viewport()
            .lines()
            .take_while(|line| *line < buffer.len_lines());
        let selection = buffer.selection();
        for (row, line) in visible.enumerate() {
            let text = buffer.line_text(line);
            let row = Rectangle::new(
                Point::new(bounds.x, bounds.y + to_f32(row) * cell.height),
                Size::new(bounds.width, cell.height),
            );
            if let Some(selection) = &selection {
                self.draw_highlight(renderer, &text, line, selection, row, cell.width);
            }
            let content = display::render_line(&text, self.editor.tab_width());
            renderer.fill_text(
                self.text(content, row.size()),
                row.position(),
                foreground,
                bounds,
            );
        }
        self.draw_cursor(renderer, state, bounds);
    }
}

impl<Message> BufferView<'_, Message> {
    fn draw_highlight<Renderer>(
        &self,
        renderer: &mut Renderer,
        text: &str,
        line: usize,
        selection: &Range<usize>,
        row: Rectangle,
        cell_width: f32,
    ) where
        Renderer: text::Renderer<Font = Font>,
    {
        let buffer = self.editor.buffer();
        let span = buffer.line_start(line)..buffer.line_end(line);
        let Some((start, end)) =
            display::highlight_columns(text, span, selection, self.editor.tab_width())
        else {
            return;
        };
        let left = row.x + to_f32(start) * cell_width;
        let right = end.map_or(row.x + row.width, |end| row.x + to_f32(end) * cell_width);
        renderer.fill_quad(
            Quad {
                bounds: Rectangle::new(
                    Point::new(left, row.y),
                    Size::new(right - left, row.height),
                ),
                ..Quad::default()
            },
            style::color(self.editor.theme().region),
        );
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

pub fn is_monospaced(narrow_width: f32, wide_width: f32) -> bool {
    (narrow_width - wide_width).abs() <= wide_width * 0.01
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

    #[test]
    fn rows_are_never_zero() {
        assert_eq!(rows_in(10.0, 20.0), 1);
        assert_eq!(rows_in(100.0, 0.0), 1);
        assert_eq!(rows_in(f32::NAN, 20.0), 1);
    }
}
