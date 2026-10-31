mod compiled;
mod gpu;
mod software;

use iced::advanced::image::{FilterMethod, Image, Renderer as _};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget};
use iced::{Element, Length, Rectangle, Renderer, Size, Theme, mouse};
use iced_wgpu::primitive::Renderer as _;
use oxmacs_core::view::takeover;

pub use gpu::wait_for_shaders;
pub use software::newly_unavailable;

#[derive(Debug)]
pub struct Backdrop<'a> {
    backdrop: &'a takeover::Backdrop,
}

impl<'a> Backdrop<'a> {
    pub fn new(backdrop: &'a takeover::Backdrop) -> Self {
        Self { backdrop }
    }
}

impl<Message> Widget<Message, Theme, Renderer> for Backdrop<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if let Renderer::Primary(_) = renderer {
            renderer.draw_primitive(bounds, gpu::Frame(self.backdrop.clone()));
        } else if let Some(handle) = software::frame(self.backdrop, (bounds.width, bounds.height)) {
            renderer.draw_image(
                Image::new(handle).filter_method(FilterMethod::Linear),
                bounds,
                bounds,
            );
        }
    }
}

impl<'a, Message: 'a> From<Backdrop<'a>> for Element<'a, Message> {
    fn from(backdrop: Backdrop<'a>) -> Self {
        Element::new(backdrop)
    }
}
