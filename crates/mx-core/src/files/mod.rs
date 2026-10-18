mod format;
mod io;

pub use format::{DecodeError, Decoded, Format, LineEnding, decode, encode};
pub use io::{DiskState, FileError, Loaded, load, save};
