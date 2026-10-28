use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::overlay::Overlay;
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Text};
use iced::alignment::Vertical;
use iced::{Border, Font, Pixels, Point, Rectangle, Shadow, Size, Theme, Vector};
use oxmacs_core::editor::PopupView;
use oxmacs_core::theme::{Color, Face, Theme as Faces};

use crate::style;
use crate::units::to_f32;

const INSET: f32 = 8.0;
const EDGE: f32 = 4.0;
const RADIUS: f32 = 6.0;
const SHADE: f32 = 0.25;
const STANDS_OUT: f64 = 1.4;

#[derive(Debug)]
pub struct Popup {
    view: PopupView,
    anchor: Point,
    cell: Size,
    faces: Faces,
    font: Font,
    size: Pixels,
}

impl Popup {
    pub fn new(
        view: PopupView,
        anchor: Point,
        cell: Size,
        faces: Faces,
        font: Font,
        size: Pixels,
    ) -> Self {
        Self {
            view,
            anchor,
            cell,
            faces,
            font,
            size,
        }
    }

    fn extent(&self) -> Size {
        let widest = self
            .view
            .items
            .iter()
            .map(|item| item.chars().count())
            .max()
            .unwrap_or(0);
        Size::new(
            to_f32(widest) * self.cell.width + 2.0 * INSET,
            to_f32(self.view.items.len()) * self.cell.height + 2.0 * EDGE,
        )
    }
}

pub fn surface(faces: &Faces) -> Color {
    faces.surface()
}

pub fn selected(faces: &Faces) -> Face {
    let current = faces.completion_current;
    if current.background.contrast_ratio(surface(faces)) >= STANDS_OUT {
        current
    } else {
        Face {
            foreground: faces.default.foreground,
            background: faces.region,
        }
    }
}

pub fn place(anchor: Point, cell: Size, extent: Size, window: Size) -> Point {
    let below = anchor.y + cell.height;
    let y = if below + extent.height <= window.height {
        below
    } else {
        (anchor.y - extent.height).max(0.0)
    };
    let x = (anchor.x - INSET).min(window.width - extent.width).max(0.0);
    Point::new(x, y)
}

impl<Message, Renderer> Overlay<Message, Theme, Renderer> for Popup
where
    Renderer: text::Renderer<Font = Font>,
{
    fn layout(&mut self, _renderer: &Renderer, bounds: Size) -> layout::Node {
        let extent = self.extent();
        layout::Node::new(extent).move_to(place(self.anchor, self.cell, extent, bounds))
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
    ) {
        let bounds = layout.bounds();
        let surface = surface(&self.faces);
        let chosen = selected(&self.faces);
        renderer.fill_quad(
            Quad {
                bounds,
                border: Border {
                    color: style::color(self.faces.mode_line_border),
                    width: 1.0,
                    radius: RADIUS.into(),
                },
                shadow: Shadow {
                    color: iced::Color::BLACK.scale_alpha(SHADE),
                    offset: Vector::new(0.0, 2.0),
                    blur_radius: 8.0,
                },
                snap: true,
            },
            style::color(surface),
        );
        for (index, item) in self.view.items.iter().enumerate() {
            let current = index == self.view.selected;
            let row = Rectangle::new(
                Point::new(
                    bounds.x + 1.0,
                    bounds.y + EDGE + to_f32(index) * self.cell.height,
                ),
                Size::new(bounds.width - 2.0, self.cell.height),
            );
            if current {
                renderer.fill_quad(
                    Quad {
                        bounds: row,
                        ..Quad::default()
                    },
                    style::color(chosen.background),
                );
            }
            let face = if current { chosen } else { self.faces.default };
            renderer.fill_text(
                Text {
                    content: item.clone(),
                    bounds: Size::new(f32::INFINITY, self.cell.height),
                    size: self.size,
                    line_height: text::LineHeight::default(),
                    font: self.font,
                    align_x: text::Alignment::Left,
                    align_y: Vertical::Top,
                    shaping: text::Shaping::Advanced,
                    wrapping: text::Wrapping::None,
                },
                Point::new(bounds.x + INSET, row.y),
                style::color(face.foreground),
                bounds,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use oxmacs_core::theme::{ADWAITA, ADWAITA_DARK};

    const CELL: Size = Size::new(10.0, 20.0);
    const WINDOW: Size = Size::new(800.0, 600.0);

    #[test]
    fn the_selected_candidate_stands_out_and_stays_readable() {
        for faces in [ADWAITA, ADWAITA_DARK] {
            let chosen = selected(&faces);

            assert!(chosen.background.contrast_ratio(surface(&faces)) >= STANDS_OUT);
            assert!(chosen.foreground.contrast_ratio(chosen.background) >= 3.0);
            assert!(faces.default.foreground.contrast_ratio(surface(&faces)) >= 4.5);
        }
    }

    #[test]
    fn the_popup_opens_under_the_word_with_its_text_aligned() {
        let at = place(Point::new(100.0, 40.0), CELL, Size::new(80.0, 48.0), WINDOW);

        assert_eq!(at, Point::new(100.0 - INSET, 60.0));
    }

    #[test]
    fn near_the_bottom_it_opens_above_the_word() {
        let at = place(
            Point::new(100.0, 560.0),
            CELL,
            Size::new(80.0, 48.0),
            WINDOW,
        );

        assert_eq!(at, Point::new(100.0 - INSET, 512.0));
    }

    #[test]
    fn near_the_right_edge_it_slides_left_to_stay_whole() {
        let at = place(Point::new(780.0, 40.0), CELL, Size::new(80.0, 48.0), WINDOW);

        assert_eq!(at.x, 720.0);
    }
}
