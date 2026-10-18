use std::borrow::Cow;
use std::ops::Range;

use regex::{Regex, RegexBuilder};

use crate::text::Buffer;

const WINDOW: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("Invalid regexp")]
pub struct InvalidPattern;

#[derive(Debug, Clone)]
pub struct Pattern {
    regex: Regex,
    literal: bool,
}

#[derive(Debug)]
pub struct Haystack<'a> {
    buffer: &'a Buffer,
    text: String,
}

impl<'a> Haystack<'a> {
    pub fn new(buffer: &'a Buffer) -> Self {
        Self {
            buffer,
            text: buffer.contents(),
        }
    }

    fn byte(&self, position: usize) -> usize {
        self.buffer.char_to_byte(position)
    }

    fn chars(&self, bytes: Range<usize>) -> Range<usize> {
        self.buffer.byte_to_char(bytes.start)..self.buffer.byte_to_char(bytes.end)
    }

    fn after(&self, byte: usize) -> usize {
        byte + self.text[byte..].chars().next().map_or(1, char::len_utf8)
    }
}

struct Counter<'t> {
    text: &'t str,
    byte: usize,
    position: usize,
}

impl Counter<'_> {
    fn to(&mut self, byte: usize) -> usize {
        self.position += self.text[self.byte..byte].chars().count();
        self.byte = byte;
        self.position
    }
}

impl Pattern {
    pub fn new(query: &str, regex: bool) -> Result<Self, InvalidPattern> {
        let source = if regex {
            Cow::Borrowed(query)
        } else {
            Cow::Owned(regex::escape(query))
        };
        RegexBuilder::new(&source)
            .case_insensitive(!has_uppercase(query, regex))
            .multi_line(true)
            .build()
            .map(|compiled| Self {
                regex: compiled,
                literal: !regex,
            })
            .map_err(|_| InvalidPattern)
    }

    pub fn find_forward(&self, haystack: &Haystack<'_>, from: usize) -> Option<Range<usize>> {
        let found = self.regex.find_at(&haystack.text, haystack.byte(from))?;
        Some(haystack.chars(found.range()))
    }

    pub fn find_backward(&self, haystack: &Haystack<'_>, before: usize) -> Option<Range<usize>> {
        let limit = haystack.byte(before);
        let mut window = WINDOW;
        loop {
            let start = haystack.byte(before.saturating_sub(window));
            let mut last = None;
            let mut at = start;
            while at <= limit
                && let Some(found) = self.regex.find_at(&haystack.text, at)
                && found.start() < limit
            {
                last = Some(found.range());
                at = haystack.after(found.start());
            }
            if last.is_some() || start == 0 {
                return last.map(|bytes| haystack.chars(bytes));
            }
            window = window.saturating_mul(4);
        }
    }

    pub fn find_all(&self, haystack: &Haystack<'_>, within: Range<usize>) -> Vec<Range<usize>> {
        let end = haystack.byte(within.end);
        let mut at = haystack.byte(within.start);
        let mut counter = Counter {
            text: &haystack.text,
            byte: at,
            position: haystack.buffer.byte_to_char(at),
        };
        let mut found = Vec::new();
        while at <= end
            && let Some(next) = self.regex.find_at(&haystack.text, at)
            && next.end() <= end
        {
            found.push(counter.to(next.start())..counter.to(next.end()));
            at = if next.is_empty() {
                haystack.after(next.end())
            } else {
                next.end()
            };
        }
        found
    }

    pub fn expand(&self, haystack: &Haystack<'_>, found: Range<usize>, template: &str) -> String {
        if self.literal {
            return template.to_owned();
        }
        let Some(captures) = self
            .regex
            .captures_at(&haystack.text, haystack.byte(found.start))
        else {
            return template.to_owned();
        };
        let group = |index: usize| captures.get(index).map_or("", |group| group.as_str());
        let mut expanded = String::new();
        let mut chars = template.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                expanded.push(c);
                continue;
            }
            match chars.next() {
                Some('&') => expanded.push_str(group(0)),
                Some(digit @ '0'..='9') => {
                    let index = digit
                        .to_digit(10)
                        .and_then(|index| usize::try_from(index).ok())
                        .unwrap_or_default();
                    expanded.push_str(group(index));
                }
                Some(other) => expanded.push(other),
                None => expanded.push('\\'),
            }
        }
        expanded
    }
}

