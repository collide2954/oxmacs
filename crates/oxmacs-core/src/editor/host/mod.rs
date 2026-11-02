use std::path::PathBuf;

use crate::command::CommandId;
use crate::editor::{Editor, Effect, Event};
use crate::input::{KeySequence, Lookup};

pub(super) type Segment = fn(&Editor) -> Vec<ModeLinePart>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeLinePart {
    pub text: String,
    pub click: Option<String>,
}

impl ModeLinePart {
    pub fn clickable(text: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            click: Some(key.into()),
        }
    }
}

impl From<String> for ModeLinePart {
    fn from(text: String) -> Self {
        Self { text, click: None }
    }
}

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

    pub fn mode_line_parts(&self) -> Vec<ModeLinePart> {
        self.macros
            .recording()
            .then(|| "Recording macro".to_owned().into())
            .into_iter()
            .chain(
                self.mode_line_segments
                    .iter()
                    .flat_map(|segment| segment(self)),
            )
            .collect()
    }

    pub fn mode_line_segments(&self) -> Vec<String> {
        self.mode_line_parts()
            .into_iter()
            .map(|part| part.text)
            .collect()
    }

    pub fn click_mode_line(&mut self, key: &str) {
        self.record(Event::ModeLineClicked(key.to_owned()));
    }
}

#[cfg(test)]
mod tests;
