use std::ops::Range;

use crate::editor::Editor;

fn closeness(range: &Range<usize>, point: usize) -> (bool, usize) {
    if range.end <= point {
        (false, point - range.end)
    } else if range.start < point {
        (false, 0)
    } else {
        (true, range.start - point)
    }
}

fn located(text: &str, code: &str, point: usize) -> Vec<Range<usize>> {
    let wanted = code.trim();
    if wanted.is_empty() {
        return Vec::new();
    }
    let length = wanted.chars().count();
    let mut counted = (0, 0);
    let exact = text.match_indices(wanted).map(|(at, _)| {
        counted = (at, counted.1 + text[counted.0..at].chars().count());
        counted.1..counted.1 + length
    });
    match exact.min_by_key(|range| closeness(range, point)) {
        Some(found) => vec![found],
        None => by_lines(text, code, point),
    }
}

fn blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn trimmed((start, line): (usize, &str)) -> Range<usize> {
    let lead = line.chars().take_while(|c| c.is_whitespace()).count();
    start + lead..start + line.trim_end().chars().count()
}

fn by_lines(text: &str, code: &str, point: usize) -> Vec<Range<usize>> {
    let wanted: Vec<&str> = code
        .lines()
        .map(str::trim_end)
        .filter(|line| !blank(line))
        .collect();
    let Some((first, rest)) = wanted.split_first().filter(|(_, rest)| !rest.is_empty()) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    let mut start = 0;
    for line in text.split('\n') {
        lines.push((start, line));
        start += line.chars().count() + 1;
    }
    let same = |at: usize, line: &str| lines[at].1.trim_end() == line;
    let follow = |from: usize| {
        let mut found = vec![from];
        let mut at = from + 1;
        for line in rest {
            at += lines[at..]
                .iter()
                .position(|(_, text)| text.trim_end() == *line)?;
            found.push(at);
            at += 1;
        }
        Some(found)
    };
    let span = |found: &Vec<usize>| {
        let (Some(first), Some(last)) = (found.first(), found.last()) else {
            return 0..0;
        };
        trimmed(lines[*first]).start..trimmed(lines[*last]).end
    };
    (0..lines.len())
        .filter(|at| same(*at, first))
        .filter_map(follow)
        .min_by_key(|found| closeness(&span(found), point))
        .map(|found| runs(&lines, &found))
        .unwrap_or_default()
}

fn runs(lines: &[(usize, &str)], found: &[usize]) -> Vec<Range<usize>> {
    let mut runs: Vec<Range<usize>> = Vec::new();
    let mut previous: Option<usize> = None;
    for &at in found {
        let range = trimmed(lines[at]);
        let joined = previous
            .is_some_and(|before| lines[before + 1..at].iter().all(|(_, line)| blank(line)));
        match runs.last_mut() {
            Some(run) if joined => run.end = range.end,
            _ => runs.push(range),
        }
        previous = Some(at);
    }
    runs
}

impl Editor {
    pub fn sent_ranges(&self, buffer: crate::text::BufferId, code: &str) -> Vec<Range<usize>> {
        self.buffer_by_id(buffer).map_or_else(Vec::new, |buffer| {
            located(&buffer.contents(), code, buffer.point())
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::editor_with;

    fn sent(text: &str, point: usize, code: &str) -> Vec<(usize, usize)> {
        let editor = editor_with(text, point);
        editor
            .sent_ranges(editor.buffer().id(), code)
            .into_iter()
            .map(|range| (range.start, range.end))
            .collect()
    }

    #[test]
    fn sent_code_is_found_where_it_sits() {
        assert_eq!(sent("x <- 1\ny <- 2\n", 7, "x <- 1"), [(0, 6)]);
        assert_eq!(sent("x <- 1\ny <- 2\n", 13, "y <- 2\n"), [(7, 13)]);
        assert_eq!(sent("é <- 1\nf(é)\n", 0, "  f(é)\n\n"), [(7, 11)]);
        assert_eq!(sent("x <- 1\n", 0, "x <- 2"), []);
        assert_eq!(sent("x <- 1\n", 0, " \n"), []);
    }

    #[test]
    fn of_several_copies_the_one_just_before_or_around_point_was_sent() {
        let text = "x <- 1\nx <- 1\nx <- 1\n";

        assert_eq!(sent(text, 7, "x <- 1"), [(0, 6)]);
        assert_eq!(sent(text, 13, "x <- 1"), [(7, 13)]);
        assert_eq!(sent(text, 9, "x <- 1"), [(7, 13)]);
        assert_eq!(sent("a\nx <- 1\nb\nx <- 1\n", 0, "x <- 1"), [(2, 8)]);
    }

    #[test]
    fn code_gathered_from_several_places_lights_each_of_them() {
        let text =
            "```{r}\na <- 1\n\nb <- 2\n```\n\nSome text\n\n```{r}\n#| echo: false\nc <- 3\n```\n";

        assert_eq!(
            sent(text, 0, "a <- 1\n\nb <- 2\nc <- 3\n"),
            [(7, 21), (60, 66)]
        );
        assert_eq!(sent(text, 0, "a <- 1\nd <- 4\n"), []);
    }
}