fn has_uppercase(query: &str, regex: bool) -> bool {
    let mut chars = query.chars();
    while let Some(c) = chars.next() {
        if regex && c == '\\' {
            chars.next();
        } else if c.is_uppercase() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str) -> Buffer {
        Buffer::with_text("t", text)
    }

    fn forward(query: &str, regex: bool, text: &str, from: usize) -> Option<Range<usize>> {
        let buffer = buffer(text);
        Pattern::new(query, regex)
            .unwrap()
            .find_forward(&Haystack::new(&buffer), from)
    }

    fn backward(query: &str, text: &str, before: usize) -> Option<Range<usize>> {
        let buffer = buffer(text);
        Pattern::new(query, false)
            .unwrap()
            .find_backward(&Haystack::new(&buffer), before)
    }

    fn all(query: &str, regex: bool, text: &str, within: Range<usize>) -> Vec<(usize, usize)> {
        let buffer = buffer(text);
        Pattern::new(query, regex)
            .unwrap()
            .find_all(&Haystack::new(&buffer), within)
            .into_iter()
            .map(|found| (found.start, found.end))
            .collect()
    }

    #[test]
    fn literal_queries_match_their_text_exactly() {
        assert_eq!(forward("a.c", false, "abc a.c", 0), Some(4..7));
    }

    #[test]
    fn lowercase_queries_ignore_case() {
        assert_eq!(forward("foo", false, "x FOO", 0), Some(2..5));
    }

    #[test]
    fn an_uppercase_letter_makes_the_search_case_sensitive() {
        assert_eq!(forward("Foo", false, "foo FOO Foo", 0), Some(8..11));
    }

    #[test]
    fn escaped_letters_in_a_regexp_do_not_count_as_uppercase() {
        assert_eq!(forward(r"\Sfoo", true, " xFOO", 0), Some(1..5));
    }

    #[test]
    fn regexps_use_rust_syntax_with_line_anchors() {
        assert_eq!(forward("a.c", true, "xabc", 0), Some(1..4));
        assert_eq!(forward("^b", true, "ab\nb", 0), Some(3..4));
    }

    #[test]
    fn invalid_regexps_are_refused() {
        assert_eq!(Pattern::new("(", true).err(), Some(InvalidPattern));
        assert!(Pattern::new("(", false).is_ok());
    }

    #[test]
    fn forward_search_starts_at_a_char_position() {
        assert_eq!(forward("foo", false, "foo bar foo", 1), Some(8..11));
        assert_eq!(forward("foo", false, "foo bar foo", 8), Some(8..11));
        assert_eq!(forward("foo", false, "foo bar foo", 9), None);
    }

    #[test]
    fn positions_count_chars_not_bytes() {
        assert_eq!(forward("foo", false, "héllo→foo", 0), Some(6..9));
        assert_eq!(backward("é", "héé", 3), Some(2..3));
    }

    #[test]
    fn backward_search_finds_the_last_match_starting_before_a_position() {
        assert_eq!(backward("foo", "foo bar foo", 8), Some(0..3));
        assert_eq!(backward("foo", "foo bar foo", 9), Some(8..11));
        assert_eq!(backward("foo", "foo bar foo", 0), None);
    }

    #[test]
    fn backward_search_sees_overlapping_matches() {
        assert_eq!(backward("aa", "aaaa", 4), Some(2..4));
    }

    #[test]
    fn backward_search_reaches_far_back() {
        let text = format!("foo{}", "x".repeat(50_000));

        assert_eq!(backward("foo", &text, text.len()), Some(0..3));
    }

    #[test]
    fn all_matches_lie_within_the_range() {
        assert_eq!(all("foo", false, "foo bar foo", 0..11), [(0, 3), (8, 11)]);
        assert_eq!(all("foo", false, "foo bar foo", 1..11), [(8, 11)]);
        assert_eq!(all("foo", false, "foo bar foo", 0..10), [(0, 3)]);
        assert_eq!(all("é", false, "héé", 0..3), [(1, 2), (2, 3)]);
    }

    #[test]
    fn empty_matches_are_found_once_each() {
        assert_eq!(all("^", true, "ab\ncd", 0..5), [(0, 0), (3, 3)]);
        assert_eq!(all("x*", true, "ab", 0..2), [(0, 0), (1, 1), (2, 2)]);
    }

    #[test]
    fn regexp_replacements_expand_groups() {
        let buffer = buffer("mail me@host now");
        let pattern = Pattern::new(r"(\w+)@(\w+)", true).unwrap();
        let haystack = Haystack::new(&buffer);

        let found = pattern.find_forward(&haystack, 0).unwrap();

        assert_eq!(
            pattern.expand(&haystack, found, r"\2 at \1 (\&) \\"),
            r"host at me (me@host) \"
        );
    }

    #[test]
    fn literal_replacements_are_used_as_they_are() {
        let buffer = buffer("a.b");
        let pattern = Pattern::new(".", false).unwrap();
        let haystack = Haystack::new(&buffer);

        assert_eq!(pattern.expand(&haystack, 1..2, r"\1\&"), r"\1\&");
    }
}
