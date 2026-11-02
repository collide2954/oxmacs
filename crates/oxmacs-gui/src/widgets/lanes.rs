use iced::advanced::graphics::geometry;
use iced::widget::canvas::{Frame, LineCap, Path, Stroke};
use iced::{Color, Point, Vector};
use oxmacs_core::display::{LANE_COLUMNS, LineGraph};
use oxmacs_core::theme::Highlight;

use crate::units::to_f32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub left: f32,
    pub top: f32,
    pub middle: f32,
    pub bottom: f32,
    pub cell_width: f32,
}

impl Area {
    fn x(&self, lane: usize) -> f32 {
        let columns = to_f32(LANE_COLUMNS);
        self.left + (to_f32(lane) + 0.5) * columns * self.cell_width
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Line {
        from: Point,
        to: Point,
        face: Highlight,
    },
    Curve {
        from: Point,
        control: Point,
        to: Point,
        face: Highlight,
    },
    Bend {
        from: Point,
        first: Point,
        second: Point,
        to: Point,
        face: Highlight,
    },
    Dot {
        center: Point,
        face: Highlight,
    },
}

fn joined(from: Point, control: Point, to: Point, face: Highlight) -> Shape {
    if (from.x - to.x).abs() < f32::EPSILON {
        Shape::Line { from, to, face }
    } else {
        Shape::Curve {
            from,
            control,
            to,
            face,
        }
    }
}

pub fn shapes(graph: &LineGraph, area: Area) -> Vec<Shape> {
    let node = graph.node.map(|node| node.lane);
    let mut shapes: Vec<Shape> = graph
        .edges
        .iter()
        .filter_map(|edge| {
            let face = edge.face;
            match (edge.from, edge.to) {
                (Some(from), Some(to)) if from == to => Some(Shape::Line {
                    from: Point::new(area.x(from), area.top),
                    to: Point::new(area.x(to), area.bottom),
                    face,
                }),
                (Some(from), Some(to)) => Some(Shape::Bend {
                    from: Point::new(area.x(from), area.top),
                    first: Point::new(area.x(from), area.middle),
                    second: Point::new(area.x(to), area.middle),
                    to: Point::new(area.x(to), area.bottom),
                    face,
                }),
                (Some(from), None) => {
                    let start = Point::new(area.x(from), area.top);
                    let end = Point::new(area.x(node?), area.middle);
                    Some(joined(start, Point::new(start.x, end.y), end, face))
                }
                (None, Some(to)) => {
                    let start = Point::new(area.x(node?), area.middle);
                    let end = Point::new(area.x(to), area.bottom);
                    Some(joined(start, Point::new(end.x, start.y), end, face))
                }
                (None, None) => None,
            }
        })
        .collect();
    shapes.extend(graph.node.map(|node| Shape::Dot {
        center: Point::new(area.x(node.lane), area.middle),
        face: node.face,
    }));
    shapes
}

pub fn draw<Renderer: geometry::Renderer>(
    frame: &mut Frame<Renderer>,
    shapes: &[Shape],
    origin: Vector,
    cell_height: f32,
    color: impl Fn(Highlight) -> Color,
) {
    let width = (cell_height * 0.09).clamp(1.25, 3.0);
    let radius = cell_height * 0.22;
    let stroke = |face| {
        Stroke::default()
            .with_width(width)
            .with_color(color(face))
            .with_line_cap(LineCap::Round)
    };
    let at = |point: Point| point - origin;
    for shape in shapes {
        match *shape {
            Shape::Line { from, to, face } => {
                frame.stroke(&Path::line(at(from), at(to)), stroke(face));
            }
            Shape::Curve {
                from,
                control,
                to,
                face,
            } => {
                let path = Path::new(|path| {
                    path.move_to(at(from));
                    path.quadratic_curve_to(at(control), at(to));
                });
                frame.stroke(&path, stroke(face));
            }
            Shape::Bend {
                from,
                first,
                second,
                to,
                face,
            } => {
                let path = Path::new(|path| {
                    path.move_to(at(from));
                    path.bezier_curve_to(at(first), at(second), at(to));
                });
                frame.stroke(&path, stroke(face));
            }
            Shape::Dot { center, face } => {
                frame.fill(&Path::circle(at(center), radius), color(face));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::display::{GraphEdge, GraphNode};

    use super::*;

    const AREA: Area = Area {
        left: 100.0,
        top: 0.0,
        middle: 10.0,
        bottom: 20.0,
        cell_width: 5.0,
    };

    fn edge(from: Option<usize>, to: Option<usize>) -> GraphEdge {
        GraphEdge {
            from,
            to,
            face: Highlight::Nick(1),
        }
    }

    #[test]
    fn lanes_become_lines_curves_and_a_dot_on_the_node() {
        let graph = LineGraph {
            node: Some(GraphNode {
                lane: 1,
                face: Highlight::Nick(2),
            }),
            edges: vec![
                edge(Some(0), Some(0)),
                edge(Some(1), None),
                edge(Some(2), None),
                edge(None, Some(1)),
                edge(None, Some(3)),
                edge(Some(3), Some(2)),
            ],
        };
        let face = Highlight::Nick(1);

        assert_eq!(
            shapes(&graph, AREA),
            [
                Shape::Line {
                    from: Point::new(105.0, 0.0),
                    to: Point::new(105.0, 20.0),
                    face
                },
                Shape::Line {
                    from: Point::new(115.0, 0.0),
                    to: Point::new(115.0, 10.0),
                    face
                },
                Shape::Curve {
                    from: Point::new(125.0, 0.0),
                    control: Point::new(125.0, 10.0),
                    to: Point::new(115.0, 10.0),
                    face
                },
                Shape::Line {
                    from: Point::new(115.0, 10.0),
                    to: Point::new(115.0, 20.0),
                    face
                },
                Shape::Curve {
                    from: Point::new(115.0, 10.0),
                    control: Point::new(135.0, 10.0),
                    to: Point::new(135.0, 20.0),
                    face
                },
                Shape::Bend {
                    from: Point::new(135.0, 0.0),
                    first: Point::new(135.0, 10.0),
                    second: Point::new(125.0, 10.0),
                    to: Point::new(125.0, 20.0),
                    face
                },
                Shape::Dot {
                    center: Point::new(115.0, 10.0),
                    face: Highlight::Nick(2)
                },
            ]
        );
    }

    #[test]
    fn edges_to_a_missing_node_are_left_out() {
        let graph = LineGraph {
            node: None,
            edges: vec![edge(None, Some(1)), edge(Some(0), Some(0))],
        };

        assert_eq!(shapes(&graph, AREA).len(), 1);
    }
}
