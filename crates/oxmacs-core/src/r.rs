const START: char = '\u{1}';
const SEPARATOR: char = '\u{2}';
const END: char = '\u{3}';

pub const PROFILE_VARIABLE: &str = "R_PROFILE_USER";
pub const ORIGINAL_PROFILE_VARIABLE: &str = "OXMACS_R_PROFILE_USER";

pub const HELPER: &str = r#"local({
  candidates <- c(
    Sys.getenv("OXMACS_R_PROFILE_USER"),
    ".Rprofile",
    path.expand("~/.Rprofile")
  )
  for (candidate in candidates) {
    if (nzchar(candidate) && file.exists(candidate)) {
      source(candidate)
      break
    }
  }
})
options(prompt = "\001>\002\003", continue = "\001+\002\003")
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Prompt(char),
    Marker { kind: String, payload: String },
}

#[derive(Debug, Default)]
pub struct Stream {
    pending: String,
}

impl Stream {
    pub fn feed(&mut self, chunk: &str) -> Vec<Piece> {
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
            pieces.push(match kind {
                ">" => Piece::Prompt('>'),
                "+" => Piece::Prompt('+'),
                _ => Piece::Marker {
                    kind: kind.to_owned(),
                    payload: payload.to_owned(),
                },
            });
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

    #[test]
    fn plain_output_is_text() {
        assert_eq!(Stream::default().feed("[1] 2\n"), [text("[1] 2\n")]);
    }

    #[test]
    fn prompts_are_recognised_anywhere() {
        let mut stream = Stream::default();

        assert_eq!(
            stream.feed("done\u{1}>\u{2}\u{3}x\u{1}+\u{2}\u{3}"),
            [
                text("done"),
                Piece::Prompt('>'),
                text("x"),
                Piece::Prompt('+')
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

    #[test]
    fn the_helper_sets_marked_prompts() {
        assert!(
            HELPER.contains(r#"options(prompt = "\001>\002\003", continue = "\001+\002\003")"#)
        );
        assert!(HELPER.contains(ORIGINAL_PROFILE_VARIABLE));
    }
}
