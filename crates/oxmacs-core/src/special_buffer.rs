use std::any::Any;
use std::fmt;
use std::ops::Range;
use std::path::Path;

use crate::command::ScopeId;
use crate::display::LineStyle;
use crate::editor::InputLine;
use crate::theme::Highlight;

pub trait SpecialBuffer: Any + Send + fmt::Debug {
    fn scope(&self) -> ScopeId;

    fn line_style(&self, _line: usize, _text: &str) -> LineStyle {
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

    fn links(&self, _line: usize) -> Vec<Range<usize>> {
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

pub(crate) fn downcast<T: SpecialBuffer>(view: &dyn SpecialBuffer) -> Option<&T> {
    let any: &dyn Any = view;
    any.downcast_ref()
}

pub(crate) fn downcast_mut<T: SpecialBuffer>(view: &mut dyn SpecialBuffer) -> Option<&mut T> {
    let any: &mut dyn Any = view;
    any.downcast_mut()
}
