use std::cell::RefCell;

use iced::widget::canvas::{self, Frame, Geometry, Image, Text};
use iced::widget::image::{FilterMethod, Handle};
use iced::{Color, Font, Pixels, Point, Rectangle, Renderer, Size, Theme, mouse};
use oxmacs_script::games::Screen;
use rodeo::draw::Viewport;
use rodeo::vec::Vec2;

const LETTERBOX: Color = Color::from_rgb(0.05, 0.05, 0.07);
const STATUS: f32 = 0.06;

thread_local! {
    static SHOWN: RefCell<Option<(u64, Handle)>> = const { RefCell::new(None) };
}

fn handle(screen: &Screen) -> Handle {
    SHOWN.with_borrow_mut(|shown| {
        if let Some((generation, handle)) = shown.as_ref()
            && *generation == screen.generation
        {
            return handle.clone();
        }
        let handle = Handle::from_rgba(screen.width, screen.height, screen.pixels.to_vec());
        *shown = Some((screen.generation, handle.clone()));
        handle
    })
}

fn narrow(value: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "window coordinates fit in an f32"
    )]
    let narrowed = value as f32;
    narrowed
}

pub fn placement(screen: &Screen, bounds: Size) -> (Rectangle, f32) {
    let fit = Viewport::fit(
        Vec2::new(f64::from(screen.width), f64::from(screen.height)),
        Vec2::new(f64::from(bounds.width), f64::from(bounds.height)),
        screen.smooth,
    );
    (
        Rectangle {
            x: narrow(fit.offset.x),
            y: narrow(fit.offset.y),
            width: narrow(fit.size.x),
            height: narrow(fit.size.y),
        },
        narrow(fit.scale),
    )
}

#[derive(Debug, Clone, Copy)]
pub struct GameScreen<'a> {
    pub screen: &'a Screen,
    pub font: Font,
    pub size: f32,
}

impl<Message> canvas::Program<Message> for GameScreen<'_> {
    type State = ();

    fn draw(
        &self,
        (): &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), LETTERBOX);
        let (placed, scale) = placement(self.screen, bounds.size());
        let filter = if self.screen.smooth {
            FilterMethod::Linear
        } else {
            FilterMethod::Nearest
        };
        frame.draw_image(
            placed,
            Image::new(handle(self.screen)).filter_method(filter),
        );
        for caption in &self.screen.captions {
            let [red, green, blue, alpha] = caption.color;
            frame.fill_text(Text {
                content: caption.text.clone(),
                position: Point::new(
                    placed.x + caption.at.0 * scale,
                    placed.y + caption.at.1 * scale,
                ),
                color: Color::from_rgba8(red, green, blue, f32::from(alpha) / 255.0),
                size: Pixels(caption.size * scale),
                font: self.font,
                ..Text::default()
            });
        }
        if let Some(status) = &self.screen.status {
            let height = (bounds.height * STATUS).max(self.size * 1.6);
            frame.fill_rectangle(
                Point::new(0.0, bounds.height - height),
                Size::new(bounds.width, height),
                Color::from_rgba(0.0, 0.0, 0.0, 0.7),
            );
            frame.fill_text(Text {
                content: status.clone(),
                position: Point::new(self.size, bounds.height - height + self.size * 0.3),
                color: Color::WHITE,
                size: Pixels(self.size),
                font: self.font,
                ..Text::default()
            });
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use iced::Size;
    use oxmacs_script::games::Screen;

    use super::placement;

    fn screen(smooth: bool) -> Screen {
        Screen {
            width: 320,
            height: 180,
            pixels: vec![0; 320 * 180 * 4].into(),
            generation: 1,
            smooth,
            captions: Vec::new(),
            status: None,
        }
    }

    #[test]
    fn games_scale_by_whole_numbers_inside_black_bars() {
        let (placed, scale) = placement(&screen(false), Size::new(1366.0, 768.0));
        assert_eq!(scale, 4.0);
        assert_eq!(
            [placed.x, placed.y, placed.width, placed.height],
            [43.0, 24.0, 1280.0, 720.0]
        );
        let (_, smooth) = placement(&screen(true), Size::new(1366.0, 768.0));
        assert!(smooth > 4.2);
    }
}
