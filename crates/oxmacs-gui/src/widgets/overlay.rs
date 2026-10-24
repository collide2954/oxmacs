use iced::alignment::{Horizontal, Vertical};
use iced::font::Family;
use iced::widget::{Column, Row, Stack, container, text};
use iced::{Border, Element, Font, Length, Padding, Shadow, Vector};
use oxmacs_core::theme::{Color, Theme as Faces};
use oxmacs_ox::overlays::{Look, Overlay, Piece, Place, Row as Line};

use crate::style;

const UI_FONT: Font = Font {
    family: Family::SansSerif,
    ..Font::DEFAULT
};
const BASE: f32 = 16.0;
const MARGIN: f32 = 12.0;
const ROW_GAP: f32 = 6.0;
const PIECE_GAP: f32 = 5.0;
const PILL_PADDING: [f32; 2] = [5.0, 12.0];
const CAP_PADDING: [f32; 2] = [0.0, 6.0];
const CAP_RADIUS: f32 = 5.0;
const CAP_DEPTH: f32 = 2.0;
const PILL_TINT: f32 = 0.08;
const CAP_TINT: f32 = 0.2;
const CAP_EDGE: f32 = 0.38;
const SHADE: f32 = 0.35;

#[derive(Debug, Clone, Copy)]
struct Colors {
    pill: Color,
    edge: Color,
    cap: Color,
    cap_edge: Color,
    text: Color,
}

