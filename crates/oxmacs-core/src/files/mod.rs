pub mod editorconfig;
mod entry;
mod format;
mod io;
pub mod paths;
pub mod roots;

pub use entry::{Entry, Kind, read_entries, sort_by_name};
pub use format::{DecodeError, Decoded, Format, LineEnding, decode, encode};
pub use io::{DiskState, FileError, Loaded, identity, load, same_file, save};
