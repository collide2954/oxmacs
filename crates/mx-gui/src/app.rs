use iced::animation::Easing;
use iced::font::Family;
use iced::theme::Mode;
use iced::time::{Duration, Instant};
use iced::widget::{column, container, text};
use iced::{Animation, Element, Font, Length, Subscription, Task, Theme, keyboard, system, window};
use mx_core::config::{self, Config, ConfigError};
use mx_core::editor::Editor;
use mx_core::theme::{Appearance, Theme as Faces};

use crate::buffer_view::{BufferView, Motion, ViewEvent};
use crate::keys::{self, Platform};
use crate::{animation, completions, echo_area, fonts, style};

#[derive(Debug)]
pub struct App {
    editor: Editor,
    font: Font,
    text_size: f32,
    animations: bool,
    blink_interval: Duration,
    last_input: Instant,
    now: Instant,
    theme_fade: Option<(Faces, Animation<bool>)>,
    reveal: Animation<bool>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Keyboard(keyboard::Event),
    View(ViewEvent),
    AppearanceChanged(Mode),
    Tick(Instant),
}

impl Default for App {
    fn default() -> Self {
        Self::new(Ok(Config::default()))
    }
}

impl App {
    pub fn new(config: Result<Config, ConfigError>) -> Self {
        let mut editor = Editor::new();
        let config = config.unwrap_or_else(|error| {
            editor.show_message(error.to_string());
            Config::default()
        });
        editor.apply_config(&config);
        let font = config
            .font_family
            .map_or(fonts::EDITOR, |family| Font::with_name(family.leak()));
        Self {
            editor,
            font,
            text_size: points_to_pixels(config.font_size),
            animations: config.animations,
            blink_interval: Duration::from_millis(config.cursor_blink_interval),
            last_input: Instant::now(),
            now: Instant::now(),
            theme_fade: None,
            reveal: transition(false, animation::REVEAL),
        }
    }

    pub fn boot() -> (Self, Task<Message>) {
        let config = config::default_path()
            .map_or_else(|| Ok(Config::default()), |path| Config::load(&path));
        (
            Self::new(config),
            system::theme().map(Message::AppearanceChanged),
        )
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn text_size(&self) -> f32 {
        self.text_size
    }

    pub fn font(&self) -> Font {
        self.font
    }

    pub fn animations(&self) -> bool {
        self.animations
    }

    pub fn is_animating(&self) -> bool {
        let fading = self
            .theme_fade
            .as_ref()
            .is_some_and(|(_, fade)| fade.is_animating(self.now));
        fading || self.reveal.is_animating(self.now)
    }

    pub fn completion_opacity(&self) -> f32 {
        self.reveal.interpolate(0.0, 1.0, self.now)
    }

    fn set_appearance(&mut self, appearance: Appearance) {
        let before = self.faces();
        self.editor.set_appearance(appearance);
        if self.animations && before.name != self.editor.theme().name {
            let fade = transition(false, animation::THEME_FADE).go(true, self.now);
            self.theme_fade = Some((before, fade));
        }
    }

    fn sync_reveal(&mut self) {
        let open = self.editor.minibuffer().is_some();
        if open == self.reveal.value() {
            return;
        }
        if open && self.animations {
            self.reveal.go_mut(true, self.now);
        } else {
            self.reveal = transition(open, animation::REVEAL);
        }
    }

    fn tick(&mut self, now: Instant) {
        self.now = now;
        if self
            .theme_fade
            .as_ref()
            .is_some_and(|(_, fade)| !fade.is_animating(now))
        {
            self.theme_fade = None;
        }
    }

    fn reject_font(&mut self) {
        if self.font == fonts::EDITOR {
            return;
        }
        if let Family::Name(name) = self.font.family {
            self.editor.show_message(format!(
                "{name} is not a monospaced font; using {}",
                fonts::EDITOR_FAMILY
            ));
        }
        self.font = fonts::EDITOR;
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        if !matches!(message, Message::Tick(_)) {
            self.now = Instant::now();
        }
        match message {
            Message::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                modifiers,
                ..
            }) => {
                if let Some(key) =
                    keys::translate(&key, &modified_key, modifiers, Platform::current())
                {
                    self.last_input = self.now;
                    self.editor.handle_key(key);
                    self.sync_reveal();
                }
            }
            Message::Keyboard(_) => {}
            Message::View(ViewEvent::RowsChanged(rows)) => self.editor.resize_window(rows),
            Message::View(ViewEvent::FontNotMonospaced) => self.reject_font(),
            Message::AppearanceChanged(mode) => {
                if let Some(appearance) = appearance(mode) {
                    self.set_appearance(appearance);
                }
            }
            Message::Tick(now) => self.tick(now),
        }
        if self.editor.should_quit() {
            iced::exit()
        } else {
            Task::none()
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let frames = if self.is_animating() {
            window::frames().map(Message::Tick)
        } else {
            Subscription::none()
        };
        Subscription::batch([
            keyboard::listen().map(Message::Keyboard),
            system::theme_changes().map(Message::AppearanceChanged),
            frames,
        ])
    }

