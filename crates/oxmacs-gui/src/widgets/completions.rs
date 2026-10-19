use std::ops::Range;

use iced::widget::{Column, container, row, space, text};
use iced::{Element, Font, Length};
use oxmacs_core::editor::Editor;
use oxmacs_core::theme::{Color, Theme as Faces};

use crate::style;

const ROWS: usize = 10;
const LABEL_SHARE: u16 = 1;
const SUMMARY_SHARE: u16 = 1;
const SUMMARY_GAP: f32 = 24.0;
const DETAIL_FADE: f32 = 0.4;

pub fn view<'a, Message: 'a>(
    editor: &'a Editor,
    faces: Faces,
    font: Font,
    size: f32,
    opacity: f32,
) -> Element<'a, Message> {
    let Some(minibuffer) = editor.minibuffer() else {
        return Column::new().into();
    };
    let color = |color| style::color(color).scale_alpha(opacity);
    let completions = editor.completions();
    let selected = minibuffer.selected();
    let range = visible_range(completions.len(), selected, ROWS);
    let rows = completions[range.clone()]
        .iter()
        .zip(range)
        .map(|(completion, index)| {
            let current = index == selected;
            let face = if current {
                faces.completion_current
            } else {
                faces.default
            };
            let detail = detail_color(&faces, current);
            let label = row![
                text(completion.label.clone())
                    .font(font)
                    .size(size)
                    .color(color(face.foreground)),
                space::horizontal(),
                text(completion.annotation.clone().unwrap_or_default())
                    .font(font)
                    .size(size)
                    .color(color(detail)),
            ];
            let line: Element<'a, Message> = match completion.summary {
                Some(summary) => row![
                    label.width(Length::FillPortion(LABEL_SHARE)),
                    container(
                        text(summary)
                            .font(font)
                            .size(size)
                            .color(color(detail))
                            .wrapping(text::Wrapping::None)
                    )
                    .padding([0.0, SUMMARY_GAP])
                    .width(Length::FillPortion(SUMMARY_SHARE))
                    .clip(true),
                ]
                .into(),
                None => label.into(),
            };
            let background = color(face.background);
            container(line)
                .width(Length::Fill)
                .padding([0, 4])
                .style(move |_| container::Style {
                    background: current.then(|| background.into()),
                    ..container::Style::default()
                })
                .into()
        });
    Column::with_children(rows).into()
}

fn detail_color(faces: &Faces, current: bool) -> Color {
    if current {
        faces.completion_current.foreground
    } else {
        faces
            .default
            .foreground
            .mix(faces.default.background, DETAIL_FADE)
    }
}

pub fn visible_range(len: usize, selected: usize, rows: usize) -> Range<usize> {
    let start = (selected + 1)
        .saturating_sub(rows)
        .min(len.saturating_sub(rows));
    start..(start + rows).min(len)
}

#[cfg(test)]
mod tests {
    use oxmacs_core::theme::{ADWAITA, ADWAITA_DARK};

    use super::*;

    #[test]
    fn details_stay_readable_on_every_row() {
        for faces in [ADWAITA, ADWAITA_DARK] {
            let current = detail_color(&faces, true);
            let other = detail_color(&faces, false);

            assert!(current.contrast_ratio(faces.completion_current.background) >= 3.0);
            assert!(other.contrast_ratio(faces.default.background) >= 3.0);
        }
    }

    #[test]
    fn short_lists_are_shown_whole() {
        assert_eq!(visible_range(3, 0, 10), 0..3);
    }

    #[test]
    fn selection_near_the_top_shows_the_first_rows() {
        assert_eq!(visible_range(30, 4, 10), 0..10);
    }

    #[test]
    fn selection_past_the_window_scrolls_it() {
        assert_eq!(visible_range(30, 12, 10), 3..13);
        assert_eq!(visible_range(30, 29, 10), 20..30);
    }

    #[test]
    fn empty_lists_show_nothing() {
        assert_eq!(visible_range(0, 0, 10), 0..0);
    }
}
