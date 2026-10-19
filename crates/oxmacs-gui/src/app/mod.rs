mod update;
mod view;

use iced::animation::Easing;
use iced::theme::Mode;
use iced::time::{Duration, Instant};
use iced::widget::{self, markdown};
use iced::{Animation, Event, Font, Subscription, Task, Theme, event, keyboard, system, window};
use jiff::Timestamp;
use native_theme::detect::prefers_reduced_motion;
use oxmacs_core::cli::Visit;
use oxmacs_core::clock;
use oxmacs_core::config::{self, Config, ConfigError, ModeLineStyle};
use oxmacs_core::editor::{ConnectionId, Editor, FetchId, JobId, WindowId};
use oxmacs_core::external::net::{self, Connection};
use oxmacs_core::external::process::{self, Handle};
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Theme as Faces;

use crate::watcher::Watched;
use crate::widgets::buffer_view::ViewEvent;
use crate::{animation, fonts, style, ticker, watcher};

#[derive(Debug)]
pub struct App {
    editor: Editor,
    font: Font,
    family: Option<String>,
    text_size: f32,
    animations: bool,
    reduced_motion: bool,
    blink_interval: Duration,
    last_input: Instant,
    now: Instant,
    theme_fade: Option<(Faces, Animation<bool>)>,
    reveal: Animation<bool>,
    clock: Option<String>,
    mode_line_style: ModeLineStyle,
    previews: Vec<(BufferId, u64, markdown::Content)>,
    jobs: Vec<(JobId, Handle)>,
    connections: Vec<(ConnectionId, Connection)>,
    preview_heights: Vec<(WindowId, f32)>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Keyboard(keyboard::Event),
    View(WindowId, ViewEvent),
    Divider {
        path: Vec<usize>,
        index: usize,
        delta: f32,
    },
    AppearanceChanged(Mode),
    Tick(Instant),
    Pasted(Option<String>),
    PastedPrimary(Option<String>),
    DiskChanged,
    Opened(Result<(), String>),
    Clock,
    OpenLink(markdown::Uri),
    Job(JobId, process::Event),
    AutoSave,
    FocusWindow(WindowId),
    PreviewSized(WindowId, f32),
    Fetched(FetchId, Result<String, String>),
    Irc(ConnectionId, net::Event),
    WeatherTick,
}

pub fn load_config() -> Result<Config, ConfigError> {
    config::default_path().map_or_else(|| Ok(Config::default()), |path| Config::load(&path))
}

impl Default for App {
    fn default() -> Self {
        Self::new(Ok(&Config::default()))
    }
}

impl App {
    pub fn new(config: Result<&Config, &ConfigError>) -> Self {
        let mut editor = Editor::new();
        let config = config.cloned().unwrap_or_else(|error| {
            editor.show_message(error.to_string());
            Config::default()
        });
        editor.apply_config(&config);
        let mut app = Self {
            editor,
            font: fonts::EDITOR,
            family: None,
            text_size: points_to_pixels(config.font_size),
            animations: config.animations,
            reduced_motion: false,
            blink_interval: Duration::from_millis(config.cursor_blink_interval),
            last_input: Instant::now(),
            now: Instant::now(),
            theme_fade: None,
            reveal: transition(false, animation::REVEAL),
            clock: None,
            mode_line_style: config.mode_line_style,
            previews: Vec::new(),
            jobs: Vec::new(),
            connections: Vec::new(),
            preview_heights: Vec::new(),
        };
        app.configure(&config);
        app
    }

    pub fn boot(config: &Result<Config, ConfigError>, visits: &[Visit]) -> (Self, Task<Message>) {
        let splash = config.as_ref().map_or(true, |config| config.splash);
        let mut app = Self::new(config.as_ref());
        app.start(visits, splash);
        app.reduced_motion = prefers_reduced_motion();
        app.configure(&app.editor.config().clone());
        app.editor.weather_tick(Timestamp::now());
        let effects = app.pending_effects();
        (
            app,
            Task::batch([system::theme().map(Message::AppearanceChanged), effects]),
        )
    }

    fn configure(&mut self, config: &Config) {
        if self.family != config.font_family {
            self.family.clone_from(&config.font_family);
            self.font = config
                .font_family
                .clone()
                .map_or(fonts::EDITOR, |family| Font::with_name(family.leak()));
        }
        self.editor.set_font_note(
            config
                .font_family
                .clone()
                .unwrap_or_else(|| format!("{} (bundled)", fonts::EDITOR_FAMILY)),
        );
        self.text_size = points_to_pixels(config.font_size);
        self.animations = animations_enabled(config.animations, self.reduced_motion);
        self.blink_interval = Duration::from_millis(config.cursor_blink_interval);
        self.clock = config.clock.then(|| config.clock_format.clone());
        self.mode_line_style = config.mode_line_style;
    }

