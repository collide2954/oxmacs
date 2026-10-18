use strum::{EnumString, IntoStaticStr, VariantArray};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, IntoStaticStr, VariantArray)]
#[strum(serialize_all = "kebab-case")]
pub enum Command {
    SelfInsertCommand,
    Newline,
    DeleteChar,
    DeleteBackwardChar,
    DeleteLine,
    DeleteWord,
    BackwardDeleteWord,
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
    Undo,
    Redo,
    ExecuteExtendedCommand,
    ExitMinibuffer,
    MinibufferNext,
    MinibufferPrevious,
    MinibufferComplete,
    MinibufferDeleteBackward,
    ExitMinibufferWithInput,
    FindFile,
    SaveBuffer,
    WriteFile,
    SaveSomeBuffers,
    SwitchToBuffer,
    KillBuffer,
    RevertBuffer,
    ReadOnlyMode,
    Dired,
    DiredJump,
    DiredNextLine,
    DiredPreviousLine,
    DiredFindFile,
    DiredUpDirectory,
    DiredCycleSort,
    DiredHideDetailsMode,
    DiredMark,
    DiredFlagFileDeletion,
    DiredUnmark,
    DiredUnmarkBackward,
    DiredUnmarkAllMarks,
    DiredToggleMarks,
    DiredDoFlaggedDelete,
    DiredDoDelete,
    DiredDoRename,
    DiredDoCopy,
    DiredCreateDirectory,
    DiredDoOpen,
    QuitWindow,
    KeyboardQuit,
    SaveBuffersKillMx,
    KillMx,
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

    pub fn modifies_text(self) -> bool {
        matches!(
            self,
            Self::SelfInsertCommand
                | Self::Newline
                | Self::DeleteChar
                | Self::DeleteBackwardChar
                | Self::DeleteLine
                | Self::DeleteWord
                | Self::BackwardDeleteWord
                | Self::Cut
                | Self::Paste
                | Self::PasteFromHistory
                | Self::Undo
                | Self::Redo
        )
    }

    pub fn scope(self) -> Scope {
        match self {
            Self::ExitMinibuffer
            | Self::MinibufferNext
            | Self::MinibufferPrevious
            | Self::MinibufferComplete
            | Self::MinibufferDeleteBackward
            | Self::ExitMinibufferWithInput => Scope::Minibuffer,
            Self::DiredNextLine
            | Self::DiredPreviousLine
            | Self::DiredFindFile
            | Self::DiredUpDirectory
            | Self::DiredCycleSort
            | Self::DiredHideDetailsMode
            | Self::DiredMark
            | Self::DiredFlagFileDeletion
            | Self::DiredUnmark
            | Self::DiredUnmarkBackward
            | Self::DiredUnmarkAllMarks
            | Self::DiredToggleMarks
            | Self::DiredDoFlaggedDelete
            | Self::DiredDoDelete
            | Self::DiredDoRename
            | Self::DiredDoCopy
            | Self::DiredCreateDirectory
            | Self::DiredDoOpen => Scope::Dired,
            _ => Scope::Global,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Minibuffer,
    Dired,
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
    fn commands_belong_to_a_scope() {
        assert_eq!(Command::ForwardChar.scope(), Scope::Global);
        assert_eq!(Command::ExitMinibuffer.scope(), Scope::Minibuffer);
        assert_eq!(Command::DiredNextLine.scope(), Scope::Dired);
    }

    #[test]
    fn commands_parse_from_their_names() {
        assert_eq!("move-end-of-line".parse(), Ok(Command::MoveEndOfLine));
        assert!("no-such-command".parse::<Command>().is_err());
    }
}
