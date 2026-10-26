mod update;
mod view;

use std::path::{Path, PathBuf};

use iced::animation::Easing;
use iced::theme::Mode;
use iced::time::{Duration, Instant};
use iced::{Animation, Event, Font, Subscription, Task, Theme, event, keyboard, system, window};
use native_theme::detect::prefers_reduced_motion;
use oxmacs_core::clock;
use oxmacs_core::config::{Config, ModeLineStyle};
use oxmacs_core::editor::Visit;
use oxmacs_core::editor::{Editor, FetchId, JobId, SERVERS, ServerId, WindowId};
use oxmacs_core::external::lsp;
use oxmacs_core::external::managed::Managed;
use oxmacs_core::external::process::{self, Handle};
use oxmacs_core::external::usage::Usage;
use oxmacs_core::text::BufferId;
use oxmacs_core::theme::Theme as Faces;
use oxmacs_script::{Ox, Pace, Wake, api::games, view::takeover};

use crate::watcher::Watched;
use crate::widgets::buffer_view::ViewEvent;
use crate::widgets::tab_bar::TabMessage;
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
    usage: Usage,
    jobs: Vec<(JobId, Handle)>,
    servers: Vec<(ServerId, lsp::Server)>,
    ox: Ox,
    takeover_size: Option<iced::Size>,
    focused_since: Option<Instant>,
    tab_hover: Option<usize>,
    tab_drag: Option<usize>,
    before_fullscreen: Option<window::Mode>,
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
    Job(JobId, process::Event),
    AutoSave,
    AutoComplete,
    Signature,
    Reparse,
    ProcessTick,
    FocusWindow(WindowId),
    Fetched(FetchId, Result<String, String>),
    Server(ServerId, lsp::Event),
    ToolInstalled(Managed, Result<PathBuf, String>),
    OxTick,
    OxPress(BufferId, usize),
    OxCall(BufferId, usize, Vec<oxmacs_script::view::views::Arg>),
    ViewSized(BufferId, iced::Size),
    TakeoverSized(iced::Size),
    EndTakeover,
    Tab(TabMessage),
    SideDragged(f32),
    WindowModeBefore(window::Id, window::Mode),
    Focus(bool),
    Idle,
    CloseRequested,
}

impl Default for App {
    fn default() -> Self {
        Self::new(&Config::default())
    }
}

