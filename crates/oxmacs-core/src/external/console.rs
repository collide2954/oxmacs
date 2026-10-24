use regex::Regex;

const START: char = '\u{1}';
const SEPARATOR: char = '\u{2}';
const END: char = '\u{3}';

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Prompt(String),
    Marker { kind: String, payload: String },
}

#[derive(Debug, Default)]
pub struct Stream {
    pending: String,
    line: String,
    prompt: String,
    prompts: Vec<Regex>,
    watches: Vec<(String, Regex)>,
}

impl Stream {
    pub fn with_patterns(prompts: Vec<Regex>, watches: Vec<(String, Regex)>) -> Self {
        Self {
            prompts,
            watches,
            ..Self::default()
        }
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn feed(&mut self, chunk: &str) -> Vec<Piece> {
        let pieces = self.split(chunk);
        if self.prompts.is_empty() && self.watches.is_empty() {
            return pieces;
        }
        let mut found = Vec::new();
        for piece in pieces {
            match piece {
                Piece::Text(text) => self.lines(&text, &mut found),
                other => {
                    self.line.clear();
                    found.push(other);
                }
            }
        }
        found
    }

    fn lines(&mut self, text: &str, found: &mut Vec<Piece>) {
        let mut rest = text;
        while let Some(at) = rest.find('\n') {
            let (line, after) = rest.split_at(at + 1);
            push_text(found, line);
            self.line.push_str(&line[..at]);
            let whole = std::mem::take(&mut self.line);
            let whole = whole.trim_end_matches('\r');
            for (kind, pattern) in &self.watches {
                if pattern.is_match(whole) {
                    found.push(Piece::Marker {
                        kind: kind.clone(),
                        payload: whole.to_owned(),
                    });
                }
            }
            rest = after;
        }
        if rest.is_empty() {
            return;
        }
        let candidate = format!("{}{rest}", self.line);
        if self
            .prompts
            .iter()
            .any(|pattern| pattern.is_match(&candidate))
        {
            self.line.clear();
            self.prompt = candidate;
            found.push(Piece::Prompt(rest.to_owned()));
        } else {
            self.line.push_str(rest);
            push_text(found, rest);
        }
    }

    fn split(&mut self, chunk: &str) -> Vec<Piece> {
        self.pending.push_str(chunk);
        let text = std::mem::take(&mut self.pending);
        let mut pieces = Vec::new();
        let mut rest = text.as_str();
        while let Some(at) = rest.find(START) {
            push_text(&mut pieces, &rest[..at]);
            let body = &rest[at + START.len_utf8()..];
            let Some(end) = body.find(END) else {
                rest[at..].clone_into(&mut self.pending);
                return pieces;
            };
            let (kind, payload) = body[..end]
                .split_once(SEPARATOR)
                .unwrap_or((&body[..end], ""));
            let piece = match kind {
                ">" | "+" if payload.is_empty() => Piece::Prompt(format!("{kind} ")),
                ">" | "+" => Piece::Prompt(payload.to_owned()),
                _ => Piece::Marker {
                    kind: kind.to_owned(),
                    payload: payload.to_owned(),
                },
            };
            if let Piece::Prompt(shown) = &piece {
                shown.clone_into(&mut self.prompt);
            }
            pieces.push(piece);
            rest = &body[end + END.len_utf8()..];
        }
        push_text(&mut pieces, rest);
        pieces
    }
}

fn push_text(pieces: &mut Vec<Piece>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Piece::Text(last)) = pieces.last_mut() {
        last.push_str(text);
    } else {
        pieces.push(Piece::Text(text.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(text: &str) -> Piece {
        Piece::Text(text.to_owned())
    }

    fn prompt(shown: &str) -> Piece {
        Piece::Prompt(shown.to_owned())
    }

    #[test]
    fn plain_output_is_text() {
        assert_eq!(Stream::default().feed("[1] 2\n"), [text("[1] 2\n")]);
    }

    #[test]
    fn prompts_are_recognized_anywhere() {
        let mut stream = Stream::default();

        assert_eq!(
            stream.feed("done\u{1}>\u{2}\u{3}x\u{1}+\u{2}\u{3}"),
            [text("done"), prompt("> "), text("x"), prompt("+ ")]
        );
    }

    #[test]
    fn prompts_may_carry_the_text_to_show() {
        let mut stream = Stream::default();

        assert_eq!(
            stream.feed("\u{1}>\u{2}>>> \u{3}\u{1}+\u{2}... \u{3}"),
            [prompt(">>> "), prompt("... ")]
        );
    }

    fn browsing() -> Stream {
        Stream::with_patterns(
            vec![Regex::new(r"^Browse\[\d+\]> $").unwrap()],
            vec![("debug".to_owned(), Regex::new(r"^debug at ").unwrap())],
        )
    }

    #[test]
    fn prompts_without_markers_are_recognized_by_their_patterns() {
        let mut stream = browsing();

        assert_eq!(
            stream.feed("Called from: f()\nBrowse[1]> "),
            [text("Called from: f()\n"), prompt("Browse[1]> ")]
        );
        assert_eq!(stream.feed("Brow"), [text("Brow")]);
        assert_eq!(stream.feed("se[2]> "), [prompt("se[2]> ")]);
        assert_eq!(stream.feed("x\n> "), [text("x\n> ")]);
    }

    #[test]
    fn watched_lines_are_shown_and_reported() {
        let mut stream = browsing();

        assert_eq!(
            stream.feed("debug at C:/a.R#2: x <- 1\r\nnext\ndebug at b"),
            [
                text("debug at C:/a.R#2: x <- 1\r\n"),
                Piece::Marker {
                    kind: "debug".to_owned(),
                    payload: "debug at C:/a.R#2: x <- 1".to_owned()
                },
                text("next\ndebug at b")
            ]
        );
        assert_eq!(
            stream.feed(".R#3: y\n"),
            [
                text(".R#3: y\n"),
                Piece::Marker {
                    kind: "debug".to_owned(),
                    payload: "debug at b.R#3: y".to_owned()
                }
            ]
        );
    }

    #[test]
    fn markers_carry_a_payload_and_may_arrive_in_pieces() {
        let mut stream = Stream::default();

        assert_eq!(stream.feed("a\u{1}plot\u{2}C:/tmp"), [text("a")]);
        assert_eq!(
            stream.feed("/p.png\u{3}b"),
            [
                Piece::Marker {
                    kind: "plot".to_owned(),
                    payload: "C:/tmp/p.png".to_owned()
                },
                text("b")
            ]
        );
    }
}
