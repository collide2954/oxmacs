use iced::widget::text::Span;
use iced::widget::{Column, Row, container, rich_text, span, text};
use iced::{Border, Element, Font, Length};
use oxmacs_core::editor::{Editor, EntryView, GroupView};
use oxmacs_core::theme::{Color, Theme as Faces};

use crate::style;

const FADE: f32 = 0.35;
const SURFACE: f32 = 0.05;
const GROUP_GAP: f32 = 40.0;
const ROW_GAP: f32 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Colors {
    surface: Color,
    title: Color,
    heading: Color,
    key: Color,
    text: Color,
    on: Color,
    off: Color,
}

fn colors(faces: &Faces) -> Colors {
    Colors {
        surface: faces
            .default
            .background
            .mix(faces.default.foreground, SURFACE),
        title: faces.minibuffer_prompt,
        heading: faces.heading,
        key: faces.syntax.keyword,
        text: faces.default.foreground,
        on: faces.success,
        off: faces.default.foreground.mix(faces.default.background, FADE),
    }
}

fn entry<'a, Message: 'a>(
    entry: &EntryView,
    colors: Colors,
    font: Font,
    size: f32,
) -> Element<'a, Message> {
    let mut spans: Vec<Span<'a, (), Font>> = vec![
        span(format!("{:>3} ", entry.key)).color(style::color(colors.key)),
        span(entry.summary).color(style::color(colors.text)),
    ];
    if let Some(flag) = entry.flag {
        let color = if entry.on { colors.on } else { colors.off };
        spans.push(span(format!(" ({flag})")).color(style::color(color)));
    }
    rich_text(spans).font(font).size(size).into()
}

fn group<'a, Message: 'a>(
    group: &GroupView,
    colors: Colors,
    font: Font,
    size: f32,
) -> Element<'a, Message> {
    let title = text(group.title)
        .font(font)
        .size(size)
        .color(style::color(colors.heading));
    Column::with_children(
        std::iter::once(title.into()).chain(
            group
                .entries
                .iter()
                .map(|item| entry(item, colors, font, size)),
        ),
    )
    .into()
}

pub fn view<'a, Message: 'a>(
    editor: &'a Editor,
    faces: Faces,
    font: Font,
    size: f32,
) -> Element<'a, Message> {
    let Some(view) = editor.transient() else {
        return Column::new().into();
    };
    let colors = colors(&faces);
    let title = if view.switching {
        format!("{}  -", view.title)
    } else {
        view.title.to_owned()
    };
    let (switches, actions): (Vec<&GroupView>, Vec<&GroupView>) = view
        .groups
        .iter()
        .partition(|group| group.entries.iter().any(|item| item.flag.is_some()));
    let mut rows: Vec<Element<'a, Message>> = vec![
        text(title)
            .font(font)
            .size(size)
            .color(style::color(colors.title))
            .into(),
    ];
    rows.extend(
        switches
            .into_iter()
            .map(|item| group(item, colors, font, size)),
    );
    rows.push(
        Row::with_children(
            actions
                .into_iter()
                .map(|item| group(item, colors, font, size)),
        )
        .spacing(GROUP_GAP)
        .into(),
    );
    let surface = style::color(colors.surface);
    let border = style::color(faces.mode_line_border);
    container(Column::with_children(rows).spacing(size * ROW_GAP))
        .padding([6, 10])
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(surface.into()),
            border: Border {
                color: border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use oxmacs_core::theme::{ADWAITA, ADWAITA_DARK};

    use super::*;

    #[test]
    fn every_part_of_the_panel_stays_readable() {
        for faces in [ADWAITA, ADWAITA_DARK] {
            let colors = colors(&faces);

            for (part, color) in [
                ("title", colors.title),
                ("heading", colors.heading),
                ("key", colors.key),
                ("text", colors.text),
                ("on", colors.on),
                ("off", colors.off),
            ] {
                let ratio = color.contrast_ratio(colors.surface);
                assert!(ratio >= 3.0, "{} {part}: {ratio:.2}", faces.name);
            }
            assert_ne!(colors.surface, faces.default.background);
            assert_ne!(colors.on, colors.off);
        }
    }
}
