use std::cell::RefCell;

use iced::widget::{
    Column, Row, button, canvas, container, image, mouse_area, scrollable, slider, space, text,
};
use iced::{Border, Color, ContentFit, Element, Font, Length};
use oxmacs_core::theme::Theme as Faces;
use oxmacs_core::view::{Arg, Node, Picture};

use crate::style;
use crate::widgets::drawing::Drawing;
use crate::widgets::game::GameScreen;
use crate::widgets::pages::PagesView;
use crate::widgets::pdf::PdfView;

const FRAME: f32 = 2.0;
const FRAME_RADIUS: f32 = 4.0;

thread_local! {
    static PICTURES: RefCell<Vec<(Picture, image::Handle)>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn handle(picture: &Picture) -> image::Handle {
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

pub struct Slots<'a, Message>(Box<dyn Fn(usize, Vec<Arg>) -> Message + 'a>);

impl<Message> std::fmt::Debug for Slots<'_, Message> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Slots").finish_non_exhaustive()
    }
}

impl<'a, Message> Slots<'a, Message> {
    pub fn new(call: impl Fn(usize, Vec<Arg>) -> Message + 'a) -> Self {
        Self(Box::new(call))
    }

    pub fn send(&self, slot: Option<&usize>, args: Vec<Arg>) -> Option<Message> {
        slot.map(|slot| (self.0)(*slot, args))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub faces: Faces,
    pub font: Font,
    pub size: f32,
}

fn framed<'a, Message: Clone + 'a>(
    picture: &Picture,
    width: Option<f32>,
    height: Option<f32>,
    selected: bool,
    look: Look,
    pressed: Option<Message>,
) -> Element<'a, Message> {
    let shown = image(handle(picture))
        .content_fit(ContentFit::Contain)
        .width(width.map_or(Length::Shrink, Length::Fixed))
        .height(height.map_or(Length::Shrink, Length::Fixed));
    let accent = style::color(look.faces.cursor);
    let framed = container(shown)
        .padding(FRAME)
        .style(move |_| container::Style {
            border: Border {
                color: if selected { accent } else { Color::TRANSPARENT },
                width: FRAME,
                radius: FRAME_RADIUS.into(),
            },
            ..container::Style::default()
        });
    match pressed {
        Some(message) => mouse_area(framed).on_press(message).into(),
        None => framed.into(),
    }
}

fn ranged<'a, Message: Clone + 'a>(
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: Option<f32>,
    width: Option<f32>,
    change: Option<usize>,
    call: impl Fn(usize, Vec<Arg>) -> Message + Copy + 'a,
) -> Element<'a, Message> {
    let step = step.unwrap_or((range.end() - range.start()) / 200.0);
    slider(range, value, move |changed| {
        change.map_or_else(
            || call(usize::MAX, Vec::new()),
            |slot| call(slot, vec![Arg::Float(f64::from(changed))]),
        )
    })
    .step(step)
    .width(width.map_or(Length::Fill, Length::Fixed))
    .into()
}

pub fn view<'a, Message: Clone + 'a>(
    node: &'a Node<usize>,
    look: Look,
    press: impl Fn(usize) -> Message + Copy + 'a,
    call: impl Fn(usize, Vec<Arg>) -> Message + Copy + 'a,
) -> Element<'a, Message> {
    match node {
        Node::Column { spacing, children } => {
            Column::with_children(children.iter().map(|child| view(child, look, press, call)))
                .spacing(*spacing)
                .into()
        }
        Node::Row { spacing, children } => {
            Row::with_children(children.iter().map(|child| view(child, look, press, call)))
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
            let message = slot.map(press);
            button(text(label.as_str()).font(look.font).size(look.size))
                .on_press_maybe(message)
                .into()
        }
        Node::Slider {
            value,
            low,
            high,
            step,
            width,
            change,
        } => ranged(*value, *low..=*high, *step, *width, *change, call),
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
        } => framed(picture, *width, *height, *selected, look, slot.map(press)),
        Node::Pages(pages) => PagesView::new(pages, look.faces, look.font, call).into(),
        Node::Pdf(shown) => PdfView::new(shown, look.faces, call).into(),
        Node::Game(screen) => canvas(GameScreen {
            screen: screen.as_ref(),
            font: look.font,
            size: look.size,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
        Node::Scrollable(child) => scrollable(view(child, look, press, call))
            .height(Length::Fill)
            .into(),
    }
}
