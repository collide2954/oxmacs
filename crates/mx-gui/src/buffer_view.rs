use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Paragraph as _, Text};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::alignment::Vertical;
use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme, mouse,
};
use mx_core::display;
use mx_core::editor::Editor;

use crate::style;

const WIDE_SAMPLE: &str = "MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM";
const NARROW_SAMPLE: &str = "iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiii";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewEvent {
    RowsChanged(usize),
    FontNotMonospaced,
}

#[derive(Debug)]
pub struct BufferView<'a, Message> {
    editor: &'a Editor,
    font: Font,
    size: Pixels,
    on_event: fn(ViewEvent) -> Message,
}

#[derive(Debug, Default)]
struct Metrics {
    measured_for: Option<(Font, Pixels)>,
    cell: Size,
    monospaced: bool,
    font_reported: bool,
}

impl<'a, Message> BufferView<'a, Message> {
    pub fn new(
        editor: &'a Editor,
        font: Font,
        size: impl Into<Pixels>,
        on_event: fn(ViewEvent) -> Message,
    ) -> Self {
        Self {
            editor,
            font,
            size: size.into(),
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

    fn draw_cursor<Renderer>(&self, renderer: &mut Renderer, bounds: Rectangle, cell: Size)
    where
        Renderer: text::Renderer<Font = Font>,
    {
        let buffer = self.editor.buffer();
        let point = buffer.point();
        let Some(row) = buffer
            .line_of(point)
            .checked_sub(self.editor.viewport().top())
        else {
            return;
        };
        let origin = Point::new(
            bounds.x + to_f32(self.editor.point_display_column()) * cell.width,
            bounds.y + to_f32(row) * cell.height,
        );
        let block = Rectangle::new(origin, cell);
        let under_cursor = buffer.chars_after(point).next().filter(|c| !c.is_control());
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
        renderer.with_layer(bounds, |renderer| {
            renderer.fill_quad(
                Quad {
                    bounds: block,
                    ..Quad::default()
                },
                style::color(faces.cursor),
            );
            if let Some(c) = under_cursor {
                renderer.fill_text(
                    self.text(c.to_string(), Size::new(cell.width * 2.0, cell.height)),
                    origin,
                    style::color(faces.default.background),
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
        tree::Tag::of::<Metrics>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Metrics::default())
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
        let metrics = tree.state.downcast_mut::<Metrics>();
        if metrics.measured_for != Some((self.font, self.size)) {
            *metrics = self.measure::<Renderer>();
        }
        layout::Node::new(limits.resolve(Length::Fill, Length::Fill, Size::ZERO))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        _event: &Event,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let metrics = tree.state.downcast_mut::<Metrics>();
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
        let cell = tree.state.downcast_ref::<Metrics>().cell;
        let bounds = layout.bounds();
        let buffer = self.editor.buffer();
        let foreground = style::color(self.editor.theme().default.foreground);
        let visible = self
            .editor
            .viewport()
            .lines()
            .take_while(|line| *line < buffer.len_lines());
        for (row, line) in visible.enumerate() {
            let content = display::render_line(&buffer.line_text(line), self.editor.tab_width());
            renderer.fill_text(
                self.text(content, Size::new(bounds.width, cell.height)),
                Point::new(bounds.x, bounds.y + to_f32(row) * cell.height),
                foreground,
                bounds,
            );
        }
        self.draw_cursor(renderer, bounds, cell);
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

#[expect(
    clippy::cast_precision_loss,
    reason = "screen coordinates stay far below the f32 precision limit"
)]
fn to_f32(value: usize) -> f32 {
    value as f32
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
