use iced::alignment::Vertical;
use iced::font::{Family, Weight};
use iced::widget::{Row, Text, column, container, row, space, text};
use iced::{Border, Element, Font, Length, Padding};
use oxmacs_core::config::ModeLineStyle;
use oxmacs_core::editor::{BufferState, Status};
use oxmacs_core::theme::{Color, Theme as Faces};

use crate::style;
use crate::widgets::powerline::{Direction, Powerline};

const UI_FONT: Font = Font {
    family: Family::SansSerif,
    ..Font::DEFAULT
};
const LINE_HEIGHT: f32 = 1.3;
const BREATH: f32 = 4.0;
const EDGE: f32 = 1.0;
const SIDE: f32 = 8.0;
const GAP: f32 = 8.0;
const PILL_GAP: f32 = 4.0;
const PILL_PADDING: [f32; 2] = [1.0, 9.0];
const BLOCK_SIDE: f32 = 10.0;
const SECONDARY: f32 = 0.9;
const TINT: f32 = 0.12;
const BLOCK_TINT: f32 = 0.25;
const DIM: f32 = 0.35;
const FADE: f32 = 0.55;

#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub faces: Faces,
    pub active: bool,
    pub font: Font,
    pub size: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Colors {
    text: Color,
    dim: Color,
    background: Color,
    accent: Color,
    on_accent: Color,
    tint: Color,
    block: Color,
    edge: Color,
}

impl Colors {
    fn new(faces: &Faces, active: bool) -> Self {
        let face = faces.mode_line;
        let colors = Self {
            text: face.foreground,
            dim: face.foreground.mix(face.background, DIM),
            background: face.background,
            accent: faces.link,
            on_accent: faces.default.background,
            tint: face.background.mix(faces.link, TINT),
            block: face.background.mix(faces.link, BLOCK_TINT),
            edge: faces.mode_line_border,
        };
        if active {
            colors
        } else {
            colors.faded(faces.mode_line_inactive.background)
        }
    }

    fn faded(self, background: Color) -> Self {
        let fade = |color: Color| color.mix(background, FADE);
        Self {
            text: fade(self.text),
            dim: fade(self.dim),
            background,
            accent: fade(self.accent),
            on_accent: fade(self.on_accent),
            tint: fade(self.tint),
            block: fade(self.block),
            edge: self.edge,
        }
    }
}

struct Paint {
    look: Look,
    colors: Colors,
}

impl Paint {
    fn label<'a>(&self, content: String, color: Color) -> Text<'a> {
        text(content)
            .font(UI_FONT)
            .size(self.look.size)
            .color(style::color(color))
            .wrapping(text::Wrapping::None)
    }

    fn bold<'a>(&self, content: String, color: Color) -> Text<'a> {
        self.label(content, color).font(Font {
            weight: Weight::Bold,
            ..UI_FONT
        })
    }

    fn small<'a>(&self, content: String, color: Color) -> Text<'a> {
        self.label(content, color).size(self.look.size * SECONDARY)
    }

    fn digits<'a>(&self, content: String, color: Color) -> Text<'a> {
        self.small(content, color).font(self.look.font)
    }

    fn text_height(&self) -> f32 {
        self.look.size * LINE_HEIGHT
    }

    fn bar_height(&self) -> f32 {
        self.text_height() + 2.0 * BREATH
    }
}

pub fn view<'a, Message: 'a>(
    status: &Status,
    extras: &[String],
    style: ModeLineStyle,
    look: Look,
) -> Element<'a, Message> {
    let paint = Paint {
        look,
        colors: Colors::new(&look.faces, look.active),
    };
    match style {
        ModeLineStyle::Classic => classic(status, extras, look),
        ModeLineStyle::Modern => framed(modern(status, extras, &paint), &paint, SIDE),
        ModeLineStyle::Minimal => framed(minimal(status, extras, &paint), &paint, SIDE),
        ModeLineStyle::Boxed => framed(boxed(status, extras, &paint), &paint, SIDE / 2.0),
        ModeLineStyle::Powerline => framed(powerline(status, extras, &paint), &paint, 0.0),
    }
}

fn classic<'a, Message: 'a>(
    status: &Status,
    extras: &[String],
    look: Look,
) -> Element<'a, Message> {
    let line = |content: String| {
        text(content)
            .font(look.font)
            .size(look.size)
            .wrapping(text::Wrapping::None)
    };
    let right = extras.iter().map(|extra| line(extra.clone()).into());
    let Look { faces, active, .. } = look;
    container(
        row![
            container(line(status.to_string()))
                .width(Length::Fill)
                .clip(true),
            spaced(right, GAP)
        ]
        .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([1, 4])
    .style(move |_| style::mode_line(&faces, active))
    .into()
}

fn filled(color: Color) -> impl Fn(&iced::Theme) -> container::Style {
    let color = style::color(color);
    move |_| container::Style {
        background: Some(color.into()),
        ..container::Style::default()
    }
}

