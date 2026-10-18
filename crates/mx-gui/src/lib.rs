pub mod app;
pub mod buffer_view;
pub mod keys;

pub use iced::Result;

use app::{App, EDITOR_FONT};

pub fn run() -> Result {
    iced::application(App::default, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .default_font(EDITOR_FONT)
        .window_size((960.0, 720.0))
        .run()
}
