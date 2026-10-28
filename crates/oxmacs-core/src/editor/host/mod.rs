use std::path::PathBuf;

use crate::command::CommandId;
use crate::editor::{Editor, Effect};
use crate::input::{KeySequence, Lookup};

pub(super) type Segment = fn(&Editor) -> Option<String>;

impl Editor {
    pub fn repeated(&self) -> bool {
        self.repeated
    }

    pub fn open_externally(&mut self, target: impl Into<PathBuf>) {
        self.effects.push(Effect::OpenExternally(target.into()));
    }

    pub(super) fn bound_command(&self, keys: &str) -> Option<CommandId> {
        let keys: KeySequence = keys.parse().ok()?;
        match self.keymaps.get(self.buffer_scope()).lookup(keys.keys()) {
            Lookup::Command(command) => Some(command),
            Lookup::Prefix | Lookup::Unbound => None,
        }
    }

    pub fn add_mode_line_segment(&mut self, segment: Segment) {
        self.mode_line_segments.push(segment);
    }

    pub fn mode_line_segments(&self) -> Vec<String> {
        self.macros
            .recording()
            .then(|| "Recording macro".to_owned())
            .into_iter()
            .chain(
                self.mode_line_segments
                    .iter()
                    .filter_map(|segment| segment(self)),
            )
            .collect()
    }
}

#[cfg(test)]
mod tests;
