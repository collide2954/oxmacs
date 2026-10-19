use std::ops::Range;

use crate::editor::Editor;
use crate::text::Buffer;
use crate::theme::Highlight;

const SCAN_LIMIT: usize = 100_000;

impl Editor {
    pub fn prepare_display(&mut self) {
        self.sync_syntax();
        for index in self.visible_buffers() {
            self.buffers.sync_syntax(index);
        }
        self.paren_pair = self.find_paren_pair();
        self.follow_source();
    }

    fn pairs(&self) -> &'static [(char, char)] {
        match self.language() {
            Some(language) if self.minibuffer.is_none() => language.pairs(),
            _ => &[],
        }
    }

    pub(super) fn insert_pair(&mut self, typed: char) -> bool {
        let pairs = self.pairs();
        if pairs.is_empty() {
            return false;
        }
        let closer = pairs
            .iter()
            .find(|(open, _)| *open == typed)
            .map(|(_, close)| *close);
        let buffer = self.buffers.current_mut();
        if let (Some(closer), Some(selection)) = (closer, buffer.selection()) {
            self.shift_selection = false;
            buffer.deactivate_mark();
            buffer.insert_at(selection.end, &closer.to_string());
            buffer.insert_at(selection.start, &typed.to_string());
            buffer.set_point(selection.end + 2);
            return true;
        }
        let point = buffer.point();
        let next = buffer.chars_after(point).next();
        let steps_over = pairs.iter().any(|(_, close)| *close == typed) && next == Some(typed);
        if steps_over {
            buffer.set_point(point + 1);
            return true;
        }
        let open_space =
            next.is_none_or(|c| c.is_whitespace() || matches!(c, ')' | ']' | '}' | ',' | ';'));
        let quote_allowed = closer != Some(typed)
            || buffer
                .chars_before(point)
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '\\');
        match closer {
            Some(closer) if open_space && quote_allowed => {
                buffer.insert(&format!("{typed}{closer}"));
                buffer.set_point(point + 1);
                true
            }
            _ => false,
        }
    }

    pub(super) fn delete_pair(&mut self) -> bool {
        let pairs = self.pairs();
        if pairs.is_empty() {
            return false;
        }
        let buffer = self.buffers.current_mut();
        let point = buffer.point();
        let around = (
            buffer.chars_before(point).next(),
            buffer.chars_after(point).next(),
        );
        let empty = pairs
            .iter()
            .any(|(open, close)| around == (Some(*open), Some(*close)));
        if empty {
            buffer.delete_range(point - 1..point + 1);
        }
        empty
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

pub(super) fn scan_partner(buffer: &Buffer, position: usize) -> Option<usize> {
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
    use crate::editor::test_support::{editor_with, press, type_text};
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

    fn rust(text: &str, point: usize) -> Editor {
        let mut editor = editor_with(text, point);
        editor.set_major_mode(Some(Language::Rust));
        editor
    }

    #[test]
    fn openers_insert_their_closers() {
        for (typed, expected) in [("(", "()"), ("[", "[]"), ("{", "{}"), ("\"", "\"\"")] {
            let mut editor = rust("", 0);

            type_text(&mut editor, typed);

            assert_eq!(editor.buffer().contents(), expected, "{typed}");
            assert_eq!(editor.buffer().point(), 1, "{typed}");
        }
    }

    #[test]
    fn closers_step_over_the_inserted_ones() {
        let mut editor = rust("", 0);

        type_text(&mut editor, "f(\"a\")");

        assert_eq!(editor.buffer().contents(), "f(\"a\")");
        assert_eq!(editor.buffer().point(), 6);
    }

    #[test]
    fn openers_before_words_stay_single() {
        let mut editor = rust("x", 0);

        type_text(&mut editor, "(");

        assert_eq!(editor.buffer().contents(), "(x");
    }

    #[test]
    fn quotes_after_words_stay_single() {
        let mut editor = rust("r", 1);

        type_text(&mut editor, "\"");

        assert_eq!(editor.buffer().contents(), "r\"");
    }

    #[test]
    fn delete_removes_an_empty_pair() {
        let mut editor = rust("", 0);

        press(&mut editor, "( DEL");

        assert_eq!(editor.buffer().contents(), "");
    }

    #[test]
    fn return_between_braces_opens_a_block() {
        let mut editor = rust("fn a() ", 7);

        press(&mut editor, "{ RET");

        assert_eq!(editor.buffer().contents(), "fn a() {\n    \n}");
        assert_eq!(editor.buffer().point(), 13);
    }

    #[test]
    fn openers_wrap_the_selection() {
        let mut editor = rust("let x = a + b;", 8);

        press(&mut editor, "C-SPC C-e C-b (");

        assert_eq!(editor.buffer().contents(), "let x = (a + b);");
        assert_eq!(editor.buffer().point(), 15);
        assert_eq!(editor.buffer().selection(), None);
    }

    fn r(text: &str, point: usize) -> Editor {
        let mut editor = editor_with(text, point);
        editor.set_major_mode(Some(Language::R));
        editor
    }

    #[test]
    fn r_pairs_single_quotes_and_backticks_too() {
        let mut editor = r("", 0);

        type_text(&mut editor, "x <- 'a' + `b`");

        assert_eq!(editor.buffer().contents(), "x <- 'a' + `b`");
        assert_eq!(editor.buffer().point(), 14);
    }

    #[test]
    fn r_quotes_after_words_stay_single() {
        let mut editor = r("# don", 5);

        type_text(&mut editor, "'");

        assert_eq!(editor.buffer().contents(), "# don'");
    }

    #[test]
    fn rust_leaves_single_quotes_alone() {
        let mut editor = rust("fn f<", 5);

        type_text(&mut editor, "'");

        assert_eq!(editor.buffer().contents(), "fn f<'");
    }

    #[test]
    fn return_between_r_braces_opens_a_two_space_block() {
        let mut editor = r("f <- function() ", 16);

        press(&mut editor, "{ RET");

        assert_eq!(editor.buffer().contents(), "f <- function() {\n  \n}");
    }

    #[test]
    fn plain_text_does_not_pair() {
        let mut editor = editor_with("", 0);

        type_text(&mut editor, "(\"");

        assert_eq!(editor.buffer().contents(), "(\"");
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
