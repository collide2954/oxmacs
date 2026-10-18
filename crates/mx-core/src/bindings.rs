use crate::command::Command;
use crate::key::KeySequence;
use crate::keymap::Keymap;

const GLOBAL: &[(&str, Command)] = &[
    ("RET", Command::Newline),
    ("C-d", Command::DeleteChar),
    ("<delete>", Command::DeleteChar),
    ("DEL", Command::DeleteBackwardChar),
    ("C-f", Command::ForwardChar),
    ("<right>", Command::ForwardChar),
    ("C-b", Command::BackwardChar),
    ("<left>", Command::BackwardChar),
    ("C-n", Command::NextLine),
    ("<down>", Command::NextLine),
    ("C-p", Command::PreviousLine),
    ("<up>", Command::PreviousLine),
    ("C-a", Command::MoveBeginningOfLine),
    ("<home>", Command::MoveBeginningOfLine),
    ("C-e", Command::MoveEndOfLine),
    ("<end>", Command::MoveEndOfLine),
    ("M-f", Command::ForwardWord),
    ("C-<right>", Command::ForwardWord),
    ("M-b", Command::BackwardWord),
    ("C-<left>", Command::BackwardWord),
    ("M-<", Command::BeginningOfBuffer),
    ("C-<home>", Command::BeginningOfBuffer),
    ("M->", Command::EndOfBuffer),
    ("C-<end>", Command::EndOfBuffer),
    ("C-g", Command::KeyboardQuit),
    ("C-x C-c", Command::SaveBuffersKillMx),
];

pub fn global() -> Keymap {
    let mut keymap = Keymap::default();
    for (notation, command) in GLOBAL {
        let sequence: KeySequence = notation
            .parse()
            .unwrap_or_else(|error| panic!("invalid default binding `{notation}`: {error}"));
        keymap.bind(sequence.keys(), *command);
    }
    keymap
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Lookup;

    #[test]
    fn every_global_binding_resolves_to_its_command() {
        let keymap = global();

        for (notation, command) in GLOBAL {
            let sequence: KeySequence = notation.parse().unwrap();
            assert_eq!(
                keymap.lookup(sequence.keys()),
                Lookup::Command(*command),
                "{notation}"
            );
        }
    }
}
