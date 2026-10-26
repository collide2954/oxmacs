use std::ops::Range;

use crate::{Link, PageText, Rect};

fn same_line(last: &Rect, next: &Rect) -> bool {
    let middle = f32::midpoint(next.top, next.bottom);
    let height = (next.bottom - next.top).max(1.0);
    (last.top..=last.bottom).contains(&middle)
        && next.left >= last.left - 1.0
        && next.left - last.right < 2.0 * height
}

pub fn rects(page: &PageText, range: Range<usize>) -> Vec<Rect> {
    let mut found: Vec<Rect> = Vec::new();
    for rect in page
        .boxes
        .get(range.start.min(page.boxes.len())..range.end.min(page.boxes.len()))
        .unwrap_or_default()
        .iter()
        .filter(|rect| !rect.is_empty())
    {
        match found.last_mut() {
            Some(last) if same_line(last, rect) => {
                last.left = last.left.min(rect.left);
                last.right = last.right.max(rect.right);
                last.top = last.top.min(rect.top);
                last.bottom = last.bottom.max(rect.bottom);
            }
            _ => found.push(*rect),
        }
    }
    found
}

pub fn char_at(page: &PageText, x: f32, y: f32) -> Option<usize> {
    page.boxes
        .iter()
        .enumerate()
        .filter(|(_, rect)| !rect.is_empty())
        .min_by(|(_, a), (_, b)| a.distance(x, y).total_cmp(&b.distance(x, y)))
        .map(|(at, rect)| {
            if x > f32::midpoint(rect.left, rect.right) {
                at + 1
            } else {
                at
            }
        })
}

pub fn link_at(page: &PageText, x: f32, y: f32) -> Option<&Link> {
    page.links.iter().find(|link| link.rect.contains(x, y))
}

pub fn between(pages: &[PageText], from: (usize, usize), to: (usize, usize)) -> String {
    let mut text = String::new();
    for (index, page) in pages.iter().enumerate().take(to.0 + 1).skip(from.0) {
        let start = if index == from.0 { from.1 } else { 0 };
        let end = if index == to.0 {
            to.1.min(page.text.len())
        } else {
            page.text.len()
        };
        if index > from.0 && !text.ends_with('\n') {
            text.push('\n');
        }
        text.extend(
            page.text
                .get(start.min(end)..end)
                .unwrap_or_default()
                .iter()
                .filter(|character| **character != '\r'),
        );
    }
    text
}

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub page: usize,
    pub rects: Vec<Rect>,
}

fn folded(character: char, exact: bool) -> char {
    if exact {
        character
    } else {
        character.to_lowercase().next().unwrap_or(character)
    }
}

fn squeezed(text: &[char], exact: bool) -> (Vec<char>, Vec<usize>) {
    let mut chars = Vec::with_capacity(text.len());
    let mut origins = Vec::with_capacity(text.len());
    for (at, character) in text.iter().enumerate() {
        if character.is_whitespace() {
            if chars.last() != Some(&' ') {
                chars.push(' ');
                origins.push(at);
            }
        } else {
            chars.push(folded(*character, exact));
            origins.push(at);
        }
    }
    (chars, origins)
}

pub fn search(pages: &[PageText], query: &str) -> Vec<Found> {
    let exact = query.chars().any(char::is_uppercase);
    let wanted: Vec<char> = query.trim().chars().collect();
    let (wanted, _) = squeezed(&wanted, exact);
    if wanted.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let (chars, origins) = squeezed(&page.text, exact);
        let mut at = 0;
        while at + wanted.len() <= chars.len() {
            if chars[at..at + wanted.len()] == wanted[..] {
                let start = origins[at];
                let end = origins[at + wanted.len() - 1] + 1;
                found.push(Found {
                    page: index,
                    rects: rects(page, start..end),
                });
                at += wanted.len();
            } else {
                at += 1;
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Target;

    fn line(text: &str, top: f32) -> PageText {
        let boxes = (0..text.chars().count())
            .map(|index| {
                let left = 72.0 + 6.0 * f32::from(u16::try_from(index).unwrap());
                Rect {
                    left,
                    top,
                    right: left + 5.0,
                    bottom: top + 10.0,
                }
            })
            .collect();
        PageText {
            text: text.chars().collect(),
            boxes,
            links: Vec::new(),
        }
    }

    fn two_lines() -> PageText {
        let mut page = line("Hello world", 100.0);
        let below = line("again here", 120.0);
        page.text.push('\r');
        page.text.push('\n');
        page.boxes.push(Rect::default());
        page.boxes.push(Rect::default());
        page.text.extend(below.text);
        page.boxes.extend(below.boxes);
        page
    }

    #[test]
    fn a_range_becomes_one_rectangle_per_line() {
        let page = two_lines();

        assert_eq!(
            rects(&page, 6..17),
            [
                Rect {
                    left: 108.0,
                    top: 100.0,
                    right: 137.0,
                    bottom: 110.0
                },
                Rect {
                    left: 72.0,
                    top: 120.0,
                    right: 95.0,
                    bottom: 130.0
                },
            ]
        );
        assert_eq!(rects(&page, 40..50), []);
    }

    #[test]
    fn points_find_the_nearest_gap_between_characters() {
        let page = line("Hello", 100.0);

        assert_eq!(char_at(&page, 73.0, 105.0), Some(0));
        assert_eq!(char_at(&page, 76.0, 105.0), Some(1));
        assert_eq!(char_at(&page, 500.0, 105.0), Some(5));
        assert_eq!(char_at(&PageText::default(), 0.0, 0.0), None);
    }

    #[test]
    fn links_are_found_under_the_pointer() {
        let mut page = line("Hello", 100.0);
        page.links.push(Link {
            rect: Rect {
                left: 72.0,
                top: 100.0,
                right: 100.0,
                bottom: 110.0,
            },
            target: Target::Page(3),
        });

        assert_eq!(
            link_at(&page, 80.0, 105.0).map(|link| &link.target),
            Some(&Target::Page(3))
        );
        assert_eq!(link_at(&page, 80.0, 200.0), None);
    }

    #[test]
    fn text_between_two_places_spans_lines_and_pages() {
        let pages = [two_lines(), line("third page", 50.0)];

        assert_eq!(between(&pages, (0, 6), (0, 18)), "world\nagain");
        assert_eq!(between(&pages, (0, 19), (1, 5)), "here\nthird");
        assert_eq!(between(&pages, (1, 6), (1, 99)), "page");
    }

    #[test]
    fn searching_ignores_case_and_line_breaks_unless_asked() {
        let pages = [two_lines(), line("Hello again", 50.0)];

        let found = search(&pages, "world again");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].page, 0);
        assert_eq!(found[0].rects.len(), 2);

        let pages_of = |query: &str| -> Vec<usize> {
            search(&pages, query)
                .into_iter()
                .map(|found| found.page)
                .collect()
        };
        assert_eq!(pages_of("hello"), [0, 1]);
        assert_eq!(pages_of("again"), [0, 1]);
        assert_eq!(pages_of("Again"), Vec::<usize>::new());
        assert_eq!(pages_of("  "), Vec::<usize>::new());
    }
}
