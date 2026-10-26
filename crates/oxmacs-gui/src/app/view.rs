use iced::widget::{
    Column, Row, canvas, column, container, mouse_area, responsive, row, sensor, space, stack,
};
use iced::{Element, Length};
use oxmacs_core::clock;
use oxmacs_core::editor::{Axis, Edge, Pane, Tiling, WindowId};
use oxmacs_core::theme::Theme as Faces;
use oxmacs_ox::takeover::{self, Takeover};
use oxmacs_ox::views::ViewBuffer;
use oxmacs_ox::{games, overlays, tabbar};

use super::{App, Message};
use crate::widgets::buffer_view::{BufferView, Motion};
use crate::widgets::divider::Divider;
use crate::widgets::drawing::Drawing;
use crate::widgets::game::GameScreen;
use crate::widgets::mode_line::{self, Look};
use crate::widgets::{completions, echo_area, overlay, ox_view, tab_bar, transient};
use crate::{style, units};

const SIDE_TINT: f32 = 0.03;
const SIDE_PADDING: f32 = 8.0;
const COLUMN_PADDING: f32 = 8.0;
const COLUMN_MARGIN: f32 = 1.5;

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let faces = self.faces();
        let offered = self.editor.completions();
        let windows = match takeover::current(&self.editor) {
            Some(shown) => self.takeover(shown, faces),
            None => self.beside_side(self.tile(self.editor.tiling(), Vec::new(), faces), faces),
        };
        let floating = overlay::view(
            &overlays::shown(&self.editor),
            faces,
            self.font,
            self.text_size,
            mode_line::height(self.text_size),
        );
        let chrome =
            !self.editor.presentation().hide_chrome && takeover::current(&self.editor).is_none();
        let tabs: Element<'_, Message> = match tabbar::shown(&self.editor) {
            Some(bar) if chrome => tab_bar::view(
                bar,
                tab_bar::Look {
                    faces,
                    font: self.font,
                    size: self.text_size,
                    hovered: self.tab_hover,
                },
                Message::Tab,
            ),
            _ => column![].into(),
        };
        column![
            tabs,
            stack![windows, floating]
                .width(Length::Fill)
                .height(Length::Fill),
            transient::view(&self.editor, faces, self.font, self.text_size),
            echo_area::view(
                &self.editor,
                offered.len(),
                faces,
                self.font,
                self.text_size
            ),
            completions::view(
                &self.editor,
                offered,
                faces,
                self.font,
                self.text_size,
                self.completion_opacity()
            ),
        ]
        .into()
    }

    fn takeover<'a>(&'a self, shown: &'a Takeover, faces: Faces) -> Element<'a, Message> {
        let drawing = canvas(Drawing {
            shapes: &shown.shapes,
            relative: false,
            background: Some(shown.background),
            faces,
            font: self.font,
            size: self.text_size,
        })
        .width(Length::Fill)
        .height(Length::Fill);
        let covered = mouse_area(
            sensor(drawing)
                .on_show(Message::TakeoverSized)
                .on_resize(Message::TakeoverSized),
        )
        .on_press(Message::EndTakeover);
        let Some(pane) = self.editor.pane_of(self.editor.selected_window()) else {
            return covered.into();
        };
        let extras: Vec<String> = self
            .editor
            .mode_line_segments()
            .into_iter()
            .chain(self.clock.as_deref().map(clock::now))
            .collect();
        let look = Look {
            faces,
            active: true,
            font: self.font,
            size: self.text_size,
        };
        column![
            covered,
            mode_line::view(&pane.status(), &extras, self.mode_line_style, look)
        ]
        .into()
    }

    fn beside_side<'a>(&'a self, main: Element<'a, Message>, faces: Faces) -> Element<'a, Message> {
        let Some(side) = self
            .editor
            .side_window()
            .filter(|_| !self.editor.presentation().hide_chrome)
        else {
            return main;
        };
        let Some(pane) = self.editor.pane_of(side.window) else {
            return main;
        };
        let tint = style::color(
            faces
                .default
                .background
                .mix(faces.default.foreground, SIDE_TINT),
        );
        let width = units::to_f32(side.columns) * units::cell_width(self.text_size) + SIDE_PADDING;
        let panel = container(self.body(pane, side.window, faces))
            .width(Length::Fixed(width))
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(tint.into()),
                ..container::Style::default()
            });
        let divider = Divider::new(
            Axis::Columns,
            1.0,
            style::color(faces.mode_line_border),
            Message::SideDragged,
        );
        match side.edge {
            Edge::Left => row![panel, divider, main].into(),
            Edge::Right => row![main, divider, panel].into(),
        }
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
        let body = self.body(pane, window, faces);
        if self.editor.presentation().hide_chrome {
            return body;
        }
        let last = self.editor.tiling().windows().last() == Some(&window);
        let segments = if last {
            self.editor.mode_line_segments()
        } else {
            Vec::new()
        };
        let time = self.clock.as_deref().filter(|_| last).map(clock::now);
        let extras: Vec<String> = segments.into_iter().chain(time).collect();
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

    fn body<'a>(&'a self, pane: Pane<'a>, window: WindowId, faces: Faces) -> Element<'a, Message> {
        let buffer = pane.buffer().id();
        if let Some(screen) = games::screen(&self.editor, buffer) {
            let drawn = canvas(GameScreen {
                screen,
                font: self.font,
                size: self.text_size,
            })
            .width(Length::Fill)
            .height(Length::Fill);
            return mouse_area(drawn)
                .on_press(Message::FocusWindow(window))
                .into();
        }
        if let Some((_, shown)) = self.editor.plugin_buffer::<ViewBuffer>(buffer) {
            let look = ox_view::Look {
                faces,
                font: self.font,
                size: self.text_size,
            };
            let drawn = ox_view::view(
                &shown.node,
                look,
                move |slot| Message::OxPress(buffer, slot),
                move |slot, args| Message::OxCall(buffer, slot, args),
            );
            let sized = sensor(container(drawn).width(Length::Fill).height(Length::Fill))
                .on_show(move |size| Message::ViewSized(buffer, size))
                .on_resize(move |size| Message::ViewSized(buffer, size));
            return mouse_area(
                container(sized)
                    .padding([4, 8])
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_press(Message::FocusWindow(window))
            .into();
        }

        let text = container(BufferView::new(
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
        .height(Length::Fill);
        let side = self.editor.side_window().map(|side| side.window);
        match self
            .editor
            .presentation()
            .column
            .filter(|_| side != Some(window))
        {
            Some(columns) => container(text.width(Length::Fixed(
                units::to_f32(columns) * units::cell_width(self.text_size) + COLUMN_PADDING,
            )))
            .center_x(Length::Fill)
            .padding([self.text_size * COLUMN_MARGIN, 0.0])
            .height(Length::Fill)
            .into(),
            None => text.into(),
        }
    }
}
