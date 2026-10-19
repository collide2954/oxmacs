use iced::widget::{
    Column, Row, column, container, markdown, mouse_area, responsive, row, scrollable, sensor,
    space,
};
use iced::{Element, Length};
use oxmacs_core::clock;
use oxmacs_core::editor::{Axis, Tiling, WindowId};
use oxmacs_core::theme::Theme as Faces;

use super::{App, Message, preview_id};
use crate::widgets::buffer_view::{BufferView, Motion};
use crate::widgets::divider::Divider;
use crate::widgets::mode_line::{self, Look};
use crate::widgets::{completions, echo_area, nick_list};
use crate::{style, units};
impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let faces = self.faces();
        column![
            self.tile(self.editor.tiling(), Vec::new(), faces),
            echo_area::view(&self.editor, faces, self.font, self.text_size),
            completions::view(
                &self.editor,
                faces,
                self.font,
                self.text_size,
                self.completion_opacity()
            ),
        ]
        .into()
    }

    fn tile<'a>(
        &'a self,
        tiling: &'a Tiling,
        path: Vec<usize>,
        faces: Faces,
    ) -> Element<'a, Message> {
        let Tiling::Split { axis, parts } = tiling else {
            let Tiling::Window(window) = tiling else {
                return space().into();
            };
            return self.window(*window, faces);
        };
        let axis = *axis;
        responsive(move |size| {
            let extent = match axis {
                Axis::Rows => size.height,
                Axis::Columns => size.width,
            };
            let mut children: Vec<Element<'a, Message>> = Vec::new();
            for (index, (weight, part)) in parts.iter().enumerate() {
                if index > 0 {
                    let at = path.clone();
                    let border = style::color(faces.mode_line_border);
                    children.push(
                        Divider::new(axis, extent, border, move |delta| Message::Divider {
                            path: at.clone(),
                            index: index - 1,
                            delta,
                        })
                        .into(),
                    );
                }
                let mut inner = path.clone();
                inner.push(index);
                let share = Length::FillPortion(units::portion(*weight));
                let part = container(self.tile(part, inner, faces));
                children.push(match axis {
                    Axis::Rows => part.width(Length::Fill).height(share).into(),
                    Axis::Columns => part.width(share).height(Length::Fill).into(),
                });
            }
            match axis {
                Axis::Rows => Column::with_children(children).into(),
                Axis::Columns => Row::with_children(children).into(),
            }
        })
        .into()
    }

    fn window(&self, window: WindowId, faces: Faces) -> Element<'_, Message> {
        let Some(pane) = self.editor.pane_of(window) else {
            return space().into();
        };
        let active = pane.is_selected();
        let body: Element<'_, Message> = match pane
            .preview_source()
            .and_then(|source| self.previews.iter().find(|(id, ..)| *id == source.id()))
        {
            Some((_, _, content)) => {
                let settings = markdown::Settings::with_text_size(
                    self.text_size,
                    markdown::Style::from_palette(self.theme().palette()),
                );
                let rendered = scrollable(
                    container(markdown::view(content.items(), settings).map(Message::OpenLink))
                        .padding(12),
                )
                .id(preview_id(window))
                .height(Length::Fill);
                let sized = sensor(rendered)
                    .on_show(move |size| Message::PreviewSized(window, size.height))
                    .on_resize(move |size| Message::PreviewSized(window, size.height));
                mouse_area(sized)
                    .on_press(Message::FocusWindow(window))
                    .into()
            }
            None => container(BufferView::new(
                pane,
                window,
                faces,
                self.font,
                self.text_size,
                Motion {
                    enabled: self.animations,
                    last_input: self.last_input,
                    blink_interval: self.blink_interval,
                },
                Message::View,
            ))
            .padding([0, 4])
            .height(Length::Fill)
            .into(),
        };
        let body = match pane.members() {
            Some(members) => {
                let own = pane.own_nick();
                row![
                    body,
                    nick_list::view(&members, own.as_deref(), faces, self.text_size)
                ]
                .into()
            }
            None => body,
        };
        let last = self.editor.tiling().windows().last() == Some(&window);
        let weather = self
            .editor
            .weather_brief()
            .filter(|_| last && self.editor.config().mode_line_weather);
        let time = self.clock.as_deref().filter(|_| last).map(clock::now);
        let extras: Vec<String> = weather.into_iter().chain(time).collect();
        let look = Look {
            faces,
            active,
            font: self.font,
            size: self.text_size,
        };
        column![
            body,
            mode_line::view(&pane.status(), &extras, self.mode_line_style, look)
        ]
        .into()
    }
}
