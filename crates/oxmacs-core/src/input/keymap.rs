use std::collections::HashMap;
use std::sync::OnceLock;

use indexmap::IndexMap;

use crate::command::CommandId;
use crate::input::{Key, KeySequence, Modifiers};

#[derive(Debug, Clone, Default)]
pub struct Keymap {
    bindings: IndexMap<Key, Binding>,
    first: OnceLock<HashMap<CommandId, KeySequence>>,
}

#[derive(Debug, Clone)]
enum Binding {
    Command(CommandId),
    Prefix(Keymap),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    Command(CommandId),
    Prefix,
    Unbound,
}

impl Keymap {
    pub fn bind(&mut self, keys: &[Key], command: impl Into<CommandId>) {
        self.bind_id(keys, command.into());
    }

    fn bind_id(&mut self, keys: &[Key], command: CommandId) {
        self.first = OnceLock::new();
        match keys {
            [] => {}
            [key] => {
                self.bindings.insert(*key, Binding::Command(command));
            }
            [key, rest @ ..] => {
                let binding = self
                    .bindings
                    .entry(*key)
                    .or_insert_with(|| Binding::Prefix(Self::default()));
                if let Binding::Command(_) = binding {
                    *binding = Binding::Prefix(Self::default());
                }
                if let Binding::Prefix(submap) = binding {
                    submap.bind_id(rest, command);
                }
            }
        }
    }

    pub fn where_is(&self, command: impl Into<CommandId>) -> Option<KeySequence> {
        self.first
            .get_or_init(|| self.first_bindings())
            .get(&command.into())
            .cloned()
    }

    fn first_bindings(&self) -> HashMap<CommandId, KeySequence> {
        let mut all = Vec::new();
        self.collect_all(&mut Vec::new(), &mut all);
        let mut first: HashMap<CommandId, Vec<Key>> = HashMap::new();
        for (command, keys) in all {
            let better = first.get(&command).is_none_or(|best| {
                (uses_super(&keys), keys.len()) < (uses_super(best), best.len())
            });
            if better {
                first.insert(command, keys);
            }
        }
        first
            .into_iter()
            .map(|(command, keys)| (command, KeySequence::from(keys)))
            .collect()
    }

    fn collect_all(&self, prefix: &mut Vec<Key>, found: &mut Vec<(CommandId, Vec<Key>)>) {
        for (key, binding) in &self.bindings {
            prefix.push(*key);
            match binding {
                Binding::Command(command) => found.push((*command, prefix.clone())),
                Binding::Prefix(keymap) => keymap.collect_all(prefix, found),
            }
            prefix.pop();
        }
    }

    pub fn bindings(&self, command: impl Into<CommandId>) -> Vec<KeySequence> {
        let mut found = Vec::new();
        self.collect(command.into(), &mut Vec::new(), &mut found);
        found.sort_by_key(|keys| (uses_super(keys), keys.len()));
        found.into_iter().map(KeySequence::from).collect()
    }

