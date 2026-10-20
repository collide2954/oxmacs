use crate::command::Command;
use crate::editor::Editor;
use crate::editor::buffers::Mode;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SplashAnimation {
    #[default]
    Plasma,
    Wave,
    Assemble,
    Neurons,
    Dna,
}

impl SplashAnimation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Plasma => "plasma",
            Self::Wave => "color wave",
            Self::Assemble => "assemble",
            Self::Neurons => "neurons",
            Self::Dna => "DNA replication",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Plasma => Self::Wave,
            Self::Wave => Self::Assemble,
            Self::Assemble => Self::Neurons,
            Self::Neurons => Self::Dna,
            Self::Dna => Self::Plasma,
        }
    }
}

pub(super) fn extras(art: SplashAnimation) -> Vec<(Command, String)> {
    vec![(
        Command::CycleSplashAnimation,
        format!("Animation: {}", art.name()),
    )]
}

impl Editor {
    pub fn splash_animation(&self) -> Option<SplashAnimation> {
        self.tiling()
            .windows()
            .into_iter()
            .filter_map(|window| self.pane_of(window))
            .find_map(|pane| pane.splash_art())
    }

    pub(in crate::editor) fn cycle_splash_animation(&mut self) {
        let Some(art) = self.buffers.splash(0).map(|splash| splash.art) else {
            self.show_message("Only the start screen animates");
            return;
        };
        let next = art.next();
        if let Some((_, Mode::Splash(splash))) = self.buffers.entry_mut(0) {
            splash.art = next;
        }
        self.refill_splash();
        self.show_message(format!("Animation: {}", next.name()));
    }
}

#[cfg(test)]
mod tests {
    use super::SplashAnimation;
    use crate::command::Command;
    use crate::editor::Editor;
    use crate::editor::test_support::{at_point, lines, press, type_text};

    fn shown() -> Editor {
        let mut editor = Editor::new();
        editor.about();
        editor
    }

    #[test]
    fn the_start_screen_animates_a_logo_above_the_keys() {
        let editor = shown();

        assert_eq!(editor.buffer().name(), "*oxmacs*");
        assert_eq!(editor.pane().splash_art(), Some(SplashAnimation::Plasma));
        assert_eq!(editor.splash_animation(), Some(SplashAnimation::Plasma));
        let lines = lines(&editor);
        assert_eq!(lines[1], format!("   oxmacs {}", env!("CARGO_PKG_VERSION")));
        let cycle = lines
            .iter()
            .find(|line| line.trim_start().starts_with("a "))
            .unwrap();
        assert!(cycle.contains("cycle-splash-animation"), "{cycle}");
        assert!(cycle.ends_with("Animation: plasma"), "{cycle}");
    }

    #[test]
    fn a_cycles_through_every_animation() {
        let mut editor = shown();

        for expected in [
            SplashAnimation::Wave,
            SplashAnimation::Assemble,
            SplashAnimation::Neurons,
            SplashAnimation::Dna,
            SplashAnimation::Plasma,
            SplashAnimation::Wave,
        ] {
            press(&mut editor, "a");
            assert_eq!(editor.pane().splash_art(), Some(expected));
        }
        assert!(
            lines(&editor)
                .iter()
                .any(|line| line.ends_with("Animation: color wave"))
        );
        assert!(at_point(&editor).starts_with("C-x C-f"));
        assert_eq!(editor.message(), Some("Animation: color wave"));
    }

    #[test]
    fn about_oxmacs_shows_the_animated_screen() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "about-oxmacs");
        press(&mut editor, "RET");

        assert_eq!(editor.buffer().name(), "*oxmacs*");
        assert!(editor.pane().splash_art().is_some());
    }

    #[test]
    fn other_buffers_have_no_animation_to_cycle() {
        let mut editor = shown();
        press(&mut editor, "q");
        assert_eq!(editor.splash_animation(), None);

        editor.execute(Command::CycleSplashAnimation);

        assert_eq!(editor.message(), Some("Only the start screen animates"));
    }
}
