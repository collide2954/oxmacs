pub mod clock;
pub mod command;
pub mod config;
pub mod display;
pub mod editor;
pub mod external;
pub mod files;
pub mod input;
pub mod plugin;
pub mod search;
pub mod syntax;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod text;
pub mod theme;
