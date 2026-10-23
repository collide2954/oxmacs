use iced::widget::{
    Column, Row, canvas, column, container, image, mouse_area, responsive, sensor, space, stack,
    text,
};
use iced::{Border, Color, ContentFit, Element, Length};
use oxmacs_core::clock;
use oxmacs_core::editor::{Axis, GalleryView, Pane, PlotView, Tiling, WindowId};
use oxmacs_core::theme::Theme as Faces;
use oxmacs_ox::takeover::{self, Takeover};
use oxmacs_ox::views::ViewBuffer;

use super::{App, Message};
use crate::widgets::buffer_view::{BufferView, Motion};
use crate::widgets::divider::Divider;
use crate::widgets::drawing::Drawing;
use crate::widgets::mode_line::{self, Look};
use crate::widgets::{completions, echo_area, ox_view, transient};
use crate::{style, units};
impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let faces = self.faces();
        let offered = self.editor.completions();
        let windows = match takeover::current(&self.editor) {
            Some(shown) => self.takeover(shown, faces),
            None => self.tile(self.editor.tiling(), Vec::new(), faces),
        };
        column![
            windows,
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
        let plots = match (pane.plot_gallery(), pane.plot()) {
            (Some(gallery), _) => Some(plot_gallery(window, &gallery, faces)),
            (None, Some(plot)) => Some(self.plot(window, &plot, faces)),
            (None, None) => None,
        };
        if let Some(body) = plots {
            let look = Look {
                faces,
                active,
                font: self.font,
                size: self.text_size,
            };
            return column![
                body,
                mode_line::view(&pane.status(), &[], self.mode_line_style, look)
            ]
            .into();
        }
        let body = self.body(pane, window, faces);
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
        if let Some((_, shown)) = self.editor.plugin_buffer::<ViewBuffer>(buffer) {
            let look = ox_view::Look {
                faces,
                font: self.font,
                size: self.text_size,
            };
            let drawn = ox_view::view(&shown.node, look, move |slot| {
                Message::OxPress(buffer, slot)
            });
            return mouse_area(
                container(drawn)
                    .padding([4, 8])
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_press(Message::FocusWindow(window))
            .into();
        }

        container(BufferView::new(
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
        .into()
    }

    fn plot(&self, window: WindowId, plot: &PlotView, faces: Faces) -> Element<'_, Message> {
        let picture = image(self.plot_image(&plot.path))
            .content_fit(ContentFit::Contain)
            .width(Length::Fill)
            .height(Length::Fill);
        let counter = container(
            text(format!("{} / {}", plot.index, plot.count))
                .font(self.font)
                .size(self.text_size * COUNTER_SCALE)
                .color(style::color(faces.shadow)),
        )
        .padding([4, 8])
        .align_right(Length::Fill);
        let sized = sensor(stack![picture, counter])
            .on_show(move |size| Message::PlotSized(window, size))
            .on_resize(move |size| Message::PlotSized(window, size));
        mouse_area(sized)
            .on_press(Message::FocusWindow(window))
            .into()
    }
}

fn plot_gallery<'a>(window: WindowId, gallery: &GalleryView, faces: Faces) -> Element<'a, Message> {
    let columns = gallery.columns.max(1);
    let accent = style::color(faces.cursor);
    let rows = gallery
        .paths
        .chunks(columns)
        .enumerate()
        .map(|(row, paths)| {
            let mut tiles: Vec<Element<'a, Message>> = paths
                .iter()
                .enumerate()
                .map(|(column, path)| {
                    let position = row * columns + column;
                    let chosen = position == gallery.selected;
                    let picture = image(image::Handle::from_path(path))
                        .content_fit(ContentFit::Contain)
                        .width(Length::Fill)
                        .height(Length::Fill);
                    let tile = container(picture)
                        .padding(TILE_PADDING)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(move |_| container::Style {
                            border: Border {
                                color: if chosen { accent } else { Color::TRANSPARENT },
                                width: TILE_BORDER,
                                radius: TILE_RADIUS.into(),
                            },
                            ..container::Style::default()
                        });
                    mouse_area(tile)
                        .on_press(Message::PickPlot(window, position))
                        .into()
                })
                .collect();
            tiles.resize_with(columns, || space().width(Length::Fill).into());
            Row::with_children(tiles).height(Length::Fill).into()
        });
    container(Column::with_children(rows))
        .padding(TILE_PADDING)
        .into()
}

const COUNTER_SCALE: f32 = 0.85;
const TILE_PADDING: u16 = 4;
const TILE_BORDER: f32 = 2.0;
const TILE_RADIUS: f32 = 4.0;
