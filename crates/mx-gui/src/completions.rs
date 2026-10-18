use std::ops::Range;

use iced::widget::{Column, container, row, space, text};
use iced::{Element, Font, Length};
use mx_core::editor::Editor;
use mx_core::theme::Theme as Faces;

use crate::style;

const ROWS: usize = 10;

pub fn view<'a, Message: 'a>(
    editor: &'a Editor,
    faces: Faces,
    font: Font,
    size: f32,
) -> Element<'a, Message> {
    let Some(minibuffer) = editor.minibuffer() else {
        return Column::new().into();
    };
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
            let line = row![
                text(completion.command.name())
                    .font(font)
                    .size(size)
                    .color(style::color(face.foreground)),
                space::horizontal(),
                text(completion.keys.clone().unwrap_or_default())
                    .font(font)
                    .size(size)
                    .color(style::color(faces.shadow)),
            ];
            container(line)
                .width(Length::Fill)
                .padding([0, 4])
                .style(move |_| container::Style {
                    background: current.then(|| style::color(face.background).into()),
                    ..container::Style::default()
                })
                .into()
        });
    Column::with_children(rows).into()
}

pub fn visible_range(len: usize, selected: usize, rows: usize) -> Range<usize> {
    let start = (selected + 1)
        .saturating_sub(rows)
        .min(len.saturating_sub(rows));
    start..(start + rows).min(len)
}

#[cfg(test)]
mod tests {
    use super::*;

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
