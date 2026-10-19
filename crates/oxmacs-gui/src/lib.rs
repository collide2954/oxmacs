pub mod animation;
pub mod app;
pub mod fonts;
pub mod keys;
pub mod style;
pub mod ticker;
mod units;
pub mod watcher;
pub mod widgets;

pub use iced::Result;

use app::App;
use iced::{Size, window};
use oxmacs_core::cli::Visit;
use oxmacs_core::config::WindowMode;

pub fn run(visits: Vec<Visit>) -> Result {
    let config = app::load_config();
    let mode = config
        .as_ref()
        .map(|config| config.window_mode)
        .unwrap_or_default();
    iced::application(move || App::boot(&config, &visits), App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .font(fonts::JETBRAINS_MONO_REGULAR)
        .default_font(fonts::EDITOR)
        .window(window_settings(mode))
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