impl Colors {
    fn new(faces: &Faces) -> Self {
        let background = faces.default.background;
        let foreground = faces.default.foreground;
        Self {
            pill: background.mix(foreground, PILL_TINT),
            edge: faces.mode_line_border,
            cap: background.mix(foreground, CAP_TINT),
            cap_edge: background.mix(foreground, CAP_EDGE),
            text: foreground,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Paint {
    faces: Faces,
    colors: Colors,
    font: Font,
    size: f32,
    opacity: f32,
}

impl Paint {
    fn color(&self, color: Color) -> iced::Color {
        style::color(color).scale_alpha(self.opacity)
    }

    fn scaled(&self, length: f32) -> f32 {
        length * self.size / BASE
    }
}

pub fn view<'a, Message: 'a>(
    overlays: &[&'a Overlay],
    faces: Faces,
    font: Font,
    size: f32,
    above: f32,
) -> Element<'a, Message> {
    let colors = Colors::new(&faces);
    let layers = [Place::BottomRight, Place::BottomCenter]
        .into_iter()
        .filter_map(|place| {
            let pills: Vec<Element<'a, Message>> = overlays
                .iter()
                .filter(|overlay| overlay.place == place)
                .flat_map(|overlay| {
                    overlay.rows.iter().map(move |line| {
                        pill(
                            line,
                            Paint {
                                faces,
                                colors,
                                font,
                                size: size * overlay.scale,
                                opacity: line.opacity,
                            },
                        )
                    })
                })
                .collect();
            (!pills.is_empty()).then(|| corner(place, pills, above))
        });
    Stack::with_children(layers)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn corner<'a, Message: 'a>(
    place: Place,
    pills: Vec<Element<'a, Message>>,
    above: f32,
) -> Element<'a, Message> {
    let across = match place {
        Place::BottomRight => Horizontal::Right,
        Place::BottomCenter => Horizontal::Center,
    };
    container(
        Column::with_children(pills)
            .spacing(ROW_GAP)
            .align_x(across),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(across)
    .align_y(Vertical::Bottom)
    .padding(Padding {
        top: MARGIN,
        right: MARGIN,
        bottom: above + MARGIN,
        left: MARGIN,
    })
    .into()
}

fn pill<'a, Message: 'a>(line: &'a Line, paint: Paint) -> Element<'a, Message> {
    let pieces = line.pieces.iter().map(|shown| piece(shown, paint));
    let fill = paint.color(paint.colors.pill);
    let edge = paint.color(paint.colors.edge);
    let shade = iced::Color::BLACK.scale_alpha(SHADE * paint.opacity);
    let depth = paint.scaled(CAP_DEPTH);
    container(
        Row::with_children(pieces)
            .spacing(paint.scaled(PIECE_GAP))
            .align_y(Vertical::Center),
    )
    .padding(PILL_PADDING.map(|length| paint.scaled(length)))
    .style(move |_| container::Style {
        background: Some(fill.into()),
        border: Border {
            color: edge,
            width: 1.0,
            radius: f32::MAX.into(),
        },
        shadow: Shadow {
            color: shade,
            offset: Vector::new(0.0, depth),
            blur_radius: 4.0 * depth,
        },
        ..container::Style::default()
    })
    .into()
}

fn piece<'a, Message: 'a>(shown: &'a Piece, paint: Paint) -> Element<'a, Message> {
    let label = |font: Font, color: Color| {
        text(shown.text.as_str())
            .font(font)
            .size(paint.size)
            .color(paint.color(color))
            .wrapping(text::Wrapping::None)
    };
    match shown.look {
        Look::Keycap => keycap(label(paint.font, paint.colors.text).into(), paint),
        Look::Plain => label(UI_FONT, paint.colors.text).into(),
        Look::Face(highlight) => label(UI_FONT, paint.faces.highlight(highlight)).into(),
    }
}

fn keycap<'a, Message: 'a>(label: Element<'a, Message>, paint: Paint) -> Element<'a, Message> {
    let fill = paint.color(paint.colors.cap);
    let edge = paint.color(paint.colors.cap_edge);
    let depth = paint.scaled(CAP_DEPTH);
    let radius = paint.scaled(CAP_RADIUS);
    container(label)
        .padding(CAP_PADDING.map(|length| paint.scaled(length)))
        .style(move |_| container::Style {
            background: Some(fill.into()),
            border: Border {
                color: edge,
                width: 1.0,
                radius: radius.into(),
            },
            shadow: Shadow {
                color: edge,
                offset: Vector::new(0.0, depth),
                blur_radius: 0.0,
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use iced::Element;
    use oxmacs_core::theme::{ADWAITA, ADWAITA_DARK, Highlight};
    use oxmacs_ox::overlays::{Look, Overlay, Piece, Place, Row};

    use super::*;
    use crate::fonts;

    fn overlay(place: Place) -> Overlay {
        let piece = |text: &str, look| Piece {
            text: text.to_owned(),
            look,
        };
        Overlay {
            place,
            scale: 1.0,
            rows: vec![
                Row {
                    pieces: vec![piece("C-n", Look::Keycap), piece("next-line", Look::Plain)],
                    opacity: 0.4,
                },
                Row {
                    pieces: vec![
                        piece("C-x", Look::Keycap),
                        piece("C-s", Look::Keycap),
                        piece("×2", Look::Face(Highlight::Comment)),
                        piece("save-buffer", Look::Plain),
                    ],
                    opacity: 1.0,
                },
            ],
        }
    }

    #[test]
    fn overlays_draw_their_keycaps_and_text() {
        let right = overlay(Place::BottomRight);
        let center = overlay(Place::BottomCenter);
        let shown = [&right, &center];
        let element: Element<'_, ()> = view(&shown, ADWAITA_DARK, fonts::EDITOR, 16.0, 30.0);
        let mut ui = iced_test::simulator(element);

        for text in ["C-x", "C-s", "×2", "save-buffer", "next-line"] {
            assert!(ui.find(text).is_ok(), "{text}");
        }
    }

    #[test]
    fn keycaps_stand_out_from_the_pill_and_stay_readable() {
        for faces in [ADWAITA, ADWAITA_DARK] {
            let colors = Colors::new(&faces);

            assert!(colors.cap.contrast_ratio(colors.pill) >= 1.15);
            assert!(colors.text.contrast_ratio(colors.cap) >= 4.5);
            assert!(colors.text.contrast_ratio(colors.pill) >= 4.5);
        }
    }
}
