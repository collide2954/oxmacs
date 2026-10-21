#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start: offset(start),
            end: offset(end),
        }
    }

    #[must_use]
    pub fn through(self, last: Self) -> Self {
        Self {
            start: self.start,
            end: last.end,
        }
    }
}

fn offset(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}
