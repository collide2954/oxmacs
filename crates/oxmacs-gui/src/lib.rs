pub mod animation;
pub mod app;
mod fetch;
pub mod fonts;
pub mod keys;
pub mod style;
pub mod ticker;
pub mod units;
pub mod watcher;
pub mod widgets;

pub use iced::Result;

use std::cell::Cell;

use app::App;
use iced::{Size, window};
use oxmacs_core::cli::Visit;
use oxmacs_core::config::WindowMode;

pub fn run(visits: Vec<Visit>) -> Result {
    let home = oxmacs_ox::home();
    let booted = App::boot(&visits, home.as_deref());
    let mode = booted.0.editor().config().window_mode;
    let booted = Cell::new(Some(booted));
    iced::application(
        move || {
            booted
                .take()
                .unwrap_or_else(|| App::boot(&visits, home.as_deref()))
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
