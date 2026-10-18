use iced::animation::Easing;
use iced::font::Family;
use iced::theme::Mode;
use iced::time::{Duration, Instant};
use iced::widget::{column, container, text};
use iced::{
    Animation, Element, Event, Font, Length, Subscription, Task, Theme, clipboard, event, keyboard,
    system, window,
};
use mx_core::cli::Visit;
use mx_core::config::{self, Config, ConfigError};
use mx_core::editor::{Editor, Effect};
use mx_core::theme::{Appearance, Theme as Faces};
use native_theme::detect::prefers_reduced_motion;

use crate::buffer_view::{BufferView, Motion, ViewEvent};
use crate::keys::{self, Platform};
use crate::{animation, completions, echo_area, fonts, style, watcher};

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
    Pasted(Option<String>),
    PastedPrimary(Option<String>),
    DiskChanged,
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

    pub fn boot(visits: &[Visit]) -> (Self, Task<Message>) {
        let config = config::default_path()
            .map_or_else(|| Ok(Config::default()), |path| Config::load(&path));
        let mut app = Self::new(config);
        app.open(visits);
        app.animations = animations_enabled(app.animations, prefers_reduced_motion());
        (app, system::theme().map(Message::AppearanceChanged))
    }

    pub fn open(&mut self, visits: &[Visit]) {
        for visit in visits {
            self.editor.visit(visit);
        }
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

    fn handle_view_event(&mut self, event: ViewEvent) {
        match event {
            ViewEvent::RowsChanged(rows) => self.editor.resize_window(rows),
            ViewEvent::FontNotMonospaced => self.reject_font(),
            ViewEvent::Pressed {
                cell,
                clicks,
                extend,
            } => {
                self.last_input = self.now;
                self.editor.click(cell, clicks, extend);
            }
            ViewEvent::Dragged(cell) => self.editor.drag(cell),
            ViewEvent::MiddleClicked(cell) => {
                self.last_input = self.now;
                self.editor.middle_click(cell);
            }
            ViewEvent::Scrolled(rows) => self.editor.scroll_lines(rows),
        }
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
            Message::View(event) => self.handle_view_event(event),
            Message::AppearanceChanged(mode) => {
                if let Some(appearance) = appearance(mode) {
                    self.set_appearance(appearance);
                }
            }
            Message::Tick(now) => self.tick(now),
            Message::Pasted(text) => self.editor.paste(text),
            Message::PastedPrimary(text) => self.editor.paste_primary(text),
            Message::DiskChanged => self.editor.check_disk_changes(),
        }
        if self.editor.should_quit() {
            return iced::exit();
        }
        Task::batch(self.editor.take_effects().into_iter().map(effect_task))
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
            watcher::changes(self.editor.visited_files()).map(|()| Message::DiskChanged),
            event::listen_with(focus_gained),
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

pub fn animations_enabled(configured: bool, reduced_motion: bool) -> bool {
    configured && !reduced_motion
}

const HAS_PRIMARY_SELECTION: bool = cfg!(target_os = "linux");

fn effect_task(effect: Effect) -> Task<Message> {
    match effect {
        Effect::WriteClipboard(text) => clipboard::write(text),
        Effect::ReadClipboard => clipboard::read().map(Message::Pasted),
        Effect::WritePrimary(text) if HAS_PRIMARY_SELECTION => clipboard::write_primary(text),
        Effect::ReadPrimary if HAS_PRIMARY_SELECTION => {
            clipboard::read_primary().map(Message::PastedPrimary)
        }
        Effect::WritePrimary(_) | Effect::ReadPrimary => Task::none(),
    }
}

fn transition(state: bool, duration: Duration) -> Animation<bool> {
    Animation::new(state)
        .duration(duration)
        .easing(Easing::EaseOutCubic)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "event::listen_with requires this signature"
)]
fn focus_gained(event: Event, _: event::Status, _: window::Id) -> Option<Message> {
    matches!(event, Event::Window(window::Event::Focused)).then_some(Message::DiskChanged)
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
    use mx_core::editor::{Cell, Clicks};
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

        assert!(ui.find("-- *scratch*   L1 C0").is_ok());
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
    fn os_reduced_motion_turns_animations_off() {
        assert!(animations_enabled(true, false));
        assert!(!animations_enabled(true, true));
        assert!(!animations_enabled(false, false));
        assert!(!animations_enabled(false, true));
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
        for c in ["e", "n", "d"] {
            press(&mut app, c, Modifiers::empty());
        }
        let mut ui = iced_test::simulator(app.view());

        assert!(ui.find("end-of-buffer").is_ok());
        assert!(ui.find("M->").is_ok());
    }

    #[test]
    fn opened_files_follow_disk_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "old").unwrap();
        let mut app = App::default();
        app.open(&[Visit {
            path: path.clone(),
            line: None,
            column: None,
        }]);
        assert_eq!(app.title(), "a.txt - mx");
        std::fs::write(&path, "new text").unwrap();

        let _ = app.update(Message::DiskChanged);

        assert_eq!(app.editor().buffer().contents(), "new text");
    }

    #[test]
    fn regaining_focus_checks_the_disk() {
        let check = |event| {
            focus_gained(
                Event::Window(event),
                event::Status::Ignored,
                window::Id::unique(),
            )
        };

        assert!(matches!(
            check(window::Event::Focused),
            Some(Message::DiskChanged)
        ));
        assert!(check(window::Event::Unfocused).is_none());
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

    fn view_event(app: &mut App, event: ViewEvent) {
        let _ = app.update(Message::View(event));
    }

    #[test]
    fn mouse_events_reach_the_editor() {
        let mut app = App::default();
        for c in ["a", "b", "c", "d"] {
            press(&mut app, c, Modifiers::empty());
        }

        view_event(
            &mut app,
            ViewEvent::Pressed {
                cell: Cell { line: 0, column: 1 },
                clicks: Clicks::Single,
                extend: false,
            },
        );
        view_event(&mut app, ViewEvent::Dragged(Cell { line: 0, column: 3 }));

        assert_eq!(app.editor().buffer().selection(), Some(1..3));
    }

    #[test]
    fn wheel_scrolling_reaches_the_editor() {
        let mut app = App::default();
        let _ = app.update(Message::Pasted(Some("x\n".repeat(100))));
        view_event(&mut app, ViewEvent::RowsChanged(10));
        press(&mut app, "<", Modifiers::ALT);

        view_event(&mut app, ViewEvent::Scrolled(3));

        assert_eq!(app.editor().viewport().top(), 3);
    }

    #[test]
    fn clipboard_contents_are_pasted_into_the_editor() {
        let mut app = App::default();

        let _ = app.update(Message::Pasted(Some("from the os".to_owned())));

        assert_eq!(app.editor().buffer().contents(), "from the os");
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
