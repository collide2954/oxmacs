use std::ops::Range;

use crate::editor::Editor;
use crate::text::Buffer;
use crate::theme::Highlight;

const SCAN_LIMIT: usize = 100_000;

impl Editor {
    pub fn prepare_display(&mut self) {
        self.sync_syntax();
        self.paren_pair = self.find_paren_pair();
    }

    fn find_paren_pair(&self) -> Option<(usize, usize)> {
        if self.minibuffer.is_some() || self.buffers.is_special(0) {
            return None;
        }
        let buffer = self.buffers.current();
        let point = buffer.point();
        if buffer
            .chars_after(point)
            .next()
            .is_some_and(|c| matches!(c, '(' | '[' | '{'))
            && let Some(partner) = self.bracket_partner(point)
        {
            return Some((point, partner));
        }
        let before = point.checked_sub(1)?;
        if buffer
            .chars_after(before)
            .next()
            .is_some_and(|c| matches!(c, ')' | ']' | '}'))
        {
            return self
                .bracket_partner(before)
                .map(|partner| (partner, before));
        }
        None
    }

    fn bracket_partner(&self, position: usize) -> Option<usize> {
        let buffer = self.buffers.current();
        match self.buffers.syntax(0) {
            Some(syntax) => syntax.bracket_partner(buffer, position),
            None => scan_partner(buffer, position),
        }
    }

    pub(super) fn paren_marks(&self, line: usize) -> Vec<(Range<usize>, Highlight)> {
        let Some((open, close)) = self.paren_pair else {
            return Vec::new();
        };
        let buffer = self.buffers.current();
        [open, close]
            .into_iter()
            .filter(|position| buffer.line_of(*position) == line)
            .map(|position| {
                let column = buffer.column(position);
                (column..column + 1, Highlight::ParenMatch)
            })
            .collect()
    }
}

fn scan_partner(buffer: &Buffer, position: usize) -> Option<usize> {
    let bracket = buffer.chars_after(position).next()?;
    let (partner, forward) = match bracket {
        '(' => (')', true),
        '[' => (']', true),
        '{' => ('}', true),
        ')' => ('(', false),
        ']' => ('[', false),
        '}' => ('{', false),
        _ => return None,
    };
    let mut depth = 0_usize;
    let mut scan = |offset: usize, c: char| {
        if c == bracket {
            depth += 1;
        } else if c == partner {
            if depth == 0 {
                return Some(offset);
            }
            depth -= 1;
        }
        None
    };
    if forward {
        buffer
            .chars_after(position + 1)
            .take(SCAN_LIMIT)
            .enumerate()
            .find_map(|(offset, c)| scan(offset, c))
            .map(|offset| position + 1 + offset)
    } else {
        buffer
            .chars_before(position)
            .take(SCAN_LIMIT)
            .enumerate()
            .find_map(|(offset, c)| scan(offset, c))
            .map(|offset| position - 1 - offset)
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::editor_with;
    use crate::syntax::Language;
    use crate::theme::Highlight;

    fn shown(text: &str, point: usize, language: Option<Language>) -> Editor {
        let mut editor = editor_with(text, point);
        editor.set_major_mode(language);
        editor.prepare_display();
        editor
    }

    fn paren(range: std::ops::Range<usize>) -> (std::ops::Range<usize>, Highlight) {
        (range, Highlight::ParenMatch)
    }

    #[test]
    fn brackets_around_point_are_marked() {
        let rust = Some(Language::Rust);
        for point in [7, 9] {
            let editor = shown("fn a() {}", point, rust);

            assert_eq!(editor.line_marks(0), [paren(7..8), paren(8..9)], "{point}");
        }
    }

    #[test]
    fn matches_can_span_lines() {
        let editor = shown("fn a() {\n}", 7, Some(Language::Rust));

        assert_eq!(editor.line_marks(0), [paren(7..8)]);
        assert_eq!(editor.line_marks(1), [paren(0..1)]);
    }

    #[test]
    fn brackets_in_strings_do_not_count() {
        let editor = shown("let s = \"(\";", 9, Some(Language::Rust));

        assert_eq!(editor.line_marks(0), []);
    }

    #[test]
    fn plain_text_matches_by_counting() {
        let editor = shown("(a [b])", 0, None);

        assert_eq!(editor.line_marks(0), [paren(0..1), paren(6..7)]);
    }

    #[test]
    fn unmatched_brackets_are_not_marked() {
        let editor = shown("(a", 0, None);

        assert_eq!(editor.line_marks(0), []);
    }
}
