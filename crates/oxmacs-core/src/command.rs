use strum::{EnumString, IntoStaticStr, VariantArray};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, IntoStaticStr, VariantArray)]
#[strum(serialize_all = "kebab-case")]
pub enum Command {
    SelfInsertCommand,
    Newline,
    IndentForTabCommand,
    IndentRegion,
    CommentLine,
    CommentDwim,
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
    ScrollUpCommand,
    ScrollDownCommand,
    BeginningOfDefun,
    EndOfDefun,
    ForwardSexp,
    BackwardSexp,
    BackwardUpList,
    DownList,
    Imenu,
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
    IsearchForward,
    IsearchBackward,
    QueryReplace,
    QueryReplaceRegexp,
    IsearchForwardRegexp,
    IsearchBackwardRegexp,
    IsearchRepeatForward,
    IsearchRepeatBackward,
    IsearchPrintingChar,
    IsearchDeleteChar,
    IsearchToggleRegexp,
    IsearchYankWordOrChar,
    IsearchYankKill,
    IsearchQueryReplace,
    IsearchQueryReplaceRegexp,
    IsearchExit,
    IsearchAbort,
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
    SaveBuffersKillOxmacs,
    KillOxmacs,
    AboutOxmacs,
    LoadTheme,
    ToggleTheme,
    RustMode,
    TomlMode,
    MarkdownMode,
    RMode,
    TextMode,
    OxmacsDoctor,
    SplitWindowBelow,
    SplitWindowRight,
    OtherWindow,
    DeleteWindow,
    DeleteOtherWindows,
    WindmoveLeft,
    WindmoveRight,
    WindmoveUp,
    WindmoveDown,
    BalanceWindows,
    EnlargeWindow,
    EnlargeWindowHorizontally,
    ShrinkWindowHorizontally,
    RustFormatBuffer,
    RustCheck,
    RustCompile,
    RustTest,
    RustRun,
    RustRunClippy,
    RInsertAssignment,
    RInsertPipe,
    RunR,
    ComintSendInput,
    ComintPreviousInput,
    ComintNextInput,
    MarkdownInsertListItem,
    MarkdownPromote,
    MarkdownDemote,
    MarkdownPreview,
    Compile,
    Recompile,
    KillCompilation,
    NextError,
    PreviousError,
    NextErrorNoSelect,
    PreviousErrorNoSelect,
    CompilationNextError,
    CompilationPreviousError,
    CompileGotoError,
    Customize,
    ListDiagnostics,
    GotoDiagnostic,
    ShowDiagnostic,
    DisplayLineNumbersMode,
    WidgetForward,
    WidgetBackward,
    WidgetButtonPress,
    ForwardButton,
    BackwardButton,
    PushButton,
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
                | Self::ScrollUpCommand
                | Self::ScrollDownCommand
                | Self::BeginningOfDefun
                | Self::EndOfDefun
                | Self::ForwardSexp
                | Self::BackwardSexp
                | Self::BackwardUpList
                | Self::DownList
        )
    }

    pub fn modifies_text(self) -> bool {
        matches!(
            self,
            Self::SelfInsertCommand
                | Self::Newline
                | Self::IndentForTabCommand
                | Self::IndentRegion
                | Self::CommentLine
                | Self::CommentDwim
                | Self::RustFormatBuffer
                | Self::RInsertAssignment
                | Self::RInsertPipe
                | Self::MarkdownInsertListItem
                | Self::MarkdownPromote
                | Self::MarkdownDemote
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
                | Self::QueryReplace
                | Self::QueryReplaceRegexp
                | Self::IsearchQueryReplace
                | Self::IsearchQueryReplaceRegexp
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
            Self::ForwardButton | Self::BackwardButton | Self::PushButton => Scope::Splash,
            Self::WidgetForward | Self::WidgetBackward | Self::WidgetButtonPress => {
                Scope::Customize
            }
            Self::RustFormatBuffer
            | Self::RustCheck
            | Self::RustCompile
            | Self::RustTest
            | Self::RustRun
            | Self::RustRunClippy => Scope::Rust,
            Self::RInsertAssignment | Self::RInsertPipe => Scope::R,
            Self::ComintSendInput | Self::ComintPreviousInput | Self::ComintNextInput => {
                Scope::Console
            }
            Self::Recompile
            | Self::KillCompilation
            | Self::NextErrorNoSelect
            | Self::PreviousErrorNoSelect
            | Self::CompilationNextError
            | Self::CompilationPreviousError
            | Self::CompileGotoError => Scope::Compilation,
            Self::MarkdownInsertListItem
            | Self::MarkdownPromote
            | Self::MarkdownDemote
            | Self::MarkdownPreview => Scope::Markdown,
            Self::IsearchRepeatForward
            | Self::IsearchRepeatBackward
            | Self::IsearchPrintingChar
            | Self::IsearchDeleteChar
            | Self::IsearchToggleRegexp
            | Self::IsearchYankWordOrChar
            | Self::IsearchYankKill
            | Self::IsearchQueryReplace
            | Self::IsearchQueryReplaceRegexp
            | Self::IsearchExit
            | Self::IsearchAbort => Scope::Search,
            _ => Scope::Global,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Minibuffer,
    Dired,
    Splash,
    Search,
    Doctor,
    Rust,
    R,
    Markdown,
    Preview,
    Console,
    Compilation,
    Customize,
    Diagnostics,
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
