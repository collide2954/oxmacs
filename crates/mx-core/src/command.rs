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
    fn commands_parse_from_their_names() {
        assert_eq!("move-end-of-line".parse(), Ok(Command::MoveEndOfLine));
        assert!("no-such-command".parse::<Command>().is_err());
    }
}