fn framed<'a, Message: 'a>(
    (left, right): (Element<'a, Message>, Element<'a, Message>),
    paint: &Paint,
    side: f32,
) -> Element<'a, Message> {
    column![
        container(space())
            .width(Length::Fill)
            .height(Length::Fixed(EDGE))
            .style(filled(paint.colors.edge)),
        container(
            row![container(left).width(Length::Fill).clip(true), right].align_y(Vertical::Center)
        )
        .width(Length::Fill)
        .height(Length::Fixed(paint.bar_height()))
        .padding(Padding::from([0.0, side]))
        .align_y(Vertical::Center)
        .style(filled(paint.colors.background)),
    ]
    .into()
}

fn spaced<'a, Message: 'a>(
    items: impl IntoIterator<Item = Element<'a, Message>>,
    gap: f32,
) -> Element<'a, Message> {
    Row::with_children(items)
        .spacing(gap)
        .align_y(Vertical::Center)
        .into()
}

fn divider<'a, Message: 'a>(paint: &Paint) -> Element<'a, Message> {
    container(space())
        .width(Length::Fixed(EDGE))
        .height(Length::Fixed(paint.look.size))
        .style(filled(paint.colors.edge))
        .into()
}

fn position(status: &Status) -> String {
    format!("{}:{}", status.line, status.column)
}

fn marker(state: BufferState) -> Option<&'static str> {
    match state {
        BufferState::Modified | BufferState::UnsavedSettings => Some("●"),
        BufferState::ReadOnly => Some("RO"),
        BufferState::Unmodified => None,
    }
}

fn titled<'a, Message: 'a>(
    status: &Status,
    paint: &Paint,
    color: Color,
    marker_color: Color,
) -> Element<'a, Message> {
    let marker = marker(status.state).map(|marker| {
        let color = if status.state == BufferState::ReadOnly {
            color
        } else {
            marker_color
        };
        paint.small(marker.to_owned(), color).into()
    });
    spaced(
        marker
            .into_iter()
            .chain([paint.bold(status.name.clone(), color).into()]),
        PILL_GAP + 2.0,
    )
}

fn name<'a, Message: 'a>(status: &Status, paint: &Paint) -> Element<'a, Message> {
    titled(status, paint, paint.colors.text, paint.colors.accent)
}

fn extras_after<'a, Message: 'a>(
    mut items: Vec<Element<'a, Message>>,
    extras: &[String],
    paint: &Paint,
) -> Element<'a, Message> {
    if !extras.is_empty() {
        items.push(divider(paint));
        items.extend(
            extras
                .iter()
                .map(|extra| paint.digits(extra.clone(), paint.colors.text).into()),
        );
    }
    spaced(items, GAP)
}

fn modern<'a, Message: 'a>(
    status: &Status,
    extras: &[String],
    paint: &Paint,
) -> (Element<'a, Message>, Element<'a, Message>) {
    let colors = paint.colors;
    let words = status
        .lint
        .iter()
        .cloned()
        .chain(status.crlf.then(|| "CRLF".to_owned()))
        .chain(status.mode.map(str::to_owned));
    let mut details: Vec<Element<'a, Message>> = Vec::new();
    for word in words {
        details.push(paint.small(word, colors.dim).into());
        details.push(paint.small("·".to_owned(), colors.dim).into());
    }
    details.push(paint.digits(position(status), colors.dim).into());
    (name(status, paint), extras_after(details, extras, paint))
}

fn minimal<'a, Message: 'a>(
    status: &Status,
    extras: &[String],
    paint: &Paint,
) -> (Element<'a, Message>, Element<'a, Message>) {
    let details = vec![paint.digits(position(status), paint.colors.dim).into()];
    (name(status, paint), extras_after(details, extras, paint))
}

