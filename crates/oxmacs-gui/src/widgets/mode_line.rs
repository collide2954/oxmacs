use iced::alignment::Vertical;
use iced::widget::{Row, Text, container, row, text};
use iced::{Border, Element, Font, Length, Padding, font};
use oxmacs_core::config::ModeLineStyle;
use oxmacs_core::editor::{BufferState, Status};
use oxmacs_core::theme::{Color, Theme as Faces};

use crate::style;
use crate::widgets::arrow::{Direction, arrow};

const GAP: f32 = 8.0;
const PILL_GAP: f32 = 4.0;
const PILL_PADDING: [u16; 2] = [0, 8];
const BLOCK_PADDING: [u16; 2] = [0, 10];
const LINE_HEIGHT: f32 = 1.3;
const FRAME: f32 = 1.0;

#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub faces: Faces,
    pub active: bool,
    pub font: Font,
    pub size: f32,
}

#[derive(Debug, Clone, Copy)]
struct Colors {
    text: Color,
    dim: Color,
    background: Color,
    accent: Color,
    on_accent: Color,
    pill: Color,
    modified: Color,
}

impl Colors {
    fn new(faces: &Faces, active: bool) -> Self {
        let face = if active {
            faces.mode_line
        } else {
            faces.mode_line_inactive
        };
        Self {
            text: face.foreground,
            dim: face.foreground.mix(face.background, 0.3),
            background: face.background,
            accent: if active {
                faces.link
            } else {
                face.background.mix(face.foreground, 0.25)
            },
            on_accent: if active {
                faces.default.background
            } else {
                face.foreground
            },
            pill: face.background.mix(face.foreground, 0.12),
            modified: faces.warning,
        }
    }
}

pub fn view<'a, Message: 'a>(
    status: &Status,
    extras: &[String],
    style: ModeLineStyle,
    look: Look,
) -> Element<'a, Message> {
    let colors = Colors::new(&look.faces, look.active);
    let (left, right) = match style {
        ModeLineStyle::Classic => (
            look.label(status.to_string(), colors.text).into(),
            spaced(
                extras
                    .iter()
                    .map(|extra| look.label(extra.clone(), colors.text).into()),
                GAP,
            ),
        ),
        ModeLineStyle::Modern => modern(status, extras, &look, &colors),
        ModeLineStyle::Minimal => minimal(status, extras, &look, &colors),
        ModeLineStyle::Boxed => boxed(status, extras, &look, &colors),
        ModeLineStyle::Powerline => powerline(status, extras, &look, &colors),
    };
    let padding = if style == ModeLineStyle::Powerline {
        Padding::ZERO
    } else {
        Padding::from([FRAME, 4.0])
    };
    let Look { faces, active, .. } = look;
    container(row![container(left).width(Length::Fill).clip(true), right].align_y(Vertical::Center))
        .width(Length::Fill)
        .padding(padding)
        .style(move |_| style::mode_line(&faces, active))
        .into()
}

impl Look {
    fn label<'a>(&self, content: String, color: Color) -> Text<'a> {
        text(content)
            .font(self.font)
            .size(self.size)
            .color(style::color(color))
            .wrapping(text::Wrapping::None)
    }

    fn bold<'a>(&self, content: String, color: Color) -> Text<'a> {
        self.label(content, color).font(Font {
            weight: font::Weight::Bold,
            ..self.font
        })
    }

    fn height(&self) -> f32 {
        self.size * LINE_HEIGHT + 2.0 * FRAME
    }
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

fn marker_color(state: BufferState, colors: &Colors) -> Color {
    if state == BufferState::ReadOnly {
        colors.dim
    } else {
        colors.modified
    }
}

fn details(status: &Status) -> Vec<String> {
    let mut details: Vec<String> = status.lint.iter().cloned().collect();
    if status.crlf {
        details.push("CRLF".to_owned());
    }
    details.extend(status.mode.map(str::to_owned));
    details.push(position(status));
    details
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

fn name<'a, Message: 'a>(status: &Status, look: &Look, colors: &Colors) -> Element<'a, Message> {
    titled(
        status,
        look,
        colors.text,
        marker_color(status.state, colors),
    )
}

fn titled<'a, Message: 'a>(
    status: &Status,
    look: &Look,
    color: Color,
    marker_color: Color,
) -> Element<'a, Message> {
    let marker =
        marker(status.state).map(|marker| look.label(marker.to_owned(), marker_color).into());
    spaced(
        marker
            .into_iter()
            .chain([look.bold(status.name.clone(), color).into()]),
        PILL_GAP + 2.0,
    )
}

