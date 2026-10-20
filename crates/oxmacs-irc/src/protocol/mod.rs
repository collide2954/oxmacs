pub mod message;
pub mod network;
pub mod session;

use oxmacs_core::theme::NICK_COLORS;

pub fn nick_color(nick: &str) -> u8 {
    let hash = nick.to_ascii_lowercase().bytes().fold(0u32, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    u8::try_from(hash % u32::from(NICK_COLORS)).unwrap_or_default()
}

const COLOR: char = '\u{3}';
const HEX_COLOR: char = '\u{4}';

fn skip_code(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, digits: usize, hex: bool) {
    let accepts = |c: &char| {
        if hex {
            c.is_ascii_hexdigit()
        } else {
            c.is_ascii_digit()
        }
    };
    let take = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
        for _ in 0..digits {
            if chars.next_if(accepts).is_none() {
                break;
            }
        }
    };
    take(chars);
    let mut ahead = chars.clone();
    if ahead.next() == Some(',') && ahead.next().as_ref().is_some_and(accepts) {
        chars.next();
        take(chars);
    }
}

pub fn plain(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            COLOR => skip_code(&mut chars, 2, false),
            HEX_COLOR => skip_code(&mut chars, 6, true),
            '\t' => plain.push(' '),
            '\u{200b}'..='\u{200d}' | '\u{2060}' | '\u{feff}' => {}
            c if c.is_control() => {}
            c => plain.push(c),
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_drops_formatting_but_keeps_look_alike_text() {
        assert_eq!(plain("\u{3}04,12red\u{3}"), "red");
        assert_eq!(plain("\u{3}4,hello"), ",hello");
        assert_eq!(plain("\u{4}FF0000,00FF00x"), "x");
        assert_eq!(plain("a\tb"), "a b");
        assert_eq!(plain("12,34"), "12,34");
    }

    #[test]
    fn nick_colors_are_stable_and_ignore_case() {
        assert_eq!(nick_color("alice"), nick_color("alice"));
        assert_eq!(nick_color("Alice"), nick_color("alice"));
        assert!(nick_color("alice") < NICK_COLORS);
    }
}
