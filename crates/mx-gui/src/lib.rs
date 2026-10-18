pub mod app;
pub mod buffer_view;
pub mod fonts;
pub mod keys;

pub use iced::Result;

use app::App;

pub fn run() -> Result {
    iced::application(App::default, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .font(fonts::JETBRAINS_MONO_REGULAR)
        .default_font(fonts::EDITOR)
        .window_size((960.0, 720.0))
        .run()
}