    pub fn title(&self) -> String {
        format!("{} - mx", self.editor.buffer().name())
    }

    pub fn faces(&self) -> Faces {
        let target = self.editor.theme();
        match &self.theme_fade {
            Some((from, fade)) => from.mix(target, fade.interpolate(0.0, 1.0, self.now)),
            None => *target,
        }
    }

    pub fn theme(&self) -> Theme {
        style::iced_theme(&self.faces())
    }

    pub fn view(&self) -> Element<'_, Message> {
        let line = |content: String| text(content).font(self.font).size(self.text_size);
        let faces = self.faces();
        column![
            container(BufferView::new(
                &self.editor,
                faces,
                self.font,
                self.text_size,
                Motion {
                    enabled: self.animations,
                    last_input: self.last_input,
                    blink_interval: self.blink_interval,
                },
                Message::View
            ))
            .padding([0, 4]),
            container(line(self.editor.mode_line()))
                .width(Length::Fill)
                .padding([1, 4])
                .style(move |_| style::mode_line(&faces)),
            echo_area::view(&self.editor, faces, self.font, self.text_size),
            completions::view(
                &self.editor,
                faces,
                self.font,
                self.text_size,
                self.completion_opacity()
            ),
        ]
        .into()
    }
}

pub fn points_to_pixels(points: f32) -> f32 {
    points * 4.0 / 3.0
}

fn transition(state: bool, duration: Duration) -> Animation<bool> {
    Animation::new(state)
        .duration(duration)
        .easing(Easing::EaseOutCubic)
}

