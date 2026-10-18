use thiserror::Error;

const BOM: &[u8] = b"\xEF\xBB\xBF";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Format {
    pub line_ending: LineEnding,
    pub bom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub format: Format,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecodeError {
    #[error("not valid UTF-8 at byte {offset}")]
    NotUtf8 { offset: usize },
}

pub fn decode(bytes: &[u8]) -> Result<Decoded, DecodeError> {
    let (bom, body) = match bytes.strip_prefix(BOM) {
        Some(rest) => (true, rest),
        None => (false, bytes),
    };
    let text = std::str::from_utf8(body).map_err(|error| DecodeError::NotUtf8 {
        offset: error.valid_up_to() + if bom { BOM.len() } else { 0 },
    })?;
    let line_ending = detect_line_ending(text);
    let text = match line_ending {
        LineEnding::Lf => text.to_owned(),
        LineEnding::CrLf => text.replace("\r\n", "\n"),
    };
    Ok(Decoded {
        text,
        format: Format { line_ending, bom },
    })
}

pub fn encode(text: &str, format: Format) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len() + BOM.len());
    if format.bom {
        bytes.extend_from_slice(BOM);
    }
    match format.line_ending {
        LineEnding::Lf => bytes.extend_from_slice(text.as_bytes()),
        LineEnding::CrLf => bytes.extend_from_slice(text.replace('\n', "\r\n").as_bytes()),
    }
    bytes
}

fn detect_line_ending(text: &str) -> LineEnding {
    let newlines = text.matches('\n').count();
    if newlines > 0 && text.matches("\r\n").count() == newlines {
        LineEnding::CrLf
    } else {
        LineEnding::Lf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(bytes: &[u8]) -> Decoded {
        decode(bytes).unwrap()
    }

    #[test]
    fn lf_text_decodes_unchanged() {
        let file = decoded(b"one\ntwo\n");

        assert_eq!(file.text, "one\ntwo\n");
        assert_eq!(file.format, Format::default());
        assert_eq!(file.format.line_ending, LineEnding::Lf);
    }

    #[test]
    fn crlf_text_is_normalized_and_remembered() {
        let file = decoded(b"one\r\ntwo\r\n");

        assert_eq!(file.text, "one\ntwo\n");
        assert_eq!(file.format.line_ending, LineEnding::CrLf);
    }

    #[test]
    fn mixed_line_endings_stay_lf_and_keep_carriage_returns() {
        let file = decoded(b"one\r\ntwo\n");

        assert_eq!(file.text, "one\r\ntwo\n");
        assert_eq!(file.format.line_ending, LineEnding::Lf);
    }

    #[test]
    fn a_utf8_byte_order_mark_is_stripped_and_remembered() {
        let file = decoded(b"\xEF\xBB\xBFhi");

        assert_eq!(file.text, "hi");
        assert!(file.format.bom);
    }

    #[test]
    fn invalid_utf8_is_refused_with_its_offset() {
        assert_eq!(decode(b"ok\xFFno"), Err(DecodeError::NotUtf8 { offset: 2 }));
        assert_eq!(
            decode(b"\xEF\xBB\xBFok\xFF"),
            Err(DecodeError::NotUtf8 { offset: 5 })
        );
    }

    #[test]
    fn encoding_restores_the_original_bytes() {
        for original in [
            b"one\ntwo\n".as_slice(),
            b"one\r\ntwo\r\n",
            b"\xEF\xBB\xBFone\r\n",
            b"one\r\ntwo\n",
            b"",
        ] {
            let file = decoded(original);

            assert_eq!(encode(&file.text, file.format), original);
        }
    }
}
