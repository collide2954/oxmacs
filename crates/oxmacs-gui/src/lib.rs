pub mod animation;
pub mod app;
pub mod crash;
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
use oxmacs_core::config::WindowMode;
use oxmacs_core::editor::Visit;
use oxmacs_core::folders::Folders;

pub fn run(visits: Vec<Visit>, init: bool) -> Result {
    let home = oxmacs_api::home();
    crash::install(
        home.as_deref()
            .map(|home| Folders::for_home(home).crashes()),
    );
    let booted = App::boot_with(&visits, home.as_deref(), init);
    let mode = booted.0.editor().config().window_mode;
    let booted = Cell::new(Some(booted));
    iced::application(
        move || {
            booted
                .take()
                .unwrap_or_else(|| App::boot_with(&visits, home.as_deref(), init))
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

pub const APP_ID: &str = "io.github.collide2954.oxmacs";

fn window_settings(mode: WindowMode) -> window::Settings {
    window::Settings {
        size: Size::new(960.0, 720.0),
        maximized: mode == WindowMode::Maximized,
        fullscreen: mode == WindowMode::Fullscreen,
        exit_on_close_request: false,
        platform_specific: platform(),
        ..window::Settings::default()
    }
}

#[cfg(target_os = "linux")]
fn platform() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific {
        application_id: APP_ID.to_owned(),
        ..window::settings::PlatformSpecific::default()
    }
}

#[cfg(not(target_os = "linux"))]
fn platform() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific::default()
}

#[cfg(test)]
mod tests {
    use oxmacs_core::config::WindowMode;

    use super::*;

    #[test]
    fn the_window_opens_in_the_configured_mode() {
        let windowed = window_settings(WindowMode::Windowed);
        assert!(!windowed.maximized && !windowed.fullscreen);
        assert!(!windowed.exit_on_close_request);
        assert_eq!(windowed.size, Size::new(960.0, 720.0));

        assert!(window_settings(WindowMode::Maximized).maximized);
        assert!(window_settings(WindowMode::Fullscreen).fullscreen);
    }

    #[test]
    fn the_desktop_entry_is_named_after_the_app_id() {
        let linux = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/linux");
        assert!(linux.join(format!("{APP_ID}.desktop")).is_file());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_windows_carry_the_app_id() {
        let settings = window_settings(WindowMode::Windowed);
        assert_eq!(settings.platform_specific.application_id, APP_ID);
    }
}
