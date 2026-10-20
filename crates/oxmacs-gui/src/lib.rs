pub mod animation;
pub mod app;
pub mod applet;
mod fetch;
pub mod fonts;
pub mod keys;
pub mod style;
pub mod ticker;
pub mod units;
pub mod watcher;
pub mod widgets;

pub use iced::Result;

use app::App;
use applet::GuiApplet;
use iced::{Size, window};
use oxmacs_core::cli::Visit;
use oxmacs_core::config::{self, WindowMode};
use oxmacs_core::theme;

pub fn run(visits: Vec<Visit>, applets: fn() -> Vec<Box<dyn GuiApplet>>) -> Result {
    for applet in applets() {
        theme::register(applet.themes());
    }
    let note = config::default_path().and_then(|path| {
        config::migrate_file(&path).unwrap_or_else(|error| Some(error.to_string()))
    });
    let config = app::load_config();
    let mode = config
        .as_ref()
        .map(|config| config.window_mode)
        .unwrap_or_default();
    iced::application(
        move || {
            let (mut app, task) = App::boot(&config, &visits, applets());
            app.note(note.clone());
            (app, task)
        },
        App::update,
        App::view,
    )
    .title(App::title)
    .theme(App::theme)
    .subscription(App::subscription)
    .font(fonts::JETBRAINS_MONO_REGULAR)
    .default_font(fonts::EDITOR)
    .window(window_settings(mode))
    .antialiasing(true)
    .run()
}

fn window_settings(mode: WindowMode) -> window::Settings {
    window::Settings {
        size: Size::new(960.0, 720.0),
        maximized: mode == WindowMode::Maximized,
        fullscreen: mode == WindowMode::Fullscreen,
        ..window::Settings::default()
    }
}

#[cfg(test)]
mod tests {
    use oxmacs_core::config::WindowMode;

    use super::*;

    #[test]
    fn the_window_opens_in_the_configured_mode() {
        let windowed = window_settings(WindowMode::Windowed);
        assert!(!windowed.maximized && !windowed.fullscreen);
        assert_eq!(windowed.size, Size::new(960.0, 720.0));

        assert!(window_settings(WindowMode::Maximized).maximized);
        assert!(window_settings(WindowMode::Fullscreen).fullscreen);
    }
}
