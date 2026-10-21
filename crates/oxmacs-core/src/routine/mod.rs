mod parse;
mod tokens;

use std::ops::Range;

pub use parse::parse;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Value { name: String, columns: Range<usize> },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Template {
    pub pieces: Vec<Piece>,
    pub columns: Range<usize>,
}

impl Template {
    pub fn literal(&self) -> Option<String> {
        let mut text = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Text(part) => text.push_str(part),
                Piece::Value { .. } => return None,
            }
        }
        Some(text)
    }

    pub fn values(&self) -> impl Iterator<Item = (&str, &Range<usize>)> {
        self.pieces.iter().filter_map(|piece| match piece {
            Piece::Value { name, columns } => Some((name.as_str(), columns)),
            Piece::Text(_) => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Text { default: Option<Template> },
    Pick(Vec<Template>),
    YesNo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Run {
        command: String,
        command_columns: Range<usize>,
        input: Option<Template>,
    },
    Insert(Template),
    Keys(Template),
    Message(Template),
    Ask {
        name: String,
        prompt: Template,
        answer: Answer,
    },
    Confirm(Template),
    Search {
        pattern: Template,
        backward: bool,
        regexp: bool,
    },
    Replace {
        pattern: Template,
        with: Template,
        regexp: bool,
        region: bool,
    },
    Shell {
        command: Template,
        insert: bool,
    },
    Filter(Template),
    Repeat {
        times: Option<Template>,
        steps: Vec<Step>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub line: usize,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    BeforeSave(String),
    AfterSave(String),
    Open(String),
    Start,
    Every(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    pub line: usize,
    pub event: Event,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub line: usize,
    pub keys: String,
    pub keys_columns: Range<usize>,
    pub language: Option<(String, Range<usize>)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Routine {
    pub summary: Option<String>,
    pub bindings: Vec<Binding>,
    pub triggers: Vec<Trigger>,
    pub favorite: bool,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub line: usize,
    pub columns: Range<usize>,
    pub message: String,
}
