use std::time::Duration;

use iced::animation::Easing;
use iced::time::Instant;
use iced::widget::container;
use iced::{Animation, Border, Color, Shadow, Vector};
use oxmacs_core::applet::Applet;
use oxmacs_core::editor::Editor;
use oxmacs_gui::applet::{Context, GuiApplet};
use oxmacs_gui::style;

const FADE: Duration = Duration::from_millis(150);
const HALO: f32 = 0.9;
const EDGE: f32 = 0.7;
const TINT: f32 = 0.1;
const BLUR: f32 = 18.0;
const RADIUS: f32 = 6.0;

#[derive(Debug)]
pub struct Glow {
    shown: Animation<bool>,
    now: Instant,
}

impl Default for Glow {
    fn default() -> Self {
        Self {
            shown: Animation::new(false)
                .duration(FADE)
                .easing(Easing::EaseOutCubic),
            now: Instant::now(),
        }
    }
}

impl Glow {
    fn level(&self, context: &Context) -> f32 {
        if context.animations {
            self.shown.interpolate(0.0, 1.0, context.now)
        } else if self.shown.value() {
            1.0
        } else {
            0.0
        }
    }
}

fn faded(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

fn frame(color: Color, level: f32) -> container::Style {
    container::Style {
        background: Some(faded(color, TINT * level).into()),
        border: Border {
            color: faded(color, EDGE * level),
            width: 1.0,
            radius: RADIUS.into(),
        },
        shadow: Shadow {
            color: faded(color, HALO * level),
            offset: Vector::ZERO,
            blur_radius: BLUR,
        },
        ..container::Style::default()
    }
}

impl Applet for Glow {
    fn name(&self) -> &'static str {
        "glow"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn opt_in(&self) -> bool {
        true
    }

    fn install(&self, _editor: &mut Editor) {}
}

impl GuiApplet for Glow {
    fn minibuffer_style(&self, _editor: &Editor, context: &Context) -> Option<container::Style> {
        let level = self.level(context);
        (level > 0.0).then(|| frame(style::color(context.faces.minibuffer_prompt), level))
    }

    fn animating(&self, editor: &Editor) -> bool {
        editor.prompting() != self.shown.value() || self.shown.is_animating(self.now)
    }

    fn tick(&mut self, editor: &Editor, now: Instant) {
        self.now = now;
        let waiting = editor.prompting();
        if waiting != self.shown.value() {
            self.shown.go_mut(waiting, now);
        }
    }
}
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use iced::theme::Palette;
    use iced::{Color, Font};
    use oxmacs_core::input::KeySequence;
    use oxmacs_core::theme::ADWAITA;
    use oxmacs_gui::style;

    use super::*;

    fn press(editor: &mut Editor, notation: &str) {
        let keys: KeySequence = notation.parse().unwrap();
        for key in keys.keys() {
            editor.handle_key(*key);
        }
    }

    fn context(now: Instant, animations: bool) -> Context {
        Context {
            faces: ADWAITA,
            palette: Palette::LIGHT,
            font: Font::DEFAULT,
            size: 14.0,
            now,
            animations,
        }
    }

    fn halo(glow: &Glow, editor: &Editor, now: Instant, animations: bool) -> Option<Color> {
        glow.minibuffer_style(editor, &context(now, animations))
            .map(|style| style.shadow.color)
    }

    fn prompt_color() -> Color {
        style::color(ADWAITA.minibuffer_prompt)
    }

    #[test]
    fn an_idle_minibuffer_does_not_glow() {
        let mut glow = Glow::default();
        let editor = Editor::new();
        let now = Instant::now();

        glow.tick(&editor, now);

        assert_eq!(halo(&glow, &editor, now, true), None);
        assert!(!glow.animating(&editor));
    }

    #[test]
    fn a_waiting_prompt_fades_the_glow_in_and_holds_it() {
        let mut glow = Glow::default();
        let mut editor = Editor::new();
        let start = Instant::now();
        press(&mut editor, "M-x");
        assert!(glow.animating(&editor));

        glow.tick(&editor, start);
        let early = halo(&glow, &editor, start + Duration::from_millis(40), true).unwrap();
        let settled = halo(&glow, &editor, start + FADE, true).unwrap();
        let later = halo(&glow, &editor, start + FADE * 20, true).unwrap();

        assert!(
            early.a > 0.0 && early.a < settled.a,
            "{early:?} {settled:?}"
        );
        assert_eq!(settled, later);
        assert_eq!(Color { a: 1.0, ..settled }, prompt_color());
        glow.tick(&editor, start + FADE);
        assert!(!glow.animating(&editor));
    }

    #[test]
    fn answering_fades_the_glow_out() {
        let mut glow = Glow::default();
        let mut editor = Editor::new();
        let start = Instant::now();
        editor.ask("Go?", |_, _| {});
        glow.tick(&editor, start);
        assert!(halo(&glow, &editor, start + FADE, true).is_some());

        press(&mut editor, "n");
        let answered = start + FADE * 2;
        glow.tick(&editor, answered);

        assert!(halo(&glow, &editor, answered + FADE / 3, true).is_some());
        assert_eq!(halo(&glow, &editor, answered + FADE, true), None);
    }

    #[test]
    fn without_animations_the_glow_is_immediate() {
        let mut glow = Glow::default();
        let mut editor = Editor::new();
        let start = Instant::now();
        press(&mut editor, "C-s");

        glow.tick(&editor, start);

        let shown = halo(&glow, &editor, start, false).unwrap();
        assert_eq!(Some(shown), halo(&glow, &editor, start + FADE, true));
    }
}
