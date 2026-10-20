use std::fmt;

use iced::theme::Palette;
use iced::time::Instant;
use iced::{Element, Font, Subscription, Task};
use oxmacs_core::applet::{Applet, Payload};
use oxmacs_core::editor::{Editor, Pane, WindowId};
use oxmacs_core::theme::Theme as Faces;

#[derive(Debug, Clone, Copy)]
pub struct Context {
    pub faces: Faces,
    pub palette: Palette,
    pub font: Font,
    pub size: f32,
    pub now: Instant,
    pub animations: bool,
}

pub trait GuiApplet: Applet + fmt::Debug {
    fn perform(&mut self, _effect: &Payload) -> Option<Task<Payload>> {
        None
    }

    fn above<'a>(
        &'a self,
        _editor: &'a Editor,
        _pane: Pane<'a>,
        _context: &Context,
    ) -> Option<Element<'a, Payload>> {
        None
    }

    fn beside<'a>(
        &'a self,
        _editor: &'a Editor,
        _pane: Pane<'a>,
        _context: &Context,
    ) -> Option<Element<'a, Payload>> {
        None
    }

    fn body<'a>(
        &'a self,
        _editor: &'a Editor,
        _pane: Pane<'a>,
        _window: WindowId,
        _context: &Context,
    ) -> Option<Element<'a, Payload>> {
        None
    }

    fn subscription(&self, _editor: &Editor) -> Subscription<Payload> {
        Subscription::none()
    }

    fn animating(&self, _editor: &Editor) -> bool {
        false
    }

    fn tick(&mut self, _editor: &Editor, _now: Instant) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Ping;

    #[derive(Debug)]
    struct Pong;

    #[derive(Debug)]
    struct Echo;

    impl Applet for Echo {
        fn name(&self) -> &'static str {
            "echo"
        }

        fn version(&self) -> &'static str {
            "0.1.0"
        }

        fn install(&self, _editor: &mut Editor) {}
    }

    impl GuiApplet for Echo {
        fn perform(&mut self, effect: &Payload) -> Option<Task<Payload>> {
            effect.get::<Ping>()?;
            Some(Task::done(Payload::new(Pong)))
        }
    }

    #[test]
    fn gui_applets_perform_only_the_effects_they_know() {
        assert!(Echo.perform(&Payload::new(Ping)).is_some());
        assert!(Echo.perform(&Payload::new(Pong)).is_none());
    }

    #[test]
    fn gui_applets_add_nothing_unless_they_choose_to() {
        let editor = Editor::new();
        let context = Context {
            faces: *editor.theme(),
            palette: iced::Theme::Light.palette(),
            font: Font::MONOSPACE,
            size: 14.0,
            now: Instant::now(),
            animations: true,
        };

        assert!(Echo.above(&editor, editor.pane(), &context).is_none());
        assert!(Echo.beside(&editor, editor.pane(), &context).is_none());
        assert!(
            Echo.body(&editor, editor.pane(), editor.selected_window(), &context)
                .is_none()
        );
        assert!(!Echo.animating(&editor));
    }
}
