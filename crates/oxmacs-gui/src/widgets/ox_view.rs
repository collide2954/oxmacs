use iced::widget::{Column, Row, button, canvas, scrollable, space, text};
use iced::{Element, Font, Length};
use oxmacs_core::theme::Theme as Faces;
use oxmacs_ox::views::{Node, Press};

use crate::style;
use crate::widgets::drawing::Drawing;

#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub faces: Faces,
    pub font: Font,
    pub size: f32,
}

pub fn view<'a, Message: Clone + 'a>(
    node: &'a Node,
    look: Look,
    press: impl Fn(usize) -> Message + Copy + 'a,
) -> Element<'a, Message> {
    match node {
        Node::Column { spacing, children } => {
            Column::with_children(children.iter().map(|child| view(child, look, press)))
                .spacing(*spacing)
                .into()
        }
        Node::Row { spacing, children } => {
            Row::with_children(children.iter().map(|child| view(child, look, press)))
                .spacing(*spacing)
                .into()
        }
        Node::Text {
            text: shown,
            face,
            size,
        } => {
            let color = face.map_or(look.faces.default.foreground, |face| {
                look.faces.highlight(face)
            });
            text(shown.as_str())
                .font(look.font)
                .size(size.unwrap_or(look.size))
                .color(style::color(color))
                .into()
        }
        Node::Button { label, press: slot } => {
            let message = match slot {
                Some(Press::Slot(slot)) => Some(press(*slot)),
                _ => None,
            };
            button(text(label.as_str()).font(look.font).size(look.size))
                .on_press_maybe(message)
                .into()
        }
        Node::Space { size: Some(size) } => space().width(*size).height(*size).into(),
        Node::Space { size: None } => space().width(Length::Fill).into(),
        Node::Canvas {
            shapes,
            height,
            relative,
        } => canvas(Drawing {
            shapes,
            relative: *relative,
            background: None,
            faces: look.faces,
            font: look.font,
            size: look.size,
        })
        .width(Length::Fill)
        .height(height.map_or(Length::Fill, Length::Fixed))
        .into(),
        Node::Scrollable(child) => scrollable(view(child, look, press))
            .height(Length::Fill)
            .into(),
    }
}