fn pill<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    paint: &Paint,
) -> Element<'a, Message> {
    let fill = style::color(paint.colors.tint);
    container(content)
        .padding(PILL_PADDING)
        .style(move |_| container::Style {
            background: Some(fill.into()),
            border: Border {
                radius: f32::MAX.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn boxed<'a, Message: 'a>(
    status: &Status,
    extras: &[String],
    paint: &Paint,
) -> (Element<'a, Message>, Element<'a, Message>) {
    let colors = paint.colors;
    let mut left = vec![
        pill(name(status, paint), paint),
        pill(paint.digits(position(status), colors.text), paint),
    ];
    let words = status
        .mode
        .map(str::to_owned)
        .into_iter()
        .chain(status.lint.clone())
        .chain(status.crlf.then(|| "CRLF".to_owned()));
    left.extend(words.map(|word| pill(paint.small(word, colors.text), paint)));
    let right = extras
        .iter()
        .map(|extra| pill(paint.digits(extra.clone(), colors.text), paint));
    (spaced(left, PILL_GAP), spaced(right, PILL_GAP))
}

fn powerline<'a, Message: 'a>(
    status: &Status,
    extras: &[String],
    paint: &Paint,
) -> (Element<'a, Message>, Element<'a, Message>) {
    let colors = paint.colors;
    let height = paint.bar_height();
    let mut blocks = Powerline::new(
        Direction::Right,
        style::color(colors.background),
        height,
        BLOCK_SIDE,
    )
    .push(
        titled(status, paint, colors.on_accent, colors.on_accent),
        style::color(colors.accent),
    );
    if let Some(mode) = status.mode {
        blocks = blocks.push(
            paint.small(mode.to_owned(), colors.text),
            style::color(colors.block),
        );
    }
    let words = status
        .lint
        .iter()
        .cloned()
        .chain(status.crlf.then(|| "CRLF".to_owned()))
        .map(|word| {
            container(paint.small(word, colors.dim))
                .padding([0.0, GAP])
                .into()
        });
    let left = spaced([blocks.into()].into_iter().chain(words), 0.0);
    let mut right = Powerline::new(
        Direction::Left,
        style::color(colors.background),
        height,
        BLOCK_SIDE,
    );
    if !extras.is_empty() {
        right = right.push(
            spaced(
                extras
                    .iter()
                    .map(|extra| paint.small(extra.clone(), colors.text).into()),
                GAP,
            ),
            style::color(colors.block),
        );
    }
    right = right.push(
        paint.small(position(status), colors.on_accent),
        style::color(colors.accent),
    );
    (left, right.into())
}
#[cfg(test)]
mod tests {
    use iced::Font;
    use oxmacs_core::config::ModeLineStyle;
    use oxmacs_core::editor::{BufferState, Status};
    use oxmacs_core::theme::{ADWAITA, ADWAITA_DARK};

    use super::*;

    fn status(state: BufferState) -> Status {
        Status {
            state,
            name: "main.rs".to_owned(),
            line: 12,
            column: 4,
            crlf: false,
            mode: Some("Rust"),
            lint: Some("E1 W2".to_owned()),
        }
    }

    fn shown(style: ModeLineStyle, state: BufferState) -> iced_test::Simulator<'static, ()> {
        let look = Look {
            faces: ADWAITA,
            active: true,
            font: Font::MONOSPACE,
            size: 14.0,
        };
        iced_test::simulator(view(&status(state), &["14:05".to_owned()], style, look))
    }

    #[test]
    fn classic_is_one_emacs_like_line() {
        let mut ui = shown(ModeLineStyle::Classic, BufferState::Modified);

        assert!(ui.find("** main.rs   L12 C4   (Rust)   E1 W2").is_ok());
        assert!(ui.find("14:05").is_ok());
    }

    #[test]
    fn modern_minimal_boxed_and_powerline_split_the_status_into_segments() {
        let everything = ["●", "main.rs", "12:4", "Rust", "E1 W2", "14:05"];
        for style in [
            ModeLineStyle::Modern,
            ModeLineStyle::Boxed,
            ModeLineStyle::Powerline,
        ] {
            let mut ui = shown(style, BufferState::Modified);
            for segment in everything {
                assert!(ui.find(segment).is_ok(), "{style:?} {segment}");
            }
        }
    }

    #[test]
    fn minimal_shows_only_the_name_and_position() {
        let mut ui = shown(ModeLineStyle::Minimal, BufferState::Modified);

        for segment in ["●", "main.rs", "12:4", "14:05"] {
            assert!(ui.find(segment).is_ok(), "{segment}");
        }
        assert!(ui.find("Rust").is_err());
        assert!(ui.find("E1 W2").is_err());
    }

    #[test]
    fn the_dot_marks_unsaved_changes_and_read_only_buffers_say_so() {
        assert!(
            shown(ModeLineStyle::Modern, BufferState::Unmodified)
                .find("●")
                .is_err()
        );
        assert!(
            shown(ModeLineStyle::Minimal, BufferState::UnsavedSettings)
                .find("●")
                .is_ok()
        );
        assert!(
            shown(ModeLineStyle::Boxed, BufferState::ReadOnly)
                .find("RO")
                .is_ok()
        );
    }

    #[test]
    fn derived_colors_stay_readable() {
        for faces in [ADWAITA, ADWAITA_DARK] {
            let colors = Colors::new(&faces, true);
            let readable = |a: Color, b: Color, ratio| {
                assert!(a.contrast_ratio(b) >= ratio, "{}", faces.name);
            };

            readable(colors.dim, colors.background, 3.0);
            readable(colors.accent, colors.background, 3.0);
            readable(colors.text, colors.tint, 4.5);
            readable(colors.text, colors.block, 4.5);
            readable(colors.on_accent, colors.accent, 4.5);
        }
    }

    #[test]
    fn inactive_windows_fade_every_color_but_the_edge() {
        let active = Colors::new(&ADWAITA_DARK, true);
        let inactive = Colors::new(&ADWAITA_DARK, false);
        let background = ADWAITA_DARK.mode_line_inactive.background;

        assert_eq!(inactive.background, background);
        assert_eq!(inactive.text, active.text.mix(background, FADE));
        assert_eq!(inactive.accent, active.accent.mix(background, FADE));
        assert_eq!(inactive.edge, active.edge);
    }
}
