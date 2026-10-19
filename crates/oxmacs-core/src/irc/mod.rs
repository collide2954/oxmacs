pub mod message;
pub mod network;
pub mod session;

use crate::theme::NICK_COLORS;

pub fn nick_color(nick: &str) -> u8 {
    let hash = nick.to_ascii_lowercase().bytes().fold(0u32, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    u8::try_from(hash % u32::from(NICK_COLORS)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nick_colors_are_stable_and_ignore_case() {
        assert_eq!(nick_color("alice"), nick_color("alice"));
        assert_eq!(nick_color("Alice"), nick_color("alice"));
        assert!(nick_color("alice") < NICK_COLORS);
    }
}
