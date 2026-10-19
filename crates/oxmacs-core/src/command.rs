use strum::{EnumMessage, EnumString, IntoStaticStr, VariantArray};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumMessage, EnumString, IntoStaticStr, VariantArray,
)]
#[strum(serialize_all = "kebab-case")]
pub enum Command {
    #[strum(message = "Insert the character you typed")]
    SelfInsertCommand,
    #[strum(message = "Break the line here and indent the new line")]
    Newline,
    #[strum(message = "Indent the current line to its proper level")]
    IndentForTabCommand,
    #[strum(message = "Reindent every line in the selection")]
    IndentRegion,
    #[strum(message = "Comment or uncomment the line or selection")]
    CommentLine,
    #[strum(
        message = "Comment the selection, or start a comment at line end",
        detailed_message = "With a selection, comments it out, or uncomments it if it is all commented. Without one, adds a comment at the end of the line, or moves to the one already there."
    )]
    CommentDwim,
    #[strum(message = "Delete the character after the cursor")]
    DeleteChar,
    #[strum(message = "Delete the character before the cursor")]
    DeleteBackwardChar,
    #[strum(
        message = "Delete to the end of the line",
        detailed_message = "At the end of a line, deletes the line break instead. Deleted text does not go to the clipboard; use C-w to cut."
    )]
    DeleteLine,
    #[strum(message = "Delete to the end of the next word")]
    DeleteWord,
    #[strum(message = "Delete back to the start of the previous word")]
    BackwardDeleteWord,
    #[strum(message = "Move forward one character")]
    ForwardChar,
    #[strum(message = "Move back one character")]
    BackwardChar,
    #[strum(message = "Move down one line")]
    NextLine,
    #[strum(message = "Move up one line")]
    PreviousLine,
    #[strum(message = "Move to the start of the line")]
    MoveBeginningOfLine,
    #[strum(message = "Move to the end of the line")]
    MoveEndOfLine,
    #[strum(message = "Move forward to the end of a word")]
    ForwardWord,
    #[strum(message = "Move back to the start of a word")]
    BackwardWord,
    #[strum(message = "Move to the top of the buffer")]
    BeginningOfBuffer,
    #[strum(message = "Move to the bottom of the buffer")]
    EndOfBuffer,
    #[strum(message = "Show the next screen of text")]
    ScrollUpCommand,
    #[strum(message = "Show the previous screen of text")]
    ScrollDownCommand,
    #[strum(message = "Move to the start of the current function")]
    BeginningOfDefun,
    #[strum(message = "Move to the end of the current function")]
    EndOfDefun,
    #[strum(message = "Move over the next balanced expression")]
    ForwardSexp,
    #[strum(message = "Move back over the previous balanced expression")]
    BackwardSexp,
    #[strum(message = "Move out to the start of the enclosing brackets")]
    BackwardUpList,
    #[strum(message = "Move into the next pair of brackets")]
    DownList,
    #[strum(
        message = "Jump to a function, type or heading by name",
        detailed_message = "Lists the items in the buffer, such as functions, types and impls in Rust, functions and sections in R, and headings in Markdown. Type to narrow the list and press RET to jump."
    )]
    Imenu,
    #[strum(
        message = "Start a selection at the cursor",
        detailed_message = "Moving the cursor then extends the selection. Holding Shift with a motion key selects too, and C-g clears the selection."
    )]
    SetMarkCommand,
    #[strum(message = "Swap the cursor and the other end of the selection")]
    ExchangePointAndMark,
    #[strum(message = "Select the whole buffer")]
    MarkWholeBuffer,
    #[strum(
        message = "Copy the selection, or the line, to the clipboard",
        detailed_message = "With nothing selected, copies the whole current line."
    )]
    Copy,
    #[strum(
        message = "Cut the selection, or the line, to the clipboard",
        detailed_message = "With nothing selected, cuts the whole current line."
    )]
    Cut,
    #[strum(message = "Paste from the clipboard")]
    Paste,
    #[strum(
        message = "Pick an earlier clipboard entry and paste it",
        detailed_message = "Lists the last 50 texts you copied or cut, newest first. Type to narrow the list and press RET to paste the selected one."
    )]
    PasteFromHistory,
    #[strum(
        message = "Undo the last change",
        detailed_message = "Typing is undone a few words at a time. Undoing again keeps going back; redo goes forward again."
    )]
    Undo,
    #[strum(message = "Redo the last undone change")]
    Redo,
    #[strum(
        message = "Run a command by name",
        detailed_message = "Type part of a name to narrow the list; several words can come in any order. Each command shows its key and what it does. Recently used commands come first, RET runs the selected one, and C-g cancels."
    )]
    ExecuteExtendedCommand,
    #[strum(message = "Accept the selected completion or the input")]
    ExitMinibuffer,
    #[strum(message = "Select the next completion")]
    MinibufferNext,
    #[strum(message = "Select the previous completion")]
    MinibufferPrevious,
    #[strum(message = "Complete the input to the selected candidate")]
    MinibufferComplete,
    #[strum(message = "Delete back, or go up a directory after a /")]
    MinibufferDeleteBackward,
    #[strum(message = "Accept exactly what you typed, ignoring completions")]
    ExitMinibufferWithInput,
    #[strum(
        message = "Search forward as you type",
        detailed_message = "Type to search; the cursor moves to the first match as you type. C-s goes to the next match and wraps around after a failed search, C-r goes back, DEL removes a character, M-r switches to regular expressions, C-w adds the next word and C-y adds the clipboard. RET stops at the match and C-g returns to where you started. Searching ignores case unless you type a capital letter."
    )]
    IsearchForward,
    #[strum(
        message = "Search backward as you type",
        detailed_message = "Works like isearch-forward, but searches toward the start of the buffer."
    )]
    IsearchBackward,
    #[strum(
        message = "Replace text, asking at each match",
        detailed_message = "Asks for the text to replace and its replacement, then stops at each match after the cursor, or in the selection. Answer y or SPC to replace, n or DEL to skip, ! to replace all the rest, . to replace this one and stop, ^ to go back, and q or RET to finish. Replacements keep the case of the text they replace."
    )]
    QueryReplace,
    #[strum(
        message = "Replace a regular expression, asking at each match",
        detailed_message = "Works like query-replace, but the search is a regular expression in Rust syntax and the replacement can use groups such as \\1."
    )]
    QueryReplaceRegexp,
    #[strum(message = "Search forward for a regular expression")]
    IsearchForwardRegexp,
    #[strum(message = "Search backward for a regular expression")]
    IsearchBackwardRegexp,
    #[strum(message = "Go to the next match")]
    IsearchRepeatForward,
    #[strum(message = "Go to the previous match")]
    IsearchRepeatBackward,
    #[strum(message = "Add the typed character to the search")]
    IsearchPrintingChar,
    #[strum(message = "Remove the last character from the search")]
    IsearchDeleteChar,
    #[strum(message = "Switch between plain and regular expression search")]
    IsearchToggleRegexp,
    #[strum(message = "Add the next word at the match to the search")]
    IsearchYankWordOrChar,
    #[strum(message = "Add the clipboard text to the search")]
    IsearchYankKill,
    #[strum(message = "Replace the current search, asking at each match")]
    IsearchQueryReplace,
    #[strum(message = "Regexp-replace the current search, asking each time")]
    IsearchQueryReplaceRegexp,
    #[strum(message = "Stop searching and stay at the match")]
    IsearchExit,
    #[strum(message = "Cancel the search and return to where it began")]
    IsearchAbort,
    #[strum(
        message = "Open a file, creating it if it is new",
        detailed_message = "Type a path and the list narrows to matching names. TAB or / enters the selected directory, DEL right after a / goes up a level, and RET opens the selection. M-RET opens exactly what you typed, which is how you name a new file. ~ stands for your home directory, and opening a directory lists it in dired."
    )]
    FindFile,
    #[strum(message = "Save the buffer to its file")]
    SaveBuffer,
    #[strum(message = "Save the buffer to a different file")]
    WriteFile,
    #[strum(
        message = "Offer to save each modified buffer",
        detailed_message = "For each modified buffer, answer y to save it, n to skip it, ! to save it and all the rest, or q to stop asking."
    )]
    SaveSomeBuffers,
    #[strum(
        message = "Switch to another buffer",
        detailed_message = "Recently used buffers come first. Type to narrow the list and press RET to switch."
    )]
    SwitchToBuffer,
    #[strum(message = "Close a buffer, asking first if it has changes")]
    KillBuffer,
    #[strum(
        message = "Reload the buffer from its file, or refresh it",
        detailed_message = "In a file buffer, reloads the file and discards unsaved edits. In listings and reports such as dired, refreshes their contents."
    )]
    RevertBuffer,
    #[strum(message = "Toggle whether the buffer can be edited")]
    ReadOnlyMode,
    #[strum(
        message = "List a directory to manage its files",
        detailed_message = "In the listing, RET opens a file or directory, ^ goes up and g refreshes. Mark files with m and flag them for deletion with d; x deletes the flagged files, D deletes the marked ones, R renames or moves, C copies, + creates a directory and W opens a file with its usual app. s changes the sort and ( hides the details. Deleted files go to the trash unless delete-to-trash is off."
    )]
    Dired,
    #[strum(message = "List the directory of the current file")]
    DiredJump,
    #[strum(message = "Move to the next file")]
    DiredNextLine,
    #[strum(message = "Move to the previous file")]
    DiredPreviousLine,
    #[strum(message = "Open the file or directory on this line")]
    DiredFindFile,
    #[strum(message = "Go up to the parent directory")]
    DiredUpDirectory,
    #[strum(message = "Sort by name, newest or largest")]
    DiredCycleSort,
    #[strum(message = "Show or hide sizes and dates")]
    DiredHideDetailsMode,
    #[strum(message = "Mark this file and move on")]
    DiredMark,
    #[strum(message = "Flag this file for deletion and move on")]
    DiredFlagFileDeletion,
    #[strum(message = "Remove this file's mark and move on")]
    DiredUnmark,
    #[strum(message = "Move back and remove that file's mark")]
    DiredUnmarkBackward,
    #[strum(message = "Remove every mark and deletion flag")]
    DiredUnmarkAllMarks,
    #[strum(message = "Mark the unmarked files and unmark the marked ones")]
    DiredToggleMarks,
    #[strum(message = "Delete the files flagged for deletion")]
    DiredDoFlaggedDelete,
    #[strum(message = "Delete the marked files, or this one")]
    DiredDoDelete,
    #[strum(message = "Rename or move the marked files, or this one")]
    DiredDoRename,
    #[strum(message = "Copy the marked files, or this one")]
    DiredDoCopy,
    #[strum(message = "Create a new directory")]
    DiredCreateDirectory,
    #[strum(message = "Open the file with its usual app")]
    DiredDoOpen,
    #[strum(message = "Leave this buffer and put it out of the way")]
    QuitWindow,
    #[strum(
        message = "Cancel the current command or prompt",
        detailed_message = "Clears the selection, closes the minibuffer or cancels a half-typed key sequence."
    )]
    KeyboardQuit,
    #[strum(message = "Offer to save changes, then quit oxmacs")]
    SaveBuffersKillOxmacs,
    #[strum(message = "Quit oxmacs at once without saving")]
    KillOxmacs,
    #[strum(message = "Show the oxmacs start screen")]
    AboutOxmacs,
    #[strum(
        message = "Switch to a color theme by name",
        detailed_message = "The new theme shows as an unsaved change in *Settings*; save it there with C-x C-s to keep it."
    )]
    LoadTheme,
    #[strum(
        message = "Switch between the light and dark theme",
        detailed_message = "The new theme shows as an unsaved change in *Settings*; save it there with C-x C-s to keep it."
    )]
    ToggleTheme,
    #[strum(message = "Edit the buffer as Rust")]
    RustMode,
    #[strum(message = "Edit the buffer as TOML")]
    TomlMode,
    #[strum(message = "Edit the buffer as Markdown")]
    MarkdownMode,
    #[strum(message = "Edit the buffer as R")]
    RMode,
    #[strum(message = "Edit the buffer as plain text")]
    TextMode,
    #[strum(
        message = "Check the config, font, languages and tools",
        detailed_message = "Opens *oxmacs-doctor*, a report on config.toml, the font in use, what each language supports, and external tools such as cargo, rustfmt and R. g refreshes it and q closes it."
    )]
    OxmacsDoctor,
    #[strum(message = "Split the window into top and bottom")]
    SplitWindowBelow,
    #[strum(message = "Split the window into left and right")]
    SplitWindowRight,
    #[strum(message = "Select the next window")]
    OtherWindow,
    #[strum(message = "Close the selected window")]
    DeleteWindow,
    #[strum(message = "Close every window but the selected one")]
    DeleteOtherWindows,
    #[strum(message = "Select the window to the left")]
    WindmoveLeft,
    #[strum(message = "Select the window to the right")]
    WindmoveRight,
    #[strum(message = "Select the window above")]
    WindmoveUp,
    #[strum(message = "Select the window below")]
    WindmoveDown,
    #[strum(message = "Make all windows the same size")]
    BalanceWindows,
    #[strum(message = "Make the window taller")]
    EnlargeWindow,
    #[strum(message = "Make the window wider")]
    EnlargeWindowHorizontally,
    #[strum(message = "Make the window narrower")]
    ShrinkWindowHorizontally,
    #[strum(message = "Format the buffer with rustfmt")]
    RustFormatBuffer,
    #[strum(message = "Run cargo check in *compilation*")]
    RustCheck,
    #[strum(message = "Run cargo build in *compilation*")]
    RustCompile,
    #[strum(message = "Run cargo test in *compilation*")]
    RustTest,
    #[strum(message = "Run cargo run in *compilation*")]
    RustRun,
    #[strum(message = "Run cargo clippy in *compilation*")]
    RustRunClippy,
    #[strum(message = "Insert the assignment arrow <-")]
    RInsertAssignment,
    #[strum(message = "Insert the native pipe |>")]
    RInsertPipe,
    #[strum(
        message = "Start an R console beside the script",
        detailed_message = "Starts R in *R*, using the r-program setting or the R found on your system. Type after the prompt and press RET to run it; M-p and M-n bring back earlier input."
    )]
    RunR,
    #[strum(message = "Send the input after the prompt")]
    ComintSendInput,
    #[strum(message = "Bring back the previous input")]
    ComintPreviousInput,
    #[strum(message = "Bring back the next input")]
    ComintNextInput,
    #[strum(message = "Continue the list with a new item")]
    MarkdownInsertListItem,
    #[strum(message = "Raise the heading or list item one level")]
    MarkdownPromote,
    #[strum(message = "Lower the heading or list item one level")]
    MarkdownDemote,
    #[strum(
        message = "Show or hide a rendered preview beside the text",
        detailed_message = "The preview follows your edits. In the preview, SPC and DEL scroll by a screen and q closes it."
    )]
    MarkdownPreview,
    #[strum(
        message = "Run a shell command and collect its errors",
        detailed_message = "Runs the command in the project directory and shows its output in *compilation*. RET or a click on an error visits it, n and p show errors while staying in the list, g runs the command again and C-c C-k stops it. M-g n and M-g p step through the errors from any buffer."
    )]
    Compile,
    #[strum(message = "Run the last compile command again")]
    Recompile,
    #[strum(message = "Stop the running compilation")]
    KillCompilation,
    #[strum(
        message = "Visit the next error or lint",
        detailed_message = "Steps through the errors of the last compile or the last lint, whichever ran most recently."
    )]
    NextError,
    #[strum(message = "Visit the previous error or lint")]
    PreviousError,
    #[strum(message = "Show the next error but stay in the list")]
    NextErrorNoSelect,
    #[strum(message = "Show the previous error but stay in the list")]
    PreviousErrorNoSelect,
    #[strum(message = "Move to the next error line")]
    CompilationNextError,
    #[strum(message = "Move to the previous error line")]
    CompilationPreviousError,
    #[strum(message = "Visit the error on this line")]
    CompileGotoError,
    #[strum(
        message = "Change settings in a form",
        detailed_message = "Opens *Settings*, grouped by section. n and p move between settings; RET or a click toggles a switch, cycles a choice or asks for a value. Changes apply at once, C-x C-s writes them to config.toml and g discards them."
    )]
    Customize,
    #[strum(
        message = "Open config.toml; saving it applies the settings",
        detailed_message = "Opens config.toml as a TOML buffer, empty if the file does not exist yet. Saving applies the settings at once; invalid settings are reported and the old ones kept. M-x customize offers a form instead."
    )]
    FindConfigFile,
    #[strum(
        message = "Explain a command, its keys and where it works",
        detailed_message = "Lists every command with its key and what it does. RET opens *Help* beside the current window with the full description; q closes it."
    )]
    DescribeCommand,
    #[strum(
        message = "List the project's errors and warnings",
        detailed_message = "Opens *Diagnostics* with every error and warning from the last lint, grouped by file. RET visits one, SPC shows it while staying in the list and g refreshes."
    )]
    ListDiagnostics,
    #[strum(message = "Visit the diagnostic on this line")]
    GotoDiagnostic,
    #[strum(message = "Show this diagnostic but stay in the list")]
    ShowDiagnostic,
    #[strum(
        message = "Show or hide line numbers in this buffer",
        detailed_message = "Affects only the current buffer. The line-numbers setting chooses the default for every file buffer."
    )]
    DisplayLineNumbersMode,
    #[strum(message = "Move to the next setting")]
    WidgetForward,
    #[strum(message = "Move to the previous setting")]
    WidgetBackward,
    #[strum(message = "Change the setting on this line")]
    WidgetButtonPress,
    #[strum(message = "Move to the next key line")]
    ForwardButton,
    #[strum(message = "Move to the previous key line")]
    BackwardButton,
    #[strum(message = "Run the command on this line")]
    PushButton,
}

