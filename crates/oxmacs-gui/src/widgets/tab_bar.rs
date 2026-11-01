use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Shell, Widget, renderer};
use iced::widget::{Row, column, container, mouse_area, row, space, text};
use iced::{Alignment, Element, Event, Font, Length, Rectangle, Size, Theme, mouse};
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
    Moved { from: usize, to: usize },
    Entered(usize),
    Left(usize),
    Close(usize),
    New,
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
    let mut slots = Vec::new();
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
            slots.push(None);
        }
        slots.push(Some(index));
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
    slots.push(None);
    let background = style::color(colors.strip);
    let strip = Strip {
        content: Row::with_children(tabs).align_y(Alignment::Center).into(),
        slots,
        on_move: Box::new(move |from, to| on(TabMessage::Moved { from, to })),
    };
    container(strip)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(background.into()),
            ..container::Style::default()
        })
        .into()
}

struct Strip<'a, M, Renderer> {
    content: Element<'a, M, Theme, Renderer>,
    slots: Vec<Option<usize>>,
    on_move: Box<dyn Fn(usize, usize) -> M + 'a>,
}

#[derive(Debug, Default)]
struct Dragged(Option<usize>);

fn slot_under(layout: Layout<'_>, slots: &[Option<usize>], cursor: mouse::Cursor) -> Option<usize> {
    layout
        .children()
        .zip(slots)
        .find(|(child, _)| cursor.is_over(child.bounds()))
        .and_then(|(_, slot)| *slot)
}

impl<M, Renderer> Widget<M, Theme, Renderer> for Strip<'_, M, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Dragged>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Dragged::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let dragged = tree.state.downcast_mut::<Dragged>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                dragged.0 = slot_under(layout, &self.slots, cursor);
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(from) = dragged.0.take()
                    && let Some(to) = slot_under(layout, &self.slots, cursor)
                    && from != to
                {
                    shell.publish((self.on_move)(from, to));
                }
            }
            _ => {}
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
}

impl<'a, M, Renderer> From<Strip<'a, M, Renderer>> for Element<'a, M, Theme, Renderer>
where
    M: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(strip: Strip<'a, M, Renderer>) -> Self {
        Element::new(strip)
    }
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