fn extras_labels<'a, Message: 'a>(
    extras: &[String],
    look: &Look,
    color: Color,
) -> impl Iterator<Item = Element<'a, Message>> {
    extras
        .iter()
        .map(move |extra| look.label(extra.clone(), color).into())
        .collect::<Vec<_>>()
        .into_iter()
}

fn modern<'a, Message: 'a>(
    status: &Status,
    extras: &[String],
    look: &Look,
    colors: &Colors,
) -> (Element<'a, Message>, Element<'a, Message>) {
    let mut right: Vec<Element<'a, Message>> = Vec::new();
    for (index, detail) in details(status).into_iter().enumerate() {
        if index > 0 {
            right.push(look.label("·".to_owned(), colors.dim).into());
        }
        right.push(look.label(detail, colors.dim).into());
    }
    right.extend(extras_labels(extras, look, colors.text));
    (name(status, look, colors), spaced(right, GAP))
}

fn minimal<'a, Message: 'a>(
    status: &Status,
    extras: &[String],
    look: &Look,
    colors: &Colors,
) -> (Element<'a, Message>, Element<'a, Message>) {
    let right = [look.label(position(status), colors.dim).into()]
        .into_iter()
        .chain(extras_labels(extras, look, colors.text));
    (name(status, look, colors), spaced(right, GAP))
}

fn pill<'a, Message: 'a>(content: Element<'a, Message>, colors: &Colors) -> Element<'a, Message> {
    let fill = style::color(colors.pill);
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
    look: &Look,
    colors: &Colors,
) -> (Element<'a, Message>, Element<'a, Message>) {
    let mut left = vec![
        pill(name(status, look, colors), colors),
        pill(look.label(position(status), colors.text).into(), colors),
    ];
    let rest = status
        .mode
        .map(str::to_owned)
        .into_iter()
        .chain(status.lint.clone())
        .chain(status.crlf.then(|| "CRLF".to_owned()));
    left.extend(rest.map(|detail| pill(look.label(detail, colors.text).into(), colors)));
    let right = extras_labels(extras, look, colors.text).map(|extra| pill(extra, colors));
    (spaced(left, PILL_GAP), spaced(right, PILL_GAP))
}

fn block<'a, Message: 'a>(
    content: Element<'a, Message>,
    background: Color,
    look: &Look,
) -> Element<'a, Message> {
    let fill = style::color(background);
    container(content)
        .padding(BLOCK_PADDING)
        .height(Length::Fixed(look.height()))
        .align_y(Vertical::Center)
        .style(move |_| container::Style {
            background: Some(fill.into()),
            ..container::Style::default()
        })
        .into()
}

fn powerline<'a, Message: 'a>(
    status: &Status,
    extras: &[String],
    look: &Look,
    colors: &Colors,
) -> (Element<'a, Message>, Element<'a, Message>) {
    let height = look.height();
    let edge = |direction, color, behind| {
        arrow(direction, style::color(color), style::color(behind), height)
    };
    let title = titled(status, look, colors.on_accent, colors.on_accent);
    let mut left = vec![block(title, colors.accent, look)];
    match status.mode {
        Some(mode) => {
            left.push(edge(Direction::Right, colors.accent, colors.pill));
            left.push(block(
                look.label(mode.to_owned(), colors.text).into(),
                colors.pill,
                look,
            ));
            left.push(edge(Direction::Right, colors.pill, colors.background));
        }
        None => left.push(edge(Direction::Right, colors.accent, colors.background)),
    }
    left.extend(
        status
            .lint
            .iter()
            .chain(status.crlf.then(|| "CRLF".to_owned()).as_ref())
            .map(|detail| {
                container(look.label(detail.clone(), colors.dim))
                    .padding([0.0, GAP])
                    .into()
            }),
    );
    let mut right = Vec::new();
    let mut behind = colors.background;
    if !extras.is_empty() {
        right.push(edge(Direction::Left, colors.pill, behind));
        right.push(block(
            spaced(extras_labels(extras, look, colors.text), GAP),
            colors.pill,
            look,
        ));
        behind = colors.pill;
    }
    right.push(edge(Direction::Left, colors.accent, behind));
    right.push(block(
        look.label(position(status), colors.on_accent).into(),
        colors.accent,
        look,
    ));
    (spaced(left, 0.0), spaced(right, 0.0))
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
            let background = faces.mode_line.background;

            assert!(
                colors.dim.contrast_ratio(background) >= 3.0,
                "{}",
                faces.name
            );
            assert!(
                colors.modified.contrast_ratio(background) >= 3.0,
                "{}",
                faces.name
            );
            assert!(
                colors.text.contrast_ratio(colors.pill) >= 4.5,
                "{}",
                faces.name
            );
            assert!(
                colors.on_accent.contrast_ratio(colors.accent) >= 4.5,
                "{}",
                faces.name
            );
        }
    }
}
