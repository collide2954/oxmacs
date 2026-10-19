use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Widget, mouse};
use iced::widget::canvas::{Frame, Path};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Size, Theme, Vector};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Right,
    Left,
}

#[must_use]
pub struct Powerline<'a, Message> {
    direction: Direction,
    end: Color,
    height: f32,
    padding: f32,
    contents: Vec<Element<'a, Message>>,
    colors: Vec<Color>,
}

impl<Message> std::fmt::Debug for Powerline<'_, Message> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Powerline")
            .field("direction", &self.direction)
            .field("colors", &self.colors)
            .finish_non_exhaustive()
    }
}

impl<'a, Message> Powerline<'a, Message> {
    pub fn new(direction: Direction, end: Color, height: f32, padding: f32) -> Self {
        Self {
            direction,
            end,
            height,
            padding,
            contents: Vec::new(),
            colors: Vec::new(),
        }
    }

    pub fn push(mut self, content: impl Into<Element<'a, Message>>, color: Color) -> Self {
        self.contents.push(content.into());
        self.colors.push(color);
        self
    }

    fn arrow_width(&self) -> f32 {
        self.height / 2.0
    }

    fn behind(&self, index: usize) -> Color {
        let neighbor = match self.direction {
            Direction::Right => index.checked_add(1),
            Direction::Left => index.checked_sub(1),
        };
        neighbor
            .and_then(|neighbor| self.colors.get(neighbor))
            .copied()
            .unwrap_or(self.end)
    }
}

fn places(direction: Direction, widths: &[f32], arrow: f32) -> (Vec<f32>, f32) {
    let mut x = 0.0;
    let mut starts = Vec::with_capacity(widths.len());
    for width in widths {
        if direction == Direction::Left {
            x += arrow;
        }
        starts.push(x);
        x += width;
        if direction == Direction::Right {
            x += arrow;
        }
    }
    (starts, x)
}

fn corners(direction: Direction, area: Rectangle) -> [Point; 3] {
    let (base, tip) = match direction {
        Direction::Right => (area.x, area.x + area.width),
        Direction::Left => (area.x + area.width, area.x),
    };
    [
        Point::new(base, area.y),
        Point::new(tip, area.y + area.height / 2.0),
        Point::new(base, area.y + area.height),
    ]
}

impl<Message> Widget<Message, Theme, Renderer> for Powerline<'_, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Fixed(self.height))
    }

    fn children(&self) -> Vec<Tree> {
        self.contents.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.contents);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let inner = layout::Limits::new(Size::ZERO, Size::new(f32::INFINITY, self.height));
        let nodes: Vec<layout::Node> = self
            .contents
            .iter_mut()
            .zip(&mut tree.children)
            .map(|(content, tree)| content.as_widget_mut().layout(tree, renderer, &inner))
            .collect();
        let widths: Vec<f32> = nodes
            .iter()
            .map(|node| node.size().width + 2.0 * self.padding)
            .collect();
        let (starts, total) = places(self.direction, &widths, self.arrow_width());
        let children = nodes
            .into_iter()
            .zip(starts)
            .map(|(node, start)| {
                let top = (self.height - node.size().height) / 2.0;
                node.move_to(Point::new(start + self.padding, top))
            })
            .collect();
        let size = limits.resolve(Length::Shrink, self.height, Size::new(total, self.height));
        layout::Node::with_children(size, children)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((content, state), layout) in self
                .contents
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                content
                    .as_widget_mut()
                    .operate(state, layout, renderer, operation);
            }
        });
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let arrow = self.arrow_width();
        let mut frame = Frame::new(renderer, bounds.size());
        for (index, (child, color)) in layout.children().zip(&self.colors).enumerate() {
            let child = child.bounds();
            let left = child.x - bounds.x - self.padding;
            let width = child.width + 2.0 * self.padding;
            frame.fill_rectangle(Point::new(left, 0.0), Size::new(width, self.height), *color);
            let start = match self.direction {
                Direction::Right => left + width,
                Direction::Left => left - arrow,
            };
            let area = Rectangle::new(Point::new(start, 0.0), Size::new(arrow, self.height));
            frame.fill_rectangle(area.position(), area.size(), self.behind(index));
            let [base, tip, end] = corners(self.direction, area);
            let triangle = Path::new(|path| {
                path.move_to(base);
                path.line_to(tip);
                path.line_to(end);
                path.close();
            });
            frame.fill(&triangle, *color);
        }
        let geometry = frame.into_geometry();
        renderer.with_translation(Vector::new(bounds.x, bounds.y), |renderer| {
            renderer.draw_geometry(geometry);
        });
        for ((content, state), layout) in self
            .contents
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            content
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport);
        }
    }
}

impl<'a, Message: 'a> From<Powerline<'a, Message>> for Element<'a, Message> {
    fn from(powerline: Powerline<'a, Message>) -> Self {
        Element::new(powerline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_leave_room_for_an_arrow_on_the_side_they_point() {
        assert_eq!(
            places(Direction::Right, &[30.0, 20.0], 5.0),
            (vec![0.0, 35.0], 60.0)
        );
        assert_eq!(
            places(Direction::Left, &[30.0, 20.0], 5.0),
            (vec![5.0, 40.0], 60.0)
        );
    }

    #[test]
    fn arrows_span_their_area_and_point_across_it() {
        let area = Rectangle::new(Point::new(4.0, 0.0), Size::new(10.0, 20.0));

        assert_eq!(
            corners(Direction::Right, area),
            [
                Point::new(4.0, 0.0),
                Point::new(14.0, 10.0),
                Point::new(4.0, 20.0)
            ]
        );
        assert_eq!(
            corners(Direction::Left, area),
            [
                Point::new(14.0, 0.0),
                Point::new(4.0, 10.0),
                Point::new(14.0, 20.0)
            ]
        );
    }
}