impl Command {
    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn summary(self) -> &'static str {
        self.get_message().unwrap_or_default()
    }

    pub fn help(self) -> Option<&'static str> {
        self.get_detailed_message()
            .filter(|help| Some(*help) != self.get_message())
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
    Help,
}

impl Scope {
    pub fn label(self) -> Option<&'static str> {
        match self {
            Self::Global => None,
            Self::Minibuffer => Some("the minibuffer"),
            Self::Search => Some("searches"),
            Self::Dired => Some("dired"),
            Self::Splash => Some("the start screen"),
            Self::Doctor => Some("*oxmacs-doctor*"),
            Self::Rust => Some("Rust buffers"),
            Self::R => Some("R buffers"),
            Self::Markdown => Some("Markdown buffers"),
            Self::Preview => Some("previews"),
            Self::Console => Some("consoles"),
            Self::Compilation => Some("*compilation*"),
            Self::Customize => Some("*Settings*"),
            Self::Diagnostics => Some("*Diagnostics*"),
            Self::Help => Some("*Help*"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_a_short_summary() {
        for command in Command::VARIANTS {
            let summary = command.summary();
            let name = command.name();
            assert!(!summary.is_empty(), "{name} has no summary");
            assert!(summary.chars().count() <= 60, "{name}: {summary}");
            assert!(summary.starts_with(char::is_uppercase), "{name}: {summary}");
            assert!(!summary.ends_with('.'), "{name}: {summary}");
        }
    }

    #[test]
    fn longer_help_is_written_in_sentences() {
        let helped: Vec<Command> = Command::VARIANTS
            .iter()
            .copied()
            .filter(|command| command.help().is_some())
            .collect();

        assert!(helped.contains(&Command::FindFile));
        for command in helped {
            let help = command.help().unwrap_or_default();
            assert!(help.ends_with('.'), "{}: {help}", command.name());
        }
    }

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
