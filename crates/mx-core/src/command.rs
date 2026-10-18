use strum::{EnumString, IntoStaticStr, VariantArray};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, IntoStaticStr, VariantArray)]
#[strum(serialize_all = "kebab-case")]
pub enum Command {
    SelfInsertCommand,
    Newline,
    DeleteChar,
    DeleteBackwardChar,
    ForwardChar,
    BackwardChar,
    NextLine,
    PreviousLine,
    MoveBeginningOfLine,
    MoveEndOfLine,
    ForwardWord,
    BackwardWord,
    BeginningOfBuffer,
    EndOfBuffer,
    SetMarkCommand,
    ExchangePointAndMark,
    MarkWholeBuffer,
    Copy,
    Cut,
    Paste,
    PasteFromHistory,
    ExecuteExtendedCommand,
    ExitMinibuffer,
    MinibufferNext,
    MinibufferPrevious,
    MinibufferComplete,
    KeyboardQuit,
    SaveBuffersKillMx,
}

impl Command {
    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn is_motion(self) -> bool {
        matches!(
            self,
            Self::ForwardChar
                | Self::BackwardChar
                | Self::NextLine
                | Self::PreviousLine
                | Self::MoveBeginningOfLine
                | Self::MoveEndOfLine
                | Self::ForwardWord
                | Self::BackwardWord
                | Self::BeginningOfBuffer
                | Self::EndOfBuffer
        )
    }

    pub fn is_minibuffer_only(self) -> bool {
        matches!(
            self,
            Self::ExitMinibuffer
                | Self::MinibufferNext
                | Self::MinibufferPrevious
                | Self::MinibufferComplete
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_named_in_kebab_case() {
        assert_eq!(Command::ForwardChar.name(), "forward-char");
        assert_eq!(Command::BeginningOfBuffer.name(), "beginning-of-buffer");
    }

    #[test]
    fn motions_are_identified() {
        assert!(Command::ForwardWord.is_motion());
        assert!(Command::EndOfBuffer.is_motion());
        assert!(!Command::DeleteChar.is_motion());
        assert!(!Command::SetMarkCommand.is_motion());
    }

    #[test]
    fn commands_parse_from_their_names() {
        assert_eq!("move-end-of-line".parse(), Ok(Command::MoveEndOfLine));
        assert!("no-such-command".parse::<Command>().is_err());
    }
}