    pub fn start(&mut self, visits: &[Visit], splash: bool) {
        for visit in visits {
            self.editor.visit(visit);
        }
        if visits.is_empty() && splash {
            self.editor.about();
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

    pub fn clock_format(&self) -> Option<&str> {
        self.clock.as_deref()
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

    pub fn subscription(&self) -> Subscription<Message> {
        let idle = if self.editor.wants_auto_save() {
            ticker::once_after(AUTO_SAVE_DELAY, self.last_input).map(|()| Message::AutoSave)
        } else {
            Subscription::none()
        };
        let weather = if self.editor.config().mode_line_weather {
            ticker::ticks(Some(ticker::MINUTE)).map(|()| Message::WeatherTick)
        } else {
            Subscription::none()
        };
        let frames = if self.is_animating() {
            window::frames().map(Message::Tick)
        } else {
            Subscription::none()
        };
        Subscription::batch([
            keyboard::listen().map(Message::Keyboard),
            system::theme_changes().map(Message::AppearanceChanged),
            watcher::changes(Watched {
                files: self.editor.visited_files(),
                directories: self.editor.watched_directories(),
            })
            .map(|()| Message::DiskChanged),
            event::listen_with(focus_gained),
            ticker::ticks(self.clock.as_deref().map(|format| {
                if clock::shows_seconds(format) {
                    ticker::SECOND
                } else {
                    ticker::MINUTE
                }
            }))
            .map(|()| Message::Clock),
            idle,
            weather,
            frames,
        ])
    }

    pub fn title(&self) -> String {
        format!("{} - oxmacs", self.editor.buffer().name())
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
}

pub fn points_to_pixels(points: f32) -> f32 {
    points * 4.0 / 3.0
}

pub fn animations_enabled(configured: bool, reduced_motion: bool) -> bool {
    configured && !reduced_motion
}

const AUTO_SAVE_DELAY: Duration = Duration::from_millis(1500);

fn preview_id(window: WindowId) -> widget::Id {
    widget::Id::from(format!("preview-{window:?}"))
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use iced::keyboard::key::{Code, Named, Physical};
    use iced::keyboard::{Key, Location, Modifiers};
    use oxmacs_core::editor::{Cell, Clicks, JobId, Outcome};
    use oxmacs_core::theme::{ADWAITA, ADWAITA_DARK};

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
    fn the_mode_line_follows_the_configured_style() {
        let config = Config::parse("mode-line-style = \"minimal\"").unwrap();
        let mut app = App::new(Ok(&config));
        {
            let mut ui = iced_test::simulator(app.view());
            assert!(ui.find("*scratch*").is_ok());
            assert!(ui.find("1:0").is_ok());
        }

        app.configure(&Config::default());

        let mut ui = iced_test::simulator(app.view());
        assert!(ui.find("-- *scratch*   L1 C0").is_ok());
    }

    #[test]
    fn command_completions_say_what_each_command_does() {
        let mut app = App::default();
        press(&mut app, "x", Modifiers::ALT);
        for c in ["w", "h", "o", "l", "e"] {
            press(&mut app, c, Modifiers::empty());
        }

        let mut ui = iced_test::simulator(app.view());

        assert!(ui.find("mark-whole-buffer").is_ok());
        assert!(ui.find("C-x h").is_ok());
        assert!(ui.find("Select the whole buffer").is_ok());
    }

    #[test]
    fn the_mode_line_shows_the_weather_when_asked() {
        let config =
            Config::parse("weather-location = \"Berlin\"\nmode-line-weather = true").unwrap();
        let mut app = App::new(Ok(&config));
        let place = r#"{"results":[{"name":"Berlin","latitude":52.5,"longitude":13.4}]}"#;
        let forecast = r#"{"current_units":{"temperature_2m":"°C","wind_speed_10m":"km/h"},
            "current":{"time":"2026-10-19T14:15","temperature_2m":13.2,"apparent_temperature":11.0,
                "relative_humidity_2m":71,"is_day":1,"weather_code":3,"wind_speed_10m":11.5,
                "wind_direction_10m":250},
            "hourly":{"time":[],"temperature_2m":[],"precipitation_probability":[],"weather_code":[]},
            "daily":{"time":[],"weather_code":[],"temperature_2m_max":[],"temperature_2m_min":[],
                "precipitation_probability_max":[],"sunrise":[],"sunset":[]}}"#;

        let _ = app.update(Message::WeatherTick);
        let _ = app.update(Message::Fetched(FetchId(0), Ok(place.to_owned())));
        let _ = app.update(Message::Fetched(FetchId(1), Ok(forecast.to_owned())));

        let mut ui = iced_test::simulator(app.view());
        assert!(ui.find("☁ 13°").is_ok());
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
        let app = App::new(Err(&ConfigError::TabWidth));

        assert_eq!(app.editor().message(), Some("tab-width must be at least 1"));
        assert_eq!(app.editor().tab_width(), 8);
    }

    #[test]
    fn config_sets_theme_and_font_size() {
        let config = Config::parse("theme = \"adwaita-dark\"\nfont-size = 12").unwrap();

        let app = App::new(Ok(&config));

        assert_eq!(app.editor().theme().name, "adwaita-dark");
        assert!((app.text_size() - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn proportional_fonts_fall_back_to_the_bundled_font() {
        let config = Config::parse("font-family = \"Comic Sans MS\"").unwrap();
        let mut app = App::new(Ok(&config));
        assert_ne!(app.font(), fonts::EDITOR);

        let _ = app.update(Message::View(
            app.editor().selected_window(),
            ViewEvent::FontNotMonospaced,
        ));

        assert_eq!(app.font(), fonts::EDITOR);
        assert_eq!(
            app.editor().message(),
            Some("Comic Sans MS is not a monospaced font; using JetBrains Mono")
        );
    }

    #[test]
    fn the_bundled_font_is_never_replaced() {
        let mut app = App::default();

        let _ = app.update(Message::View(
            app.editor().selected_window(),
            ViewEvent::FontNotMonospaced,
        ));

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

    fn named(app: &mut App, named: Named, code: Code) {
        let _ = app.update(Message::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(named),
            modified_key: Key::Named(named),
            physical_key: Physical::Code(code),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
            text: None,
            repeat: false,
        }));
    }

    fn enter(app: &mut App) {
        named(app, Named::Enter, Code::Enter);
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            if c == ' ' {
                named(app, Named::Space, Code::Space);
            } else {
                press(app, &c.to_string(), Modifiers::empty());
            }
        }
    }

    #[test]
    fn job_events_reach_the_compilation_buffer() {
        let mut app = App::default();
        press(&mut app, "x", Modifiers::ALT);
        type_text(&mut app, "compile");
        enter(&mut app);
        press(&mut app, "a", Modifiers::CTRL);
        press(&mut app, "k", Modifiers::CTRL);
        type_text(&mut app, "cargo --version");
        enter(&mut app);

        let _ = app.update(Message::Job(
            JobId(0),
            process::Event::Line("hello".to_owned()),
        ));
        let _ = app.update(Message::Job(
            JobId(0),
            process::Event::Finished(Outcome::Exited(Some(0))),
        ));

        let window = app.editor().tiling().windows()[1];
        let output = app.editor().pane_of(window).unwrap().buffer().contents();
        assert!(output.contains("$ cargo --version\n\nhello\n"), "{output}");
        assert_eq!(app.editor().message(), Some("Compilation finished"));
        assert!(app.jobs.is_empty());
    }

    #[test]
    fn theme_commands_crossfade_too() {
        let mut app = App::default();

        press(&mut app, "x", Modifiers::ALT);
        for c in "toggle-theme".chars() {
            press(&mut app, &c.to_string(), Modifiers::empty());
        }
        enter(&mut app);
        assert!(app.is_animating());
        assert_eq!(app.faces().default.background, ADWAITA.default.background);

        let _ = app.update(Message::Tick(later()));
        assert_eq!(app.faces(), ADWAITA_DARK);
    }

    #[test]
    fn theme_changes_are_instant_without_animations() {
        let config = Config::parse("animations = false").unwrap();
        let mut app = App::new(Ok(&config));

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
        let mut app = App::new(Ok(&config));

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
        assert!(!App::new(Ok(&config)).animations());
    }

    #[test]
    fn auto_save_messages_save_idle_rust_buffers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        let path = dir.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut app = App::default();
        app.start(&[visit(&path)], false);
        press(&mut app, "x", Modifiers::empty());
        assert!(app.editor().wants_auto_save());

        let _ = app.update(Message::AutoSave);

        assert!(!app.editor().buffer().is_modified());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "xfn main() {}\n");
    }

    #[test]
    fn clicking_a_window_without_text_selects_it() {
        let mut app = App::default();
        press(&mut app, "x", Modifiers::CTRL);
        press(&mut app, "3", Modifiers::empty());
        let other = app.editor().tiling().windows()[1];

        let _ = app.update(Message::FocusWindow(other));

        assert_eq!(app.editor().selected_window(), other);
    }

    #[test]
    fn preview_sizes_are_remembered_per_window() {
        let mut app = App::default();
        let window = app.editor().selected_window();

        let _ = app.update(Message::PreviewSized(window, 300.0));
        let _ = app.update(Message::PreviewSized(window, 400.0));

        assert_eq!(app.preview_heights, [(window, 400.0)]);
    }

    #[test]
    fn reconfiguring_updates_the_view_settings() {
        let mut app = App::default();

        app.configure(&Config::parse("font-size = 12\nclock = false\nanimations = false").unwrap());

        assert!((app.text_size() - 16.0).abs() < f32::EPSILON);
        assert_eq!(app.clock_format(), None);
        assert!(!app.animations());
    }

    #[test]
    fn reduced_motion_outlasts_reconfiguring() {
        let mut app = App {
            reduced_motion: true,
            ..App::default()
        };

        app.configure(&Config::default());

        assert!(!app.animations());
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

    fn visit(path: &Path) -> Visit {
        Visit {
            path: path.to_owned(),
            line: None,
            column: None,
        }
    }

    #[test]
    fn starting_without_files_shows_the_splash() {
        let mut app = App::default();

        app.start(&[], true);

        assert_eq!(app.editor().buffer().name(), "*oxmacs*");
    }

    #[test]
    fn starting_with_files_skips_the_splash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "a").unwrap();
        let mut app = App::default();

        app.start(&[visit(&path)], true);

        assert_eq!(app.editor().buffer_names(), ["a.txt", "*scratch*"]);
    }

    #[test]
    fn the_splash_can_be_turned_off() {
        let mut app = App::default();

        app.start(&[], false);

        assert_eq!(app.editor().buffer_names(), ["*scratch*"]);
    }

    #[test]
    fn opened_files_follow_disk_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "old").unwrap();
        let mut app = App::default();
        app.start(&[visit(&path)], true);
        assert_eq!(app.title(), "a.txt - oxmacs");
        std::fs::write(&path, "new text").unwrap();