impl App {
    pub fn new(config: &Config) -> Self {
        let mut editor = Editor::new();
        editor.apply_config(config);
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
            usage: Usage::new(),
            jobs: Vec::new(),
            servers: Vec::new(),
            ox: Ox::new(),
            takeover_size: None,
            focused_since: Some(Instant::now()),
            tab_hover: None,
            tab_drag: None,
            before_fullscreen: None,
        };
        app.configure(config);
        app
    }

    pub fn boot(visits: &[Visit], home: Option<&Path>) -> (Self, Task<Message>) {
        Self::boot_with(visits, home, true)
    }

    pub fn boot_with(visits: &[Visit], home: Option<&Path>, init: bool) -> (Self, Task<Message>) {
        let mut app = Self::default();
        app.editor.add_language_servers(SERVERS);
        app.editor.set_recovery_folder(
            home.and(dirs::cache_dir())
                .map(|cache| cache.join("oxmacs").join("recovery")),
        );
        let started = if init {
            app.start_ox(home)
        } else {
            app.ox.start_without_init(&mut app.editor, home);
            app.pending_effects()
        };
        let splash = app.editor.config().splash;
        app.start(visits, splash);
        app.reduced_motion = prefers_reduced_motion();
        app.configure(&app.editor.config().clone());
        let effects = app.pending_effects();
        (
            app,
            Task::batch([
                system::theme().map(Message::AppearanceChanged),
                started,
                effects,
                Task::done(Message::OxTick),
            ]),
        )
    }

    pub fn start_ox(&mut self, home: Option<&Path>) -> Task<Message> {
        self.ox.start(&mut self.editor, home);
        self.pending_effects()
    }

    fn configure(&mut self, config: &Config) {
        if self.family != config.font_family {
            self.family.clone_from(&config.font_family);
            self.font = config
                .font_family
                .clone()
                .map_or(fonts::EDITOR, |family| Font::with_name(family.leak()));
        }

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
        fading
            || self.reveal.is_animating(self.now)
            || takeover::current(&self.editor).is_some()
            || games::active(&self.editor)
    }

    fn rested_since(&self) -> Option<Instant> {
        self.focused_since
            .map(|focused| focused.max(self.last_input))
    }

    pub fn idle_wait(&self) -> Option<(Duration, Instant)> {
        let since = self.rested_since()?;
        self.ox
            .idle_wait(&self.editor, since)
            .map(|wait| (wait, since))
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
        let completing = if self.editor.wants_auto_complete() {
            ticker::once_after(AUTO_COMPLETE_DELAY, self.last_input).map(|()| Message::AutoComplete)
        } else {
            Subscription::none()
        };
        let signature = if self.editor.wants_signature() {
            ticker::once_after(SIGNATURE_DELAY, self.last_input).map(|()| Message::Signature)
        } else {
            Subscription::none()
        };
        let reparse = if self.editor.wants_reparse() {
            ticker::once_after(REPARSE_DELAY, self.last_input).map(|()| Message::Reparse)
        } else {
            Subscription::none()
        };
        let processes = if self.editor.wants_process_usage() {
            ticker::ticks(Some(ticker::SECOND)).map(|()| Message::ProcessTick)
        } else {
            Subscription::none()
        };
        let frames = if self.is_animating() || self.ox.pace() == Pace::Running {
            window::frames().map(Message::Tick)
        } else {
            Subscription::none()
        };
        let resting = self
            .idle_wait()
            .map_or_else(Subscription::none, |(wait, since)| {
                ticker::once_after(wait, since).map(|()| Message::Idle)
            });
        let waiting = match (self.ox.pace(), self.ox.next_wake()) {
            (Pace::Waiting, Wake::Soon) => ticker::ticks(Some(OX_POLL)).map(|()| Message::OxTick),
            (Pace::Waiting, Wake::At(deadline)) => ticker::at(deadline).map(|()| Message::OxTick),
            _ => Subscription::none(),
        };
        Subscription::batch([
            keyboard::listen().map(Message::Keyboard),
            window::close_requests().map(|_| Message::CloseRequested),
            system::theme_changes().map(Message::AppearanceChanged),
            watcher::changes(Watched {
                files: self.editor.visited_files(),
                directories: self.editor.watched_directories(),
            })
            .map(|()| Message::DiskChanged),
            event::listen_with(focus_changes),
            ticker::ticks(self.clock.as_deref().map(|format| {
                if clock::shows_seconds(format) {
                    ticker::SECOND
                } else {
                    ticker::MINUTE
                }
            }))
            .map(|()| Message::Clock),
            idle,
            completing,
            signature,
            reparse,
            processes,
            frames,
            waiting,
            resting,
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
const AUTO_COMPLETE_DELAY: Duration = Duration::from_millis(250);
const SIGNATURE_DELAY: Duration = Duration::from_millis(400);
const REPARSE_DELAY: Duration = Duration::from_millis(250);
const OX_POLL: Duration = Duration::from_millis(100);

fn transition(state: bool, duration: Duration) -> Animation<bool> {
    Animation::new(state)
        .duration(duration)
        .easing(Easing::EaseOutCubic)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "event::listen_with requires this signature"
)]
fn focus_changes(event: Event, _: event::Status, _: window::Id) -> Option<Message> {
    match event {
        Event::Window(window::Event::Focused) => Some(Message::Focus(true)),
        Event::Window(window::Event::Unfocused) => Some(Message::Focus(false)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use iced::keyboard::key::{Code, Named, Physical};
    use iced::keyboard::{Key, Location, Modifiers};
    use oxmacs_core::editor::{Cell, Clicks, JobId, Outcome};
    use oxmacs_core::theme::{ADWAITA, ADWAITA_DARK};
    use serde_json::{Value, json};

    fn configured(settings: &Value) -> Config {
        Config::from_settings(settings).unwrap()
    }

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
        let config = configured(&json!({"mode-line-style": "minimal"}));
        let mut app = App::new(&config);
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
    fn ox_starts_from_init_ox_in_the_home_folder() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("init.ox"),
            r#"(defcommand greet "Say hello" (message "Hello"))"#,
        )
        .unwrap();
        let mut app = App::default();

        let _ = app.start_ox(Some(home.path()));

        assert!(app.editor().command_named("greet").is_some());
    }

    #[test]
    fn ox_views_draw_in_their_buffer_and_buttons_run_ox() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("init.ox"),
            r#"(module (import (ox ui :only [column text button])))
(def count (atom 0))
(open-view "*counter*" (fn [] (column (text "Forecast" :face :heading) (text "Count {@count}") (button "More" :on-press (fn [] (swap! count inc))))))"#,
        )
        .unwrap();
        let mut app = App::default();
        let _ = app.start_ox(Some(home.path()));

        let mut ui = iced_test::simulator(app.view());
        assert!(ui.find("Forecast").is_ok());
        assert!(ui.find("Count 0").is_ok());
        ui.click("More").unwrap();
        let messages: Vec<Message> = ui.into_messages().collect();
        for message in messages {
            let _ = app.update(message);
        }

        let mut ui = iced_test::simulator(app.view());
        assert!(ui.find("Count 1").is_ok());
    }

    #[test]
    fn plugin_segments_show_in_the_mode_line() {
        let (mut app, _) = App::boot(&[], None);
        app.editor
            .add_mode_line_segment(|_| Some("☂ bring one".to_owned()));

        let mut ui = iced_test::simulator(app.view());

        assert!(ui.find("☂ bring one").is_ok());
    }

    #[test]
    fn plugin_overlays_float_over_the_windows() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("init.ox"),
            r#"(set-overlay :keys [[["C-x" :keycap] ["C-s" :keycap] "save-buffer"]])"#,
        )
        .unwrap();
        let mut app = App::default();
        let _ = app.start_ox(Some(home.path()));

        let mut ui = iced_test::simulator(app.view());

        assert!(ui.find("save-buffer").is_ok());
        assert!(ui.find("C-s").is_ok());
        assert!(ui.find("-- *scratch*   L1 C0   (Ox)").is_ok());
    }

    fn with_tab_bar() -> App {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("init.ox"),
            r#"(set-tab-bar ["main" "notes"] :selected 0
  :on-select (fn [i] (message "picked {i}"))
  :on-close (fn [i] (message "closed {i}"))
  :on-new (fn [] (message "new tab"))
  :on-move (fn [from to] (message "moved {from} to {to}")))"#,
        )
        .unwrap();
        let mut app = App::default();
        let _ = app.start_ox(Some(home.path()));
        app
    }

    fn click(app: &mut App, label: &str) {
        let mut ui = iced_test::simulator(app.view());
        ui.click(label).unwrap();
        let messages: Vec<Message> = ui.into_messages().collect();
        for message in messages {
            let _ = app.update(message);
        }
    }

    #[test]
    fn plugin_tab_bars_show_above_the_windows_and_answer_clicks() {
        let mut app = with_tab_bar();
        {
            let mut ui = iced_test::simulator(app.view());
            assert!(ui.find("main").is_ok());
            assert!(ui.find("-- *scratch*   L1 C0   (Ox)").is_ok());
        }

        click(&mut app, "notes");
        assert_eq!(app.editor().message(), Some("picked 1"));
        click(&mut app, "+");
        assert_eq!(app.editor().message(), Some("new tab"));
        click(&mut app, "×");
        assert_eq!(app.editor().message(), Some("closed 0"));
    }

    #[test]
    fn dragging_a_tab_onto_another_moves_it() {
        use crate::widgets::tab_bar::TabMessage;

        let mut app = with_tab_bar();

        for message in [
            TabMessage::Pressed(0),
            TabMessage::Entered(1),
            TabMessage::Released(1),
            TabMessage::BarReleased,
        ] {
            let _ = app.update(Message::Tab(message));
        }
        assert_eq!(app.editor().message(), Some("moved 0 to 1"));

        for message in [TabMessage::Released(0), TabMessage::BarReleased] {
            let _ = app.update(Message::Tab(message));
        }
        assert_eq!(app.editor().message(), Some("moved 0 to 1"));
    }

    fn with_side_window() -> App {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("init.ox"),
            r#"(open-buffer "*tree*" ["src" "README.md"] :show false
  :on-drop (fn [from to] (message "dropped {from} on {to}")))
