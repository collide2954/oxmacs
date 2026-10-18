use std::ops::Range;

use crate::theme::Highlight;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineStyle {
    pub background: Option<Highlight>,
    pub spans: Vec<(Range<usize>, Highlight)>,
}
