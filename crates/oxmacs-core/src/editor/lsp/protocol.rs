use std::fmt::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
    Utf32,
}

impl Encoding {
    pub fn named(name: Option<&str>) -> Self {
        match name {
            Some("utf-8") => Self::Utf8,
            Some("utf-32") => Self::Utf32,
            _ => Self::Utf16,
        }
    }

    fn units(self, character: char) -> usize {
        match self {
            Self::Utf8 => character.len_utf8(),
            Self::Utf16 => character.len_utf16(),
            Self::Utf32 => 1,
        }
    }

    pub fn to_units(self, line: &str, column: usize) -> usize {
        line.chars()
            .take(column)
            .map(|character| self.units(character))
            .sum()
    }

    pub fn to_column(self, line: &str, units: usize) -> usize {
        let mut counted = 0;
        let mut column = 0;
        for character in line.chars() {
            if counted >= units {
                break;
            }
            counted += self.units(character);
            column += 1;
        }
        column
    }
}

fn keeps(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte)
}

pub fn uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let rooted = if text.starts_with('/') {
        text
    } else {
        format!("/{text}")
    };
    let mut encoded = String::from("file://");
    for byte in rooted.bytes() {
        if keeps(byte) {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = (bytes[at] == b'%')
            .then(|| text.get(at + 1..at + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        if let Some(byte) = escaped {
            out.push(byte);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn path(uri: &str) -> Option<PathBuf> {
    let rest = decoded(uri.strip_prefix("file://")?);
    let drive = rest.as_bytes().get(2) == Some(&b':')
        && rest.starts_with('/')
        && rest.as_bytes()[1].is_ascii_alphabetic();
    if drive {
        let mut local = rest[1..].to_owned();
        local[..1].make_ascii_uppercase();
        return Some(PathBuf::from(local.replace('/', "\\")));
    }
    Some(PathBuf::from(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_count_in_the_server_s_units() {
        let line = "a😀é b";

        assert_eq!(Encoding::Utf16.to_units(line, 3), 4);
        assert_eq!(Encoding::Utf8.to_units(line, 3), 7);
        assert_eq!(Encoding::Utf32.to_units(line, 3), 3);
        assert_eq!(Encoding::Utf16.to_column(line, 4), 3);
        assert_eq!(Encoding::Utf8.to_column(line, 7), 3);
        assert_eq!(Encoding::Utf16.to_column(line, 99), 5);
        assert_eq!(Encoding::named(None), Encoding::Utf16);
        assert_eq!(Encoding::named(Some("utf-32")), Encoding::Utf32);
    }

    #[test]
    fn paths_become_file_uris_and_back() {
        if cfg!(windows) {
            let path = PathBuf::from(r"C:\Users\ana\my code\main.rs");
            assert_eq!(uri(&path), "file:///C:/Users/ana/my%20code/main.rs");
            assert_eq!(super::path(&uri(&path)), Some(path.clone()));
            assert_eq!(
                super::path("file:///c%3A/Users/ana/my%20code/main.rs"),
                Some(path)
            );
        } else {
            let path = PathBuf::from("/home/ana/my code/naïve.rs");
            assert_eq!(uri(&path), "file:///home/ana/my%20code/na%C3%AFve.rs");
            assert_eq!(super::path(&uri(&path)), Some(path));
        }
        assert_eq!(path("https://example.com"), None);
    }
}