        let _ = app.update(Message::DiskChanged);

        assert_eq!(app.editor().buffer().contents(), "new text");
    }

    #[test]
    fn failing_to_open_a_file_externally_is_reported() {
        let mut app = App::default();

        let _ = app.update(Message::Opened(Err("a.txt: no handler".to_owned())));

        assert_eq!(app.editor().message(), Some("a.txt: no handler"));
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
    fn pasted_rust_is_highlighted_at_once() {
        let mut app = App::default();
        press(&mut app, "x", Modifiers::ALT);
        for c in "rust-mode".chars() {
            press(&mut app, &c.to_string(), Modifiers::empty());
        }
        enter(&mut app);

        let _ = app.update(Message::Pasted(Some("fn main() {}".to_owned())));

        assert_eq!(
            app.editor().line_style(0).spans[0],
            (0..2, oxmacs_core::theme::Highlight::Keyword)
        );
    }

    #[test]
    fn the_doctor_names_the_font_in_use() {
        let config = Config::parse("font-family = \"Comic Sans MS\"").unwrap();
        let mut app = App::new(Ok(&config));
        let _ = app.update(Message::View(
            app.editor().selected_window(),
            ViewEvent::FontNotMonospaced,
        ));

        press(&mut app, "x", Modifiers::ALT);
        for c in "oxmacs-doctor".chars() {
            press(&mut app, &c.to_string(), Modifiers::empty());
        }
        enter(&mut app);

        assert!(
            app.editor()
                .buffer()
                .contents()
                .contains("Comic Sans MS is not monospaced; using JetBrains Mono")
        );
    }

    #[test]
    fn the_clock_is_on_unless_turned_off() {
        assert_eq!(App::default().clock_format(), Some("%H:%M"));

        let config = Config::parse("clock = false").unwrap();
        assert_eq!(App::new(Ok(&config)).clock_format(), None);
    }

    #[test]
    fn the_mode_line_shows_the_clock() {
        let config = Config::parse("clock-format = \"teatime\"").unwrap();
        let app = App::new(Ok(&config));
        let mut ui = iced_test::simulator(app.view());

        assert!(ui.find("teatime").is_ok());
    }

    fn split(app: &mut App) -> WindowId {
        press(app, "x", Modifiers::CTRL);
        press(app, "3", Modifiers::empty());
        app.editor().tiling().windows()[1]
    }

    #[test]
    fn clicking_another_window_selects_it() {
        let mut app = App::default();
        let other = split(&mut app);

        let _ = app.update(Message::View(
            other,
            ViewEvent::Pressed {
                cell: Cell { row: 0, column: 0 },
                clicks: Clicks::Single,
                extend: false,
            },
        ));

        assert_eq!(app.editor().selected_window(), other);
    }

    #[test]
    fn each_window_has_its_own_size_and_scroll() {
        let mut app = App::default();
        let _ = app.update(Message::Pasted(Some("x\n".repeat(100))));
        press(&mut app, "<", Modifiers::ALT);
        let other = split(&mut app);

        let _ = app.update(Message::View(
            other,
            ViewEvent::Resized {
                rows: 5,
                columns: 40,
            },
        ));
        let _ = app.update(Message::View(other, ViewEvent::Scrolled(3)));

        let pane = app.editor().pane_of(other).unwrap();
        assert_eq!(pane.viewport().rows(), 5);
        assert_eq!(pane.viewport().top().line, 3);
        assert_ne!(app.editor().selected_window(), other);
    }

    #[test]
    fn dragging_a_divider_resizes_the_windows() {
        let mut app = App::default();
        split(&mut app);

        let _ = app.update(Message::Divider {
            path: Vec::new(),
            index: 0,
            delta: 0.1,
        });

        assert_eq!(app.editor().tiling().boundary(&[], 0), Some(0.6));
    }

    #[test]
    fn previews_parse_their_source_again_after_edits() {
        let mut app = App::default();
        press(&mut app, "x", Modifiers::ALT);
        for c in "markdown-mode".chars() {
            press(&mut app, &c.to_string(), Modifiers::empty());
        }
        enter(&mut app);
        let _ = app.update(Message::Pasted(Some("# Title\n".to_owned())));
        press(&mut app, "c", Modifiers::CTRL);
        press(&mut app, "c", Modifiers::CTRL);
        press(&mut app, "p", Modifiers::empty());
        assert_eq!(app.previews.len(), 1);
        let first = app.previews[0].1;

        press(&mut app, "x", Modifiers::empty());

        assert_eq!(app.previews[0].1, app.editor().buffer().revision());
        assert_ne!(app.previews[0].1, first);
    }

    #[test]
    fn title_names_the_current_buffer() {
        assert_eq!(App::default().title(), "*scratch* - oxmacs");
    }

    #[test]
    fn key_presses_reach_the_editor() {
        let mut app = App::default();

        press(&mut app, "a", Modifiers::empty());

        assert_eq!(app.editor().buffer().contents(), "a");
    }

    fn view_event(app: &mut App, event: ViewEvent) {
        let window = app.editor().selected_window();
        let _ = app.update(Message::View(window, event));
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
                cell: Cell { row: 0, column: 1 },
                clicks: Clicks::Single,
                extend: false,
            },
        );
        view_event(&mut app, ViewEvent::Dragged(Cell { row: 0, column: 3 }));

        assert_eq!(app.editor().buffer().selection(), Some(1..3));
    }

    #[test]
    fn wheel_scrolling_reaches_the_editor() {
        let mut app = App::default();
        let _ = app.update(Message::Pasted(Some("x\n".repeat(100))));
        view_event(
            &mut app,
            ViewEvent::Resized {
                rows: 10,
                columns: 0,
            },
        );
        press(&mut app, "<", Modifiers::ALT);

        view_event(&mut app, ViewEvent::Scrolled(3));

        assert_eq!(app.editor().viewport().top().line, 3);
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

        let _ = app.update(Message::View(
            app.editor().selected_window(),
            ViewEvent::Resized {
                rows: 7,
                columns: 80,
            },
        ));

        assert_eq!(app.editor().viewport().rows(), 7);
        assert_eq!(app.editor().viewport().columns(), 80);
    }

    #[test]
    fn quit_sequence_sets_the_editor_to_quit() {
        let mut app = App::default();

        press(&mut app, "x", Modifiers::CTRL);
        press(&mut app, "c", Modifiers::CTRL);

        assert!(app.editor().should_quit());
    }
}
