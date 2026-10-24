use std::cell::RefCell;

use iced::widget::{
    Column, Row, button, canvas, container, image, mouse_area, scrollable, space, text,
};
use iced::{Border, Color, ContentFit, Element, Font, Length};
use oxmacs_core::theme::Theme as Faces;
use oxmacs_ox::views::{Node, Picture, Press};

use crate::style;
use crate::widgets::drawing::Drawing;

const FRAME: f32 = 2.0;
const FRAME_RADIUS: f32 = 4.0;

thread_local! {
    static PICTURES: RefCell<Vec<(Picture, image::Handle)>> = const { RefCell::new(Vec::new()) };
}

fn handle(picture: &Picture) -> image::Handle {
    PICTURES.with_borrow_mut(|known| {
        known.retain(|(kept, _)| !kept.unused());
        if let Some((_, handle)) = known.iter().find(|(kept, _)| kept.key() == picture.key()) {
            return handle.clone();
        }
        let (width, height) = picture.size();
        let handle = image::Handle::from_rgba(width, height, picture.rgba());
        known.push((picture.clone(), handle.clone()));
        handle
    })
}

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
        Node::Image {
            picture,
            width,
            height,
            press: slot,
            selected,
        } => {
            let shown = image(handle(picture))
                .content_fit(ContentFit::Contain)
                .width(width.map_or(Length::Shrink, Length::Fixed))
                .height(height.map_or(Length::Shrink, Length::Fixed));
            let accent = style::color(look.faces.cursor);
            let framed = container(shown)
                .padding(FRAME)
                .style(move |_| container::Style {
                    border: Border {
                        color: if *selected {
                            accent
                        } else {
                            Color::TRANSPARENT
                        },
                        width: FRAME,
                        radius: FRAME_RADIUS.into(),
                    },
                    ..container::Style::default()
                });
            match slot {
                Some(Press::Slot(slot)) => mouse_area(framed).on_press(press(*slot)).into(),
                _ => framed.into(),
            }
        }
        Node::Scrollable(child) => scrollable(view(child, look, press))
            .height(Length::Fill)
            .into(),
    }
}
