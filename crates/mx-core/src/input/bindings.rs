use crate::command::Command;
use crate::input::{KeySequence, Keymap};

const GLOBAL: &[(&str, Command)] = &[
    ("RET", Command::Newline),
    ("C-j", Command::Newline),
    ("C-d", Command::DeleteChar),
    ("<delete>", Command::DeleteChar),
    ("DEL", Command::DeleteBackwardChar),
    ("C-k", Command::DeleteLine),
    ("M-d", Command::DeleteWord),
    ("C-<delete>", Command::DeleteWord),
    ("M-DEL", Command::BackwardDeleteWord),
    ("C-DEL", Command::BackwardDeleteWord),
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
    ("C-SPC", Command::SetMarkCommand),
    ("C-@", Command::SetMarkCommand),
    ("C-x C-x", Command::ExchangePointAndMark),
    ("C-x h", Command::MarkWholeBuffer),
    ("s-a", Command::MarkWholeBuffer),
    ("M-w", Command::Copy),
    ("s-c", Command::Copy),
    ("C-w", Command::Cut),
    ("s-x", Command::Cut),
    ("C-y", Command::Paste),
    ("s-v", Command::Paste),
    ("M-y", Command::PasteFromHistory),
    ("M-x", Command::ExecuteExtendedCommand),
    ("C-g", Command::KeyboardQuit),
    ("ESC ESC", Command::KeyboardQuit),
    ("C-x C-c", Command::SaveBuffersKillMx),
];

const MINIBUFFER: &[(&str, Command)] = &[
    ("RET", Command::ExitMinibuffer),
    ("C-j", Command::ExitMinibuffer),
    ("C-n", Command::MinibufferNext),
    ("<down>", Command::MinibufferNext),
    ("C-p", Command::MinibufferPrevious),
    ("<up>", Command::MinibufferPrevious),
    ("TAB", Command::MinibufferComplete),
];

pub fn global() -> Keymap {
    build(GLOBAL)
}

pub fn minibuffer() -> Keymap {
    build(MINIBUFFER)
}

fn build(bindings: &[(&str, Command)]) -> Keymap {
    let mut keymap = Keymap::default();
    for (notation, command) in bindings {
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
    use crate::input::Lookup;

    fn assert_resolves(keymap: &Keymap, bindings: &[(&str, Command)]) {
        for (notation, command) in bindings {
            let sequence: KeySequence = notation.parse().unwrap();
            assert_eq!(
                keymap.lookup(sequence.keys()),
                Lookup::Command(*command),
                "{notation}"
            );
        }
    }

    #[test]
    fn every_global_binding_resolves_to_its_command() {
        assert_resolves(&global(), GLOBAL);
    }

    #[test]
    fn every_minibuffer_binding_resolves_to_its_command() {
        assert_resolves(&minibuffer(), MINIBUFFER);
    }
}
