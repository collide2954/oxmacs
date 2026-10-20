use iced::font::Family;
use iced::widget::{Column, container, scrollable, text};
use iced::{Border, Element, Font, Length};
use oxmacs_core::theme::{Color, Highlight, Theme as Faces};
use oxmacs_gui::style;

use crate::protocol::network::Member;
use crate::protocol::nick_color;

const UI_FONT: Font = Font {
    family: Family::SansSerif,
    ..Font::DEFAULT
};
const WIDTH: f32 = 10.0;
const SECONDARY: f32 = 0.9;
const AWAY_FADE: f32 = 0.5;
const PADDING: u16 = 8;
const SPACING: f32 = 2.0;
const TINT: f32 = 0.04;

pub fn view<'a, Message: 'a>(
    members: &[Member],
    own: Option<&str>,
    faces: Faces,
    size: f32,
) -> Element<'a, Message> {
    let background = faces.default.background;
    let label = |content: String, color: Color| {
        text(content)
            .font(UI_FONT)
            .size(size * SECONDARY)
            .color(style::color(color))
            .wrapping(text::Wrapping::None)
    };
    let header = label(
        format!("{} members", members.len()),
        faces.highlight(Highlight::Comment),
    );
    let rows = members.iter().map(|member| {
        let rank = member.prefixes.chars().next().unwrap_or(' ');
        let highlight = if own.is_some_and(|own| own.eq_ignore_ascii_case(&member.nick)) {
            Highlight::Heading
        } else {
            Highlight::Nick(nick_color(&member.nick))
        };
        let color = faces.highlight(highlight);
        let color = if member.away {
            color.mix(background, AWAY_FADE)
        } else {
            color
        };
        label(format!("{rank}{}", member.nick), color).into()
    });
    let list = Column::with_children(std::iter::once(header.into()).chain(rows))
        .spacing(SPACING)
        .padding(PADDING);
    let fill = style::color(background.mix(faces.default.foreground, TINT));
    let edge = style::color(faces.mode_line_border);
    container(scrollable(list).height(Length::Fill))
        .width(Length::Fixed(size * WIDTH))
        .height(Length::Fill)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(fill.into()),
            border: Border {
                color: edge,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}