(show-side-window "*tree*" :width 20)"#,
        )
        .unwrap();
        let mut app = App::default();
        let _ = app.start_ox(Some(home.path()));
        app
    }

    #[test]
    fn side_windows_resize_by_whole_columns_as_their_divider_moves() {
        let mut app = with_side_window();
        let cell = crate::units::cell_width(app.text_size());

        let _ = app.update(Message::SideDragged(cell * 4.2));
        assert_eq!(app.editor().side_window().unwrap().columns, 24);
        let _ = app.update(Message::SideDragged(cell * -30.0));
        assert_eq!(app.editor().side_window().unwrap().columns, 10);

        let mut ui = iced_test::simulator(app.view());
        assert!(ui.find("-- *scratch*   L1 C0   (Ox)").is_ok());
    }

    #[test]
    fn dropping_a_line_on_another_in_a_side_window_reaches_its_plugin() {
        let mut app = with_side_window();
        let side = app.editor().side_window().unwrap().window;
        let at = |row| Cell { row, column: 0 };

        for event in [
            ViewEvent::Resized {
                rows: 10,
                columns: 20,
            },
            ViewEvent::Pressed {
                cell: at(0),
                clicks: Clicks::Single,
                extend: false,
            },
            ViewEvent::Dragged(at(1)),
            ViewEvent::Released(at(1)),
        ] {
            let _ = app.update(Message::View(side, event));
        }

        assert_eq!(app.editor().message(), Some("dropped 0 on 1"));
    }

    #[test]
    fn hidden_chrome_drops_the_mode_lines_tab_bar_and_side_window() {
        let mut app = with_tab_bar();
        app.editor
            .set_presentation(oxmacs_core::editor::Presentation {
                hide_chrome: true,
                column: Some(60),
                ..oxmacs_core::editor::Presentation::default()
            });

        let mut ui = iced_test::simulator(app.view());

        assert!(ui.find("-- *scratch*   L1 C0   (Ox)").is_err());
        assert!(ui.find("notes").is_err());
    }

    #[test]
    fn book_pages_tell_ox_where_each_page_starts() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("init.ox"),
            r#"(module (import (ox ui)))
