use std::fmt::Write;
use std::path::{Path, PathBuf};

fn keeps(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte)
}

pub fn from_path(path: &Path) -> String {
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

pub fn to_path(uri: &str) -> Option<PathBuf> {
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
    fn paths_round_trip_through_percent_encoded_file_uris() {
        for (local, encoded) in [
            (
                r"C:\Users\ana\my code\a#b%c.ox",
                "file:///C:/Users/ana/my%20code/a%23b%25c.ox",
            ),
            (
                "/home/ana/naïve €.ox",
                "file:///home/ana/na%C3%AFve%20%E2%82%AC.ox",
            ),
        ] {
            assert_eq!(from_path(Path::new(local)), encoded);
            assert_eq!(to_path(encoded), Some(PathBuf::from(local)));
        }
    }

    #[test]
    fn odd_escapes_decode_as_far_as_they_can() {
        for (encoded, local) in [
            ("file:///c%3A/x/y.ox", r"C:\x\y.ox"),
            ("file:///tmp/100%", "/tmp/100%"),
            ("file:///tmp/%zz%4", "/tmp/%zz%4"),
            ("file:///tmp/%aé%é%41", "/tmp/%aé%éA"),
        ] {
            assert_eq!(to_path(encoded), Some(PathBuf::from(local)), "{encoded}");
        }
        assert_eq!(to_path("https://example.com"), None);
    }
}
