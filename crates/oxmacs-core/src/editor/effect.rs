use std::mem;
use std::path::PathBuf;

use crate::editor::Editor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    WriteClipboard(String),
    ReadClipboard,
    WritePrimary(String),
    ReadPrimary,
    OpenExternally(PathBuf),
}

impl Editor {
    pub fn take_effects(&mut self) -> Vec<Effect> {
        mem::take(&mut self.effects)
    }
}
