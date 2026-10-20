use std::fmt;

use iced::Task;
use oxmacs_core::applet::{Applet, Payload};

pub trait GuiApplet: Applet + fmt::Debug {
    fn perform(&self, _effect: &Payload) -> Option<Task<Payload>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::editor::Editor;

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
        fn perform(&self, effect: &Payload) -> Option<Task<Payload>> {
            effect.get::<Ping>()?;
            Some(Task::done(Payload::new(Pong)))
        }
    }

    #[test]
    fn gui_applets_perform_only_the_effects_they_know() {
        assert!(Echo.perform(&Payload::new(Ping)).is_some());
        assert!(Echo.perform(&Payload::new(Pong)).is_none());
    }
}
