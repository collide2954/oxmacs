use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer::{self, Quad};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::{Color, Element, Event, Length, Rectangle, Size, Theme};
use oxmacs_core::editor::Axis;

const THICKNESS: f32 = 5.0;

pub struct Divider<'a, Message> {
    axis: Axis,
    extent: f32,
    color: Color,
    on_drag: Box<dyn Fn(f32) -> Message + 'a>,
}

impl<Message> std::fmt::Debug for Divider<'_, Message> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Divider")
            .field("axis", &self.axis)
            .field("extent", &self.extent)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct State {
    dragging: bool,
}

impl<'a, Message> Divider<'a, Message> {
    pub fn new(
        axis: Axis,
        extent: f32,
        color: Color,
        on_drag: impl Fn(f32) -> Message + 'a,
    ) -> Self {
        Self {
            axis,
            extent,
            color,
            on_drag: Box::new(on_drag),
        }
    }
}

impl<Message> Divider<'_, Message> {
    fn lengths(&self) -> Size<Length> {
        match self.axis {
            Axis::Rows => Size::new(Length::Fill, Length::Fixed(THICKNESS)),
            Axis::Columns => Size::new(Length::Fixed(THICKNESS), Length::Fill),
        }
    }
}

pub fn drag_fraction(axis: Axis, bounds: Rectangle, cursor: iced::Point, extent: f32) -> f32 {
    if extent <= 0.0 {
        return 0.0;
    }
    let center = bounds.center();
    let moved = match axis {
        Axis::Rows => cursor.y - center.y,
        Axis::Columns => cursor.x - center.x,
    };
    moved / extent
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for Divider<'_, Message>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        self.lengths()
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = self.lengths();
        layout::Node::new(limits.resolve(size.width, size.height, Size::ZERO))
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
        let bounds = layout.bounds();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                state.dragging = true;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging => {
                let fraction = drag_fraction(self.axis, bounds, *position, self.extent);
                if fraction != 0.0 {
                    shell.publish((self.on_drag)(fraction));
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
                state.dragging = false;
                shell.capture_event();
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
        let dragging = tree.state.downcast_ref::<State>().dragging;
        if !dragging && !cursor.is_over(layout.bounds()) {
            return mouse::Interaction::None;
        }
        match self.axis {
            Axis::Rows => mouse::Interaction::ResizingVertically,
            Axis::Columns => mouse::Interaction::ResizingHorizontally,
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let center = bounds.center();
        let line = match self.axis {
            Axis::Rows => Rectangle::new(
                iced::Point::new(bounds.x, center.y.floor()),
                Size::new(bounds.width, 1.0),
            ),
            Axis::Columns => Rectangle::new(
                iced::Point::new(center.x.floor(), bounds.y),
                Size::new(1.0, bounds.height),
            ),
        };
        renderer.fill_quad(
            Quad {
                bounds: line,
                ..Quad::default()
            },
            self.color,
        );
    }
}

impl<'a, Message, Renderer> From<Divider<'a, Message>> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(divider: Divider<'a, Message>) -> Self {
        Element::new(divider)
    }
}

#[cfg(test)]
mod tests {
    use iced::Point;

    use super::*;

    #[test]
    fn drags_move_by_a_fraction_of_the_split() {
        let bounds = Rectangle::new(Point::new(100.0, 0.0), Size::new(4.0, 300.0));

        let fraction = drag_fraction(Axis::Columns, bounds, Point::new(142.0, 10.0), 400.0);
        assert!((fraction - 0.1).abs() < 1e-4);

        let rows = Rectangle::new(Point::new(0.0, 98.0), Size::new(300.0, 4.0));
        let fraction = drag_fraction(Axis::Rows, rows, Point::new(5.0, 60.0), 400.0);
        assert!((fraction + 0.1).abs() < 1e-4);
        assert!(drag_fraction(Axis::Rows, rows, Point::new(5.0, 60.0), 0.0).abs() < f32::EPSILON);
    }
}
