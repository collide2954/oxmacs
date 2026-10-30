use std::ops::Range;

use oxmacs_core::theme::Highlight;

pub(crate) type Spans = Vec<(Range<usize>, Highlight)>;

pub(crate) trait LineLexer {
    type State: Clone;
    type Error;

    fn lex(
        &mut self,
        line: usize,
        text: &str,
        state: &Self::State,
    ) -> Result<(Spans, Self::State), Self::Error>;

    fn same(&self, a: &Self::State, b: &Self::State) -> bool;
}

#[derive(Debug)]
pub(crate) struct Lexed<S> {
    pub(crate) revision: u64,
    lines: Vec<String>,
    spans: Vec<Spans>,
    states: Vec<S>,
}

impl<S> Lexed<S> {
    pub(crate) fn spans(&self, line: usize) -> Spans {
        self.spans.get(line).cloned().unwrap_or_default()
    }

    pub(crate) fn states(&self) -> &[S] {
        &self.states
    }
}

pub(crate) fn relex<L: LineLexer>(
    old: Option<Lexed<L::State>>,
    revision: u64,
    lines: Vec<String>,
    initial: &L::State,
    lexer: &mut L,
) -> (Lexed<L::State>, Option<L::Error>) {
    let (was, mut spans, mut states) = old.map_or_else(
        || (Vec::new(), Vec::new(), Vec::new()),
        |old| (old.lines, old.spans, old.states),
    );
    let whole = states.len() == was.len() && spans.len() == was.len();
    let prefix = was
        .iter()
        .zip(&lines)
        .take(states.len().min(spans.len()))
        .take_while(|(old, new)| old == new)
        .count();
    let suffix = if whole {
        was[prefix..]
            .iter()
            .rev()
            .zip(lines[prefix..].iter().rev())
            .take_while(|(old, new)| old == new)
            .count()
    } else {
        0
    };
    let tail_spans = spans.split_off(prefix);
    let tail_states = states.split_off(prefix);
    let new_tail = lines.len() - suffix;
    let old_tail = was.len() - suffix;
    let mut failed = None;
    for at in prefix..lines.len() {
        let incoming = if at == 0 {
            initial.clone()
        } else {
            states[at - 1].clone()
        };
        if at >= new_tail {
            let old_at = at - new_tail + old_tail;
            let old_incoming = match old_at {
                0 => Some(initial),
                _ if old_at - 1 < prefix => states.get(old_at - 1),
                _ => tail_states.get(old_at - 1 - prefix),
            };
            if old_incoming.is_some_and(|old| lexer.same(&incoming, old)) {
                spans.extend(tail_spans.into_iter().skip(old_at - prefix));
                states.extend(tail_states.into_iter().skip(old_at - prefix));
                break;
            }
        }
        match lexer.lex(at, &lines[at], &incoming) {
            Ok((found, next)) => {
                spans.push(found);
                states.push(next);
            }
            Err(error) => {
                failed = Some(error);
                break;
            }
        }
    }
    spans.resize(lines.len(), Vec::new());
    (
        Lexed {
            revision,
            lines,
            spans,
            states,
        },
        failed,
    )
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use oxmacs_core::theme::Highlight;

    use super::{Lexed, LineLexer, Spans, relex};

    #[derive(Default)]
    struct Quotes {
        lexed: Vec<usize>,
    }

    impl LineLexer for Quotes {
        type State = bool;
        type Error = Infallible;

        fn lex(
            &mut self,
            line: usize,
            text: &str,
            inside: &bool,
        ) -> Result<(Spans, bool), Infallible> {
            self.lexed.push(line);
            let flips = text.matches('"').count() % 2 == 1;
            let spans = if *inside {
                vec![(0..text.chars().count(), Highlight::String)]
            } else {
                Vec::new()
            };
            Ok((spans, *inside != flips))
        }

        fn same(&self, a: &bool, b: &bool) -> bool {
            a == b
        }
    }

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_owned).collect()
    }

    fn lexed(old: Option<Lexed<bool>>, text: &str) -> (Lexed<bool>, Vec<usize>) {
        let mut quotes = Quotes::default();
        let (relexed, _) = relex(old, 0, lines(text), &false, &mut quotes);
        (relexed, quotes.lexed)
    }

    fn strings(lexed: &Lexed<bool>, count: usize) -> Vec<bool> {
        (0..count)
            .map(|line| !lexed.spans(line).is_empty())
            .collect()
    }

    #[test]
    fn an_edit_relexes_only_until_the_state_settles() {
        let text = "a\nb\nc\nd\ne\nf";
        let (first, all) = lexed(None, text);
        assert_eq!(all, [0, 1, 2, 3, 4, 5]);

        let (second, again) = lexed(Some(first), "a\nb\nC\nd\ne\nf");
        assert_eq!(again, [2]);

        let (third, quoted) = lexed(Some(second), "a\nb\n\"C\nd\ne\nf");
        assert_eq!(quoted, [2, 3, 4, 5]);
        assert_eq!(strings(&third, 6), [false, false, false, true, true, true]);

        let (fourth, closed) = lexed(Some(third), "a\nb\n\"C\nd\"\ne\nf");
        assert_eq!(closed, [3, 4, 5]);
        assert_eq!(
            strings(&fourth, 6),
            [false, false, false, true, false, false]
        );
    }

    #[test]
    fn inserted_and_removed_lines_reuse_the_lines_after_them() {
        let (first, _) = lexed(None, "a\n\"b\nc\nd\"\ne");
        let (second, inserted) = lexed(Some(first), "a\n\"b\nnew\nc\nd\"\ne");
        assert_eq!(inserted, [2]);
        assert_eq!(strings(&second, 6), [false, false, true, true, true, false]);

        let (third, removed) = lexed(Some(second), "a\n\"b\nd\"\ne");
        assert_eq!(removed, Vec::<usize>::new());
        assert_eq!(strings(&third, 4), [false, false, true, false]);
    }
}
