pub mod animation;
pub mod app;
pub mod buffer_view;
pub mod completions;
pub mod echo_area;
pub mod fonts;
pub mod keys;
pub mod style;
mod units;

pub use iced::Result;

use app::App;

pub fn run() -> Result {
    iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .font(fonts::JETBRAINS_MONO_REGULAR)
        .default_font(fonts::EDITOR)
        .window_size((960.0, 720.0))
        .run()
}
