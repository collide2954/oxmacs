use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Size, Theme, mouse};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Right,
    Left,
}

#[derive(Debug, Clone, Copy)]
struct Arrow {
    direction: Direction,
    color: Color,
    behind: Color,
}

pub fn arrow<'a, Message: 'a>(
    direction: Direction,
    color: Color,
    behind: Color,
    height: f32,
) -> Element<'a, Message> {
    Canvas::new(Arrow {
        direction,
        color,
        behind,
    })
    .width(Length::Fixed(height / 2.0))
    .height(Length::Fixed(height))
    .into()
}

fn corners(direction: Direction, size: Size) -> [Point; 3] {
    let (base, tip) = match direction {
        Direction::Right => (0.0, size.width),
        Direction::Left => (size.width, 0.0),
    };
    [
        Point::new(base, 0.0),
        Point::new(tip, size.height / 2.0),
        Point::new(base, size.height),
    ]
}

impl<Message> canvas::Program<Message> for Arrow {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), self.behind);
        let [base, tip, end] = corners(self.direction, bounds.size());
        let triangle = Path::new(|path| {
            path.move_to(base);
            path.line_to(tip);
            path.line_to(end);
            path.close();
        });
        frame.fill(&triangle, self.color);
        vec![frame.into_geometry()]
    }
}
#[cfg(test)]
mod tests {
    use iced::{Point, Size};

    use super::*;

    #[test]
    fn arrows_span_the_whole_height_and_point_across() {
        let size = Size::new(10.0, 20.0);

        assert_eq!(
            corners(Direction::Right, size),
            [
                Point::new(0.0, 0.0),
                Point::new(10.0, 10.0),
                Point::new(0.0, 20.0)
            ]
        );
        assert_eq!(
            corners(Direction::Left, size),
            [
                Point::new(10.0, 0.0),
                Point::new(0.0, 10.0),
                Point::new(10.0, 20.0)
            ]
        );
    }
}