(def heard (atom nil))
(def words "It was a bright cold day in April, and the clocks were striking thirteen. ")
(def doc (ui/document (vec (map (fn [_] {kind: :paragraph runs: [(apply str (repeat 6 words))]}) (range 40)))))
(open-view "*book*" (fn [] (ui/pages doc :spread :single :on-pages (fn [starts turn] (reset! heard [(count starts) turn (first starts)])))))"#,
        )
        .unwrap();
        let mut app = App::default();
        let _ = app.start_ox(Some(home.path()));

        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            iced::Size::new(800.0, 600.0),
            app.view(),
        );
        let _ = ui.simulate([Event::Mouse(iced::mouse::Event::CursorMoved {
            position: iced::Point::new(10.0, 10.0),
        })]);
        let messages: Vec<Message> = ui.into_messages().collect();
        for message in messages {
            let _ = app.update(message);
        }

        let heard = app.ox.eval(&mut app.editor, "@heard").unwrap();
        let pages: usize = heard
            .trim_start_matches('[')
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(pages > 3, "{heard}");
        assert!(heard.ends_with(" 1 [0 0]]"), "{heard}");
    }

    fn taken_over() -> App {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("init.ox"),
            r#"(take-over (fn [frame] [[:rect 0 0 frame.width frame.height :link]])
                         :on-end (fn [] (message "back")))"#,
        )
        .unwrap();
        let mut app = App::default();
        let _ = app.start_ox(Some(home.path()));
        app
    }

    #[test]
    fn a_takeover_covers_the_windows_but_keeps_a_mode_line_and_the_echo_area() {
        let mut app = taken_over();
        app.ox
            .eval(
                &mut app.editor,
                r#"(module (import (ox ui :only [text])))
(open-view "*hidden*" (fn [] (text "under the screensaver")))"#,
            )
            .unwrap();
        app.ox.tick(&mut app.editor);
        app.editor
            .add_mode_line_segment(|_| Some("☂ bring one".to_owned()));
        app.editor.show_message("still here");

        let mut ui = iced_test::simulator(app.view());

        assert!(ui.find("under the screensaver").is_err());
        assert!(ui.find("☂ bring one").is_ok());
        assert!(ui.find("still here").is_ok());
    }

    #[test]
    fn frames_draw_the_takeover_at_its_size() {
        use oxmacs_core::theme::Highlight;
        use oxmacs_script::view::shapes::{Ink, Paint, Shape};

        let mut app = taken_over();
        assert!(app.is_animating());

        let _ = app.update(Message::TakeoverSized(iced::Size::new(300.0, 200.0)));
        let _ = app.update(Message::Tick(Instant::now()));

        let shown = takeover::current(app.editor()).unwrap();
        assert_eq!(
            shown.shapes,
            [Shape::Rect {
                x: 0.0,
                y: 0.0,
                width: 300.0,
                height: 200.0,
                paint: Paint {
                    ink: Ink::Face(Highlight::Link),
                    alpha: 1.0,
                },
            }]
        );
    }

    #[test]
    fn the_first_key_ends_a_takeover_and_does_nothing_else() {
        let mut app = taken_over();

        press(&mut app, "x", Modifiers::empty());

        assert!(takeover::current(app.editor()).is_none());
        assert_eq!(app.editor().buffer().contents(), "");
        assert_eq!(app.editor().message(), Some("back"));
        press(&mut app, "y", Modifiers::empty());
        assert_eq!(app.editor().buffer().contents(), "y");
    }

    #[test]
    fn a_click_ends_a_takeover() {
        let mut app = taken_over();

        let _ = app.update(Message::EndTakeover);

        assert!(takeover::current(app.editor()).is_none());
    }

    fn playing_game() -> (App, tempfile::TempDir) {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("game.ox");
        std::fs::write(
            &path,
            r##"(module (import (rodeo)))
(defonce heard (atom []))
(def (tick) (when (rodeo/pressed? :a) (swap! heard conj :a)))
(def (draw) (rodeo/clear "#ff0000") (rodeo/text "SCORE 3" [2 2]))
"##,
        )
        .unwrap();
        let mut app = App::default();
        let shown = path.display().to_string().replace('\\', "/");
        app.ox
            .eval(
                &mut app.editor,
                &format!("(module (import (rodeo))) (play-game \"{shown}\")"),
            )
            .unwrap();
        (app, folder)
    }

    #[test]
    fn a_running_game_keeps_frames_coming_and_fills_its_screen() {
        let (mut app, _folder) = playing_game();
        assert!(app.is_animating());

        let _ = app.update(Message::Tick(Instant::now()));

        let id = app.editor().buffer_named("*game*").unwrap();
        let screen = games::screen(app.editor(), id).unwrap();
        assert!(screen.generation > 0);
        assert_eq!(&screen.pixels[..4], &[255, 0, 0, 255]);
        assert_eq!(screen.captions[0].text, "SCORE 3");
        let _ = iced_test::simulator(app.view());
    }

    #[test]
    fn keys_go_to_a_selected_game_but_c_x_stays_with_the_editor() {
        let (mut app, _folder) = playing_game();
        let start = Instant::now();
        let _ = app.update(Message::Tick(start));
        let game = app.editor().buffer().id();

        press(&mut app, "a", Modifiers::empty());
        let _ = app.update(Message::Tick(start + Duration::from_millis(20)));

        assert_eq!(app.ox.eval(&mut app.editor, "@heard").unwrap(), "[:a]");
        assert_eq!(app.editor().message(), None);
        press(&mut app, "x", Modifiers::CTRL);
        press(&mut app, "o", Modifiers::empty());
        assert_ne!(app.editor().buffer().id(), game);
    }

    #[test]
    fn view_echoes_the_editor_message() {
        let mut app = App::default();
        press(&mut app, "q", Modifiers::CTRL);
        let mut ui = iced_test::simulator(app.view());

        assert!(ui.find("C-q is undefined").is_ok());
    }

    #[test]
    fn booting_applies_init_ox_settings_before_anything_shows() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("init.ox"),
            "(settings {theme: :adwaita-dark font-size: 12 window-mode: :maximized splash: false})",
        )
        .unwrap();

        let (app, _) = App::boot(&[], Some(home.path()));

        assert_eq!(app.editor().theme().name, "adwaita-dark");
        assert!((app.text_size() - 16.0).abs() < f32::EPSILON);
        assert_eq!(
            app.editor().config().window_mode,
            oxmacs_core::config::WindowMode::Maximized
        );
        assert_eq!(app.editor().buffer().name(), "*scratch*");
    }

    #[test]
    fn booting_without_init_files_keeps_the_default_settings() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("init.ox"),
            "(settings {window-mode: :maximized splash: false})",
        )
        .unwrap();

        let (app, _) = App::boot_with(&[], Some(home.path()), false);

        assert_eq!(app.editor().config(), &Config::default());
        assert_eq!(app.editor().buffer().name(), "*oxmacs*");
    }

    #[test]
    fn booting_without_a_home_shows_the_start_screen() {
        let (app, _) = App::boot(&[], None);

        assert_eq!(app.editor().buffer().name(), "*oxmacs*");
    }

    #[test]
    fn config_sets_theme_and_font_size() {
        let config = configured(&json!({"theme": "adwaita-dark", "font-size": 12}));

        let app = App::new(&config);

        assert_eq!(app.editor().theme().name, "adwaita-dark");
        assert!((app.text_size() - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn proportional_fonts_fall_back_to_the_bundled_font() {
        let config = configured(&json!({"font-family": "Comic Sans MS"}));
        let mut app = App::new(&config);
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
        let config = configured(&json!({"animations": false}));
        let mut app = App::new(&config);

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
        let config = configured(&json!({"animations": false}));
        let mut app = App::new(&config);

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

        let config = configured(&json!({"animations": false}));
        assert!(!App::new(&config).animations());
    }

    #[test]
    fn auto_save_messages_keep_recovery_copies_of_idle_buffers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, "hello\n").unwrap();
        let mut app = App::default();
        app.editor
            .set_recovery_folder(Some(dir.path().join("recovery")));
        app.start(&[visit(&path)], false);
        press(&mut app, "x", Modifiers::empty());
        assert!(app.editor().wants_auto_save());

        let _ = app.update(Message::AutoSave);

        assert!(app.editor().buffer().is_modified());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello\n");
        assert_eq!(
            std::fs::read_dir(dir.path().join("recovery"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn closing_the_window_asks_about_unsaved_files_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, "hello\n").unwrap();
        let mut app = App::default();
        app.start(&[visit(&path)], false);
        press(&mut app, "x", Modifiers::empty());

        let _ = app.update(Message::CloseRequested);

        assert!(!app.editor().should_quit());
        assert!(
            app.editor()
                .question()
                .is_some_and(|question| question.prompt().starts_with("Save file "))
        );
        press(&mut app, "n", Modifiers::empty());
        assert!(app.editor().should_quit());
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
    fn reconfiguring_updates_the_view_settings() {
        let mut app = App::default();

        app.configure(&configured(
            &json!({"font-size": 12, "clock": false, "animations": false}),
        ));

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
    fn the_window_reports_gaining_and_losing_focus() {
        let check = |event| {
            focus_changes(
                Event::Window(event),
                event::Status::Ignored,
                window::Id::unique(),
            )
        };

        assert!(matches!(
            check(window::Event::Focused),
            Some(Message::Focus(true))
        ));
        assert!(matches!(
            check(window::Event::Unfocused),
            Some(Message::Focus(false))
        ));
        assert!(check(window::Event::Closed).is_none());
    }

    #[test]
    fn regaining_focus_checks_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "old").unwrap();
        let mut app = App::default();
        app.start(&[visit(&path)], true);
        std::fs::write(&path, "new text").unwrap();

        let _ = app.update(Message::Focus(true));

        assert_eq!(app.editor().buffer().contents(), "new text");
    }

    #[test]
    fn idle_hooks_wait_for_a_rest_while_the_window_has_focus() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("init.ox"),
            r#"(on-idle :nap 60 (fn [] (message "napping")))"#,
        )
        .unwrap();
        let mut app = App::default();
        let _ = app.start_ox(Some(home.path()));
        app.editor.set_major_mode(None);
        let long_ago = Instant::now().checked_sub(Duration::from_secs(61)).unwrap();

        let _ = app.update(Message::Focus(true));
        assert_eq!(
            app.idle_wait().map(|(wait, _)| wait),
            Some(Duration::from_secs(60))
        );
        let _ = app.update(Message::Focus(false));
        assert_eq!(app.idle_wait(), None);
        app.last_input = long_ago;
        let _ = app.update(Message::Idle);
        assert_ne!(app.editor().message(), Some("napping"));

        let _ = app.update(Message::Focus(true));
        app.focused_since = Some(long_ago);
        let _ = app.update(Message::Idle);
        assert_eq!(app.editor().message(), Some("napping"));
        assert_eq!(app.idle_wait(), None);

        press(&mut app, "x", Modifiers::empty());
        assert_eq!(
            app.idle_wait().map(|(wait, _)| wait),
            Some(Duration::from_secs(60))
        );
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
    fn a_proportional_font_is_named_and_replaced() {
        let config = configured(&json!({"font-family": "Comic Sans MS"}));
        let mut app = App::new(&config);
        let _ = app.update(Message::View(
            app.editor().selected_window(),
            ViewEvent::FontNotMonospaced,
        ));

        assert_eq!(
            app.editor().message(),
            Some("Comic Sans MS is not a monospaced font; using JetBrains Mono")
        );
    }

    #[test]
    fn the_clock_is_on_unless_turned_off() {
        assert_eq!(App::default().clock_format(), Some("%H:%M"));

        let config = configured(&json!({"clock": false}));
        assert_eq!(App::new(&config).clock_format(), None);
    }

    #[test]
    fn the_mode_line_shows_the_clock() {
        let config = configured(&json!({"clock-format": "teatime"}));
        let app = App::new(&config);
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
