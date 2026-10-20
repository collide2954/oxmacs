mod art;

use iced::Element;
use iced::time::Instant;
use oxmacs_core::applet::{Applet, Payload};
use oxmacs_core::command::{CommandId, CommandSpec, Scope};
use oxmacs_core::editor::{Editor, Pane};
use oxmacs_gui::applet::{Context, GuiApplet};

const HELP: &str = "The start screen draws the oxmacs logo in block letters, animated above the usual keys. a cycles between a drifting plasma field with a shimmer, a color wave through the letters, the logo assembling from scattered pixels before it twinkles, a network of neurons firing signals that light the letters as they pass, and DNA replicating behind the logo with its helicase, polymerases and growing strands. The animation holds still when animations are off.";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Animation {
    #[default]
    Plasma,
    Wave,
    Assemble,
    Neurons,
    Dna,
}

impl Animation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Plasma => "plasma",
            Self::Wave => "color wave",
            Self::Assemble => "assemble",
            Self::Neurons => "neurons",
            Self::Dna => "DNA replication",
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Plasma => Self::Wave,
            Self::Wave => Self::Assemble,
            Self::Assemble => Self::Neurons,
            Self::Neurons => Self::Dna,
            Self::Dna => Self::Plasma,
        }
    }

    fn label(self) -> String {
        format!("Animation: {}", self.name())
    }
}

#[derive(Debug, Clone, Copy)]
struct Current {
    command: CommandId,
    animation: Animation,
}

pub fn animation(editor: &Editor) -> Option<Animation> {
    editor
        .applet_state::<Current>()
        .map(|current| current.animation)
}

fn cycle(editor: &mut Editor) {
    let Some(current) = editor.applet_state::<Current>().copied() else {
        return;
    };
    if !editor.pane().is_splash() {
        editor.show_message("Only the start screen animates");
        return;
    }
    let animation = current.animation.next();
    editor.set_applet_state(Current {
        animation,
        ..current
    });
    editor.set_splash_entry(current.command, animation.label());
    editor.show_message(animation.label());
}

#[derive(Debug, Default)]
pub struct Splash {
    clock: Option<(Animation, Instant)>,
}

impl Splash {
    pub fn seconds(&self, animation: Animation, context: &Context) -> f32 {
        match self.clock {
            Some((known, start)) if context.animations && known == animation => {
                context.now.saturating_duration_since(start).as_secs_f32()
            }
            _ => art::still(animation),
        }
    }
}

impl Applet for Splash {
    fn name(&self) -> &'static str {
        "animated-splash"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn opt_in(&self) -> bool {
        true
    }

    fn install(&self, editor: &mut Editor) {
        let command = editor.add_command(CommandSpec {
            name: "cycle-splash-animation",
            summary: "Switch the start screen's logo animation",
            help: Some(HELP),
            scope: Scope::Splash.into(),
            run: cycle,
        });
        let _ = editor.bind(Scope::Splash, "a", command);
        let animation = Animation::default();
        editor.set_applet_state(Current { command, animation });
        editor.set_splash_entry(command, animation.label());
    }
}

impl GuiApplet for Splash {
    fn above<'a>(
        &'a self,
        editor: &'a Editor,
        pane: Pane<'a>,
        context: &Context,
    ) -> Option<Element<'a, Payload>> {
        if !pane.is_splash() {
            return None;
        }
        let animation = animation(editor)?;
        Some(
            art::SplashArt::new(
                animation,
                self.seconds(animation, context),
                context.faces,
                context.font,
                context.size,
            )
            .into(),
        )
    }

    fn animating(&self, editor: &Editor) -> bool {
        editor.splash_shown()
    }

    fn tick(&mut self, editor: &Editor, now: Instant) {
        let shown = animation(editor).filter(|_| editor.splash_shown());
        match (shown, self.clock) {
            (Some(animation), Some((known, _))) if animation == known => {}
            (Some(animation), _) => self.clock = Some((animation, now)),
            (None, _) => self.clock = None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use iced::Font;
    use oxmacs_core::input::KeySequence;

    use super::*;

    fn press(editor: &mut Editor, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
    }

    fn shown() -> Editor {
        let mut editor = Editor::new();
        editor.install(&Splash::default());
        editor.about();
        editor
    }

    fn lines(editor: &Editor) -> Vec<String> {
        editor
            .buffer()
            .contents()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn context(editor: &Editor, now: Instant, animations: bool) -> Context {
        Context {
            faces: *editor.theme(),
            font: Font::MONOSPACE,
            size: 14.0,
            now,
            animations,
        }
    }

    #[test]
    fn the_start_screen_offers_a_key_for_the_next_animation() {
        let editor = shown();

        let line = lines(&editor)
            .into_iter()
            .find(|line| line.trim_start().starts_with("a "))
            .unwrap();
        assert!(line.contains("cycle-splash-animation"), "{line}");
        assert!(line.ends_with("Animation: plasma"), "{line}");
        assert_eq!(animation(&editor), Some(Animation::Plasma));
    }

    #[test]
    fn a_cycles_through_every_animation() {
        let mut editor = shown();

        for expected in [
            Animation::Wave,
            Animation::Assemble,
            Animation::Neurons,
            Animation::Dna,
            Animation::Plasma,
            Animation::Wave,
        ] {
            press(&mut editor, "a");
            assert_eq!(animation(&editor), Some(expected));
        }
        assert!(
            lines(&editor)
                .iter()
                .any(|line| line.ends_with("Animation: color wave"))
        );
        assert_eq!(editor.message(), Some("Animation: color wave"));
    }

    #[test]
    fn other_buffers_have_no_animation_to_cycle() {
        let mut editor = shown();
        press(&mut editor, "q");
        let command = editor.applet_state::<Current>().unwrap().command;

        editor.execute(command);

        assert_eq!(editor.message(), Some("Only the start screen animates"));
        assert_eq!(animation(&editor), Some(Animation::Plasma));
    }

    #[test]
    fn the_art_shows_above_the_start_screen_and_animates_only_there() {
        let mut editor = shown();
        let splash = Splash::default();
        let now = Instant::now();

        assert!(
            splash
                .above(&editor, editor.pane(), &context(&editor, now, true))
                .is_some()
        );
        assert!(splash.animating(&editor));

        press(&mut editor, "q");

        assert!(
            splash
                .above(&editor, editor.pane(), &context(&editor, now, true))
                .is_none()
        );
        assert!(!splash.animating(&editor));
    }

    #[test]
    fn the_clock_restarts_with_each_animation_and_holds_still_without_animations() {
        let mut editor = shown();
        let mut splash = Splash::default();
        let start = Instant::now();
        splash.tick(&editor, start);
        let later = start + Duration::from_secs(3);
        splash.tick(&editor, later);

        let seconds = splash.seconds(Animation::Plasma, &context(&editor, later, true));
        assert!((seconds - 3.0).abs() < 1e-3, "{seconds}");
        assert!(
            (splash.seconds(Animation::Plasma, &context(&editor, later, false))
                - art::still(Animation::Plasma))
            .abs()
                < 1e-6
        );

        press(&mut editor, "a");
        splash.tick(&editor, later);
        let seconds = splash.seconds(Animation::Wave, &context(&editor, later, true));
        assert!(seconds.abs() < 1e-6, "{seconds}");
    }
}
