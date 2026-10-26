use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::path::Path;

use crate::command::ScopeId;
use crate::display::LineStyle;
use crate::editor::InputLine;
use crate::theme::Highlight;

pub use crate::editor::Offer;

pub trait PluginBuffer: Any + Send + fmt::Debug {
    fn scope(&self) -> ScopeId;

    fn line_style(&self, _line: usize) -> LineStyle {
        LineStyle::default()
    }

    fn input_line(&mut self) -> Option<&mut InputLine> {
        None
    }

    fn directory(&self) -> Option<&Path> {
        None
    }

    fn file(&self) -> Option<&Path> {
        None
    }

    fn fixed_name(&self) -> bool {
        false
    }

    fn watched(&self) -> Option<&Path> {
        None
    }

    fn position(&self) -> Option<String> {
        None
    }

    fn pills(&self, _line: usize) -> Vec<(Range<usize>, Highlight)> {
        Vec::new()
    }

    fn hears_clicks(&self) -> bool {
        false
    }

    fn hears_typing(&self) -> bool {
        false
    }

    fn line_cursor(&self) -> bool {
        false
    }

    fn unsaved(&self) -> bool {
        false
    }
}

#[derive(Default)]
pub struct States(HashMap<TypeId, Box<dyn Any + Send>>);

impl fmt::Debug for States {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("States")
            .field("count", &self.0.len())
            .finish()
    }
}

impl States {
    pub fn get<T: Any + Send>(&self) -> Option<&T> {
        self.0.get(&TypeId::of::<T>())?.downcast_ref()
    }

    pub fn get_mut<T: Any + Send>(&mut self) -> Option<&mut T> {
        self.0.get_mut(&TypeId::of::<T>())?.downcast_mut()
    }

    pub fn set<T: Any + Send>(&mut self, state: T) {
        self.0.insert(TypeId::of::<T>(), Box::new(state));
    }

    pub fn take<T: Any + Send>(&mut self) -> Option<T> {
        self.0
            .remove(&TypeId::of::<T>())?
            .downcast()
            .ok()
            .map(|state| *state)
    }
}
