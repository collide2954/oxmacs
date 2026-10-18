use std::collections::HashMap;

use crate::command::Command;
use crate::key::Key;

#[derive(Debug, Clone, Default)]
pub struct Keymap {
    bindings: HashMap<Key, Binding>,
}

#[derive(Debug, Clone)]
enum Binding {
    Command(Command),
    Prefix(Keymap),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    Command(Command),
    Prefix,
    Unbound,
}

impl Keymap {
    pub fn bind(&mut self, keys: &[Key], command: Command) {
        match keys {
            [] => {}
            [key] => {
                self.bindings.insert(*key, Binding::Command(command));
            }
            [key, rest @ ..] => {
                let mut submap = match self.bindings.remove(key) {
                    Some(Binding::Prefix(keymap)) => keymap,
                    _ => Self::default(),
                };
                submap.bind(rest, command);
                self.bindings.insert(*key, Binding::Prefix(submap));
            }
        }
    }

    pub fn lookup(&self, keys: &[Key]) -> Lookup {
        let Some((key, rest)) = keys.split_first() else {
            return Lookup::Prefix;
        };
        match (self.bindings.get(key), rest) {
            (Some(Binding::Command(command)), []) => Lookup::Command(*command),
            (Some(Binding::Prefix(keymap)), rest) => keymap.lookup(rest),
            _ => Lookup::Unbound,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::KeySequence;

    fn keys(notation: &str) -> Vec<Key> {
        notation.parse::<KeySequence>().unwrap().keys().to_vec()
    }

    #[test]
    fn single_key_resolves_to_its_command() {
        let mut keymap = Keymap::default();
        keymap.bind(&keys("C-f"), Command::ForwardChar);

        assert_eq!(
            keymap.lookup(&keys("C-f")),
            Lookup::Command(Command::ForwardChar)
        );
    }

    #[test]
    fn unbound_keys_are_reported() {
        let keymap = Keymap::default();

        assert_eq!(keymap.lookup(&keys("C-q")), Lookup::Unbound);
    }

    #[test]
    fn prefix_of_a_sequence_awaits_more_keys() {
        let mut keymap = Keymap::default();
        keymap.bind(&keys("C-x C-f"), Command::ForwardChar);

        assert_eq!(keymap.lookup(&keys("C-x")), Lookup::Prefix);
        assert_eq!(
            keymap.lookup(&keys("C-x C-f")),
            Lookup::Command(Command::ForwardChar)
        );
        assert_eq!(keymap.lookup(&keys("C-x C-g")), Lookup::Unbound);
    }

    #[test]
    fn keys_beyond_a_command_are_unbound() {
        let mut keymap = Keymap::default();
        keymap.bind(&keys("C-f"), Command::ForwardChar);

        assert_eq!(keymap.lookup(&keys("C-f C-f")), Lookup::Unbound);
    }

    #[test]
    fn later_bindings_replace_earlier_ones() {
        let mut keymap = Keymap::default();
        keymap.bind(&keys("C-x"), Command::ForwardChar);
        keymap.bind(&keys("C-x C-b"), Command::BackwardChar);
        keymap.bind(&keys("C-n"), Command::ForwardChar);
        keymap.bind(&keys("C-n"), Command::NextLine);

        assert_eq!(keymap.lookup(&keys("C-x")), Lookup::Prefix);
        assert_eq!(
            keymap.lookup(&keys("C-n")),
            Lookup::Command(Command::NextLine)
        );
    }
}
