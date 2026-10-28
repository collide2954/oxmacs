use iced::widget::{Row, column, container, mouse_area, row, space, text};
use iced::{Alignment, Element, Font, Length, mouse};
use oxmacs_core::theme::{Color, Theme as Faces};
use oxmacs_core::view::tabbar::TabBar;

use crate::style;

const ACCENT: f32 = 2.0;
const HOVER: f32 = 0.5;
const CLOSE: &str = "×";
const NEW: &str = "+";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabMessage {
    Pressed(usize),
    Released(usize),
    Entered(usize),
    Left(usize),
    Close(usize),
    New,
    BarReleased,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub faces: Faces,
    pub font: Font,
    pub size: f32,
    pub hovered: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Colors {
    strip: Color,
    hovered: Color,
    selected: Color,
    accent: Color,
    text: Color,
    current: Color,
    rule: Color,
}

fn colors(faces: &Faces) -> Colors {
    let strip = faces.mode_line_inactive.background;
    Colors {
        strip,
        hovered: strip.mix(faces.default.background, HOVER),
        selected: faces.default.background,
        accent: faces.cursor,
        text: faces.default.foreground.dimmed(strip),
        current: faces.default.foreground,
        rule: faces.mode_line_border,
    }
}

fn label<'a, M: 'a>(shown: &str, color: Color, look: Look) -> Element<'a, M> {
    text(shown.to_owned())
        .font(look.font)
        .size(look.size)
        .color(style::color(color))
        .into()
}

fn tab<'a, M: Clone + 'a>(
    index: usize,
    name: &str,
    current: bool,
    look: Look,
    colors: Colors,
    on: impl Fn(TabMessage) -> M + Copy,
) -> Element<'a, M> {
    let hovered = look.hovered == Some(index);
    let ink = if current { colors.current } else { colors.text };
    let close: Element<'a, M> = if current || hovered {
        mouse_area(label(CLOSE, ink, look))
            .on_press(on(TabMessage::Close(index)))
            .interaction(mouse::Interaction::Pointer)
            .into()
    } else {
        text(CLOSE)
            .font(look.font)
            .size(look.size)
            .color(iced::Color::TRANSPARENT)
            .into()
    };
    let body = container(
        row![label(name, ink, look), close]
            .spacing(look.size * 0.6)
            .align_y(Alignment::Center),
    )
    .padding([look.size * 0.3, look.size * 0.9]);
    let accent = style::color(colors.accent);
    let top = container(space().height(ACCENT))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: current.then(|| accent.into()),
            ..container::Style::default()
        });
    let fill = if current {
        Some(colors.selected)
    } else if hovered {
        Some(colors.hovered)
    } else {
        None
    };
    let shaped =
        container(column![top, body].width(Length::Shrink)).style(move |_| container::Style {
            background: fill.map(|fill| style::color(fill).into()),
            ..container::Style::default()
        });
    mouse_area(shaped)
        .on_press(on(TabMessage::Pressed(index)))
        .on_release(on(TabMessage::Released(index)))
        .on_middle_press(on(TabMessage::Close(index)))
        .on_enter(on(TabMessage::Entered(index)))
        .on_exit(on(TabMessage::Left(index)))
        .interaction(mouse::Interaction::Pointer)
        .into()
}

fn rule<'a, M: 'a>(color: Color, look: Look) -> Element<'a, M> {
    let color = style::color(color);
    container(space().width(1.0).height(look.size))
        .style(move |_| container::Style {
            background: Some(color.into()),
            ..container::Style::default()
        })
        .into()
}

pub fn view<'a, M: Clone + 'a>(
    bar: &TabBar,
    look: Look,
    on: impl Fn(TabMessage) -> M + Copy + 'a,
) -> Element<'a, M> {
    let colors = colors(&look.faces);
    let mut tabs: Vec<Element<'a, M>> = Vec::new();
    for (index, name) in bar.tabs.iter().enumerate() {
        if index > 0 {
            let touching = [index - 1, index]
                .iter()
                .any(|at| bar.selected == Some(*at) || look.hovered == Some(*at));
            if touching {
                tabs.push(space().width(1.0).into());
            } else {
                tabs.push(rule(colors.rule, look));
            }
        }
        tabs.push(tab(
            index,
            name,
            bar.selected == Some(index),
            look,
            colors,
            on,
        ));
    }
    let new = mouse_area(
        container(label(NEW, colors.text, look)).padding([look.size * 0.3, look.size * 0.8]),
    )
    .on_press(on(TabMessage::New))
    .interaction(mouse::Interaction::Pointer);
    tabs.push(new.into());
    let strip = style::color(colors.strip);
    mouse_area(
        container(Row::with_children(tabs).align_y(Alignment::Center))
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(strip.into()),
                ..container::Style::default()
            }),
    )
    .on_release(on(TabMessage::BarReleased))
    .into()
}

#[cfg(test)]
mod tests {
    use oxmacs_core::theme::{ADWAITA, ADWAITA_DARK};

    use super::*;

    #[test]
    fn tab_labels_stay_readable_on_the_strip_and_the_selected_tab() {
        for faces in [ADWAITA, ADWAITA_DARK] {
            let colors = colors(&faces);

            for (part, color, under) in [
                ("tab", colors.text, colors.strip),
                ("hovered tab", colors.text, colors.hovered),
                ("selected tab", colors.current, colors.selected),
            ] {
                let ratio = color.contrast_ratio(under);
                assert!(ratio >= 4.5, "{} {part}: {ratio:.2}", faces.name);
            }
            assert_eq!(colors.selected, faces.default.background);
            assert_ne!(colors.strip, colors.selected);
        }
    }
}
