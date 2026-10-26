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

#[derive(Debug, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub text: String,
}

fn offset(at: usize) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}

pub fn line_column(source: &str, offset: u32) -> (usize, usize) {
    let offset = (offset as usize).min(source.len());
    let before = source.get(..offset).unwrap_or(source);
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .chars()
        .count()
        + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::line_column;

    #[test]
    fn offsets_become_lines_and_columns_counted_from_one() {
        assert_eq!(line_column("ab\ncd", 0), (1, 1));
        assert_eq!(line_column("ab\ncd", 4), (2, 2));
        assert_eq!(line_column("λx\ny", 2), (1, 2));
        assert_eq!(line_column("ab", 99), (1, 3));
    }
}