fn appearance(mode: Mode) -> Option<Appearance> {
    match mode {
        Mode::Light => Some(Appearance::Light),
        Mode::Dark => Some(Appearance::Dark),
        Mode::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Key, Location, Modifiers};
    use mx_core::theme::{ADWAITA, ADWAITA_DARK};

    fn press(app: &mut App, c: &str, modifiers: Modifiers) {
        let _ = app.update(Message::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Character(c.into()),
            modified_key: Key::Character(c.into()),
            physical_key: Physical::Code(Code::KeyA),
            location: Location::Standard,
            modifiers,
            text: Some(c.into()),
            repeat: false,
        }));
    }

    #[test]
    fn view_shows_the_mode_line() {
        let app = App::default();
        let mut ui = iced_test::simulator(app.view());

        assert!(ui.find("*scratch*   L1 C0").is_ok());
    }

    #[test]
    fn view_echoes_the_editor_message() {
        let mut app = App::default();
        press(&mut app, "q", Modifiers::CTRL);
        let mut ui = iced_test::simulator(app.view());

        assert!(ui.find("C-q is undefined").is_ok());
    }

    #[test]
    fn config_errors_fall_back_to_defaults_and_are_reported() {
        let app = App::new(Err(ConfigError::TabWidth));

        assert_eq!(app.editor().message(), Some("tab-width must be at least 1"));
        assert_eq!(app.editor().tab_width(), 8);
    }

    #[test]
    fn config_sets_theme_and_font_size() {
        let config = Config::parse("theme = \"adwaita-dark\"\nfont-size = 12").unwrap();

        let app = App::new(Ok(config));

        assert_eq!(app.editor().theme().name, "adwaita-dark");
        assert!((app.text_size() - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn proportional_fonts_fall_back_to_the_bundled_font() {
        let config = Config::parse("font-family = \"Comic Sans MS\"").unwrap();
        let mut app = App::new(Ok(config));
        assert_ne!(app.font(), fonts::EDITOR);

        let _ = app.update(Message::View(ViewEvent::FontNotMonospaced));

        assert_eq!(app.font(), fonts::EDITOR);
        assert_eq!(
            app.editor().message(),
            Some("Comic Sans MS is not a monospaced font; using JetBrains Mono")
        );
    }

    #[test]
    fn the_bundled_font_is_never_replaced() {
        let mut app = App::default();

        let _ = app.update(Message::View(ViewEvent::FontNotMonospaced));

        assert_eq!(app.font(), fonts::EDITOR);
        assert_eq!(app.editor().message(), None);
    }

    fn later() -> Instant {
        Instant::now() + Duration::from_secs(1)
    }

    #[test]
    fn theme_changes_crossfade_when_animated() {
        let mut app = App::default();

        let _ = app.update(Message::AppearanceChanged(Mode::Dark));
        assert!(app.is_animating());
        assert_eq!(app.faces().default.background, ADWAITA.default.background);

        let _ = app.update(Message::Tick(later()));
        assert_eq!(app.faces(), ADWAITA_DARK);
        assert!(!app.is_animating());
    }

    #[test]
    fn theme_changes_are_instant_without_animations() {
        let config = Config::parse("animations = false").unwrap();
        let mut app = App::new(Ok(config));

        let _ = app.update(Message::AppearanceChanged(Mode::Dark));

        assert_eq!(app.faces(), ADWAITA_DARK);
        assert!(!app.is_animating());
    }

    #[test]
    fn opening_the_minibuffer_fades_in_the_completions() {
        let mut app = App::default();

        press(&mut app, "x", Modifiers::ALT);
        assert!(app.completion_opacity() < 0.01);

        let _ = app.update(Message::Tick(later()));
        assert!((app.completion_opacity() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn completions_appear_at_once_without_animations() {
        let config = Config::parse("animations = false").unwrap();
        let mut app = App::new(Ok(config));

        press(&mut app, "x", Modifiers::ALT);

        assert!((app.completion_opacity() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn animations_follow_the_config() {
        assert!(App::default().animations());

        let config = Config::parse("animations = false").unwrap();
        assert!(!App::new(Ok(config)).animations());
    }

    #[test]
    fn font_sizes_convert_from_points_to_logical_pixels() {
        assert!((points_to_pixels(12.0) - 16.0).abs() < f32::EPSILON);
        assert!((points_to_pixels(9.0) - 12.0).abs() < f32::EPSILON);
    }

    #[test]
    fn os_appearance_changes_switch_the_theme() {
        let mut app = App::default();

        let _ = app.update(Message::AppearanceChanged(Mode::Dark));
        assert_eq!(app.editor().theme().name, "adwaita-dark");

        let _ = app.update(Message::AppearanceChanged(Mode::None));
        assert_eq!(app.editor().theme().name, "adwaita-dark");

        let _ = app.update(Message::AppearanceChanged(Mode::Light));
        assert_eq!(app.editor().theme().name, "adwaita");
    }

    #[test]
    fn iced_theme_is_built_from_the_editor_theme() {
        let palette = App::default().theme().palette();

        assert_eq!(palette.background, iced::Color::from_rgb8(0xED, 0xED, 0xED));
        assert_eq!(palette.text, iced::Color::from_rgb8(0x2E, 0x34, 0x36));
    }

    #[test]
    fn m_x_lists_commands_with_their_keys() {
        let mut app = App::default();
        press(&mut app, "x", Modifiers::ALT);
        let mut ui = iced_test::simulator(app.view());

        assert!(ui.find("end-of-buffer").is_ok());
        assert!(ui.find("M->").is_ok());
    }

    #[test]
    fn title_names_the_current_buffer() {
        assert_eq!(App::default().title(), "*scratch* - mx");
    }

    #[test]
    fn key_presses_reach_the_editor() {
        let mut app = App::default();

        press(&mut app, "a", Modifiers::empty());

        assert_eq!(app.editor().buffer().contents(), "a");
    }

    #[test]
    fn key_releases_are_ignored() {
        let mut app = App::default();

        let _ = app.update(Message::Keyboard(keyboard::Event::KeyReleased {
            key: Key::Character("a".into()),
            modified_key: Key::Character("a".into()),
            physical_key: Physical::Code(Code::KeyA),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
        }));

        assert_eq!(app.editor().buffer().contents(), "");
    }

    #[test]
    fn window_rows_resize_the_editor_viewport() {
        let mut app = App::default();

        let _ = app.update(Message::View(ViewEvent::RowsChanged(7)));

        assert_eq!(app.editor().viewport().rows(), 7);
    }

    #[test]
    fn quit_sequence_sets_the_editor_to_quit() {
        let mut app = App::default();

        press(&mut app, "x", Modifiers::CTRL);
        press(&mut app, "c", Modifiers::CTRL);

        assert!(app.editor().should_quit());
    }
}