    fn collect(&self, command: CommandId, prefix: &mut Vec<Key>, found: &mut Vec<Vec<Key>>) {
        for (key, binding) in &self.bindings {
            prefix.push(*key);
            match binding {
                Binding::Command(bound) if *bound == command => found.push(prefix.clone()),
                Binding::Command(_) => {}
                Binding::Prefix(keymap) => keymap.collect(command, prefix, found),
            }
            prefix.pop();
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

fn uses_super(keys: &[Key]) -> bool {
    keys.iter()
        .any(|key| key.modifiers().contains(Modifiers::SUPER))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::input::KeySequence;

    fn keys(notation: &str) -> Vec<Key> {
        notation.parse::<KeySequence>().unwrap().keys().to_vec()
    }

    fn found(command: Command) -> Lookup {
        Lookup::Command(command.into())
    }

    #[test]
    fn single_key_resolves_to_its_command() {
        let mut keymap = Keymap::default();
        keymap.bind(&keys("C-f"), Command::ForwardChar);

        assert_eq!(keymap.lookup(&keys("C-f")), found(Command::ForwardChar));
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
        assert_eq!(keymap.lookup(&keys("C-x C-f")), found(Command::ForwardChar));
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
        assert_eq!(keymap.lookup(&keys("C-n")), found(Command::NextLine));
    }

    fn where_is(keymap: &Keymap, command: Command) -> Option<String> {
        keymap
            .where_is(command)
            .map(|sequence| sequence.to_string())
    }

    #[test]
    fn where_is_prefers_the_first_bound_of_equal_length() {
        let mut keymap = Keymap::default();
        keymap.bind(&keys("C-f"), Command::ForwardChar);
        keymap.bind(&keys("<right>"), Command::ForwardChar);

        assert_eq!(
            where_is(&keymap, Command::ForwardChar).as_deref(),
            Some("C-f")
        );
    }

    #[test]
    fn where_is_prefers_shorter_sequences() {
        let mut keymap = Keymap::default();
        keymap.bind(&keys("C-x C-f"), Command::ForwardChar);
        keymap.bind(&keys("C-o"), Command::ForwardChar);

        assert_eq!(
            where_is(&keymap, Command::ForwardChar).as_deref(),
            Some("C-o")
        );
    }

    #[test]
    fn where_is_shows_super_bindings_last() {
        let mut keymap = Keymap::default();
        keymap.bind(&keys("s-a"), Command::MarkWholeBuffer);
        keymap.bind(&keys("C-x h"), Command::MarkWholeBuffer);

        assert_eq!(
            where_is(&keymap, Command::MarkWholeBuffer).as_deref(),
            Some("C-x h")
        );
    }

    #[test]
    fn bindings_lists_every_sequence_in_order_of_preference() {
        let mut keymap = Keymap::default();
        keymap.bind(&keys("s-a"), Command::MarkWholeBuffer);
        keymap.bind(&keys("C-x h"), Command::MarkWholeBuffer);
        keymap.bind(&keys("C-a"), Command::MarkWholeBuffer);

        let found: Vec<String> = keymap
            .bindings(Command::MarkWholeBuffer)
            .iter()
            .map(ToString::to_string)
            .collect();

        assert_eq!(found, ["C-a", "C-x h", "s-a"]);
        assert_eq!(
            keymap.bindings(Command::BackwardChar),
            Vec::<KeySequence>::new()
        );
    }

    #[test]
    fn where_is_follows_later_bindings() {
        let mut keymap = Keymap::default();
        keymap.bind(&keys("C-x C-f"), Command::ForwardChar);
        assert_eq!(
            where_is(&keymap, Command::ForwardChar).as_deref(),
            Some("C-x C-f")
        );

        keymap.bind(&keys("C-f"), Command::ForwardChar);
        assert_eq!(
            where_is(&keymap, Command::ForwardChar).as_deref(),
            Some("C-f")
        );

        keymap.bind(&keys("C-f"), Command::BackwardChar);
        assert_eq!(
            where_is(&keymap, Command::ForwardChar).as_deref(),
            Some("C-x C-f")
        );
        assert_eq!(
            where_is(&keymap, Command::BackwardChar).as_deref(),
            Some("C-f")
        );
    }

    #[test]
    fn where_is_finds_nothing_for_unbound_commands() {
        let mut keymap = Keymap::default();
        keymap.bind(&keys("C-f"), Command::ForwardChar);

        assert_eq!(where_is(&keymap, Command::BackwardChar), None);
    }

    #[test]
    fn rebinding_inside_a_prefix_keeps_existing_bindings() {
        let mut keymap = Keymap::default();
        keymap.bind(&keys("C-x C-f"), Command::ForwardChar);
        keymap.bind(&keys("C-x C-b"), Command::BackwardChar);

        assert_eq!(keymap.lookup(&keys("C-x C-f")), found(Command::ForwardChar));
    }
}
