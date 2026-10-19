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
use oxmacs_core::cli::Visit;

pub fn run(visits: Vec<Visit>) -> Result {
    iced::application(move || App::boot(&visits), App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .font(fonts::JETBRAINS_MONO_REGULAR)
        .default_font(fonts::EDITOR)
        .window_size((960.0, 720.0))
        .run()
}
