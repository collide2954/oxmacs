use crate::command::Command;
use crate::input::{KeySequence, Keymap};

const GLOBAL: &[(&str, Command)] = &[
    ("RET", Command::Newline),
    ("C-j", Command::Newline),
    ("TAB", Command::IndentForTabCommand),
    ("C-M-\\", Command::IndentRegion),
    ("C-x C-;", Command::CommentLine),
    ("s-/", Command::CommentLine),
    ("M-;", Command::CommentDwim),
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
    ("C-v", Command::ScrollUpCommand),
    ("<next>", Command::ScrollUpCommand),
    ("M-v", Command::ScrollDownCommand),
    ("<prior>", Command::ScrollDownCommand),
    ("C-M-a", Command::BeginningOfDefun),
    ("C-M-e", Command::EndOfDefun),
    ("C-M-f", Command::ForwardSexp),
    ("C-M-<right>", Command::ForwardSexp),
    ("C-M-b", Command::BackwardSexp),
    ("C-M-<left>", Command::BackwardSexp),
    ("C-M-u", Command::BackwardUpList),
    ("C-M-d", Command::DownList),
    ("M-g i", Command::Imenu),
    ("C-c n f", Command::AtlasFindNote),
    ("C-c n t", Command::AtlasToday),
    ("C-c n c", Command::AtlasCapture),
    ("C-c n b", Command::AtlasBacklinks),
    ("C-c n a", Command::AtlasAgenda),
    ("C-c n m", Command::AtlasCalendar),
    ("M-,", Command::XrefGoBack),
    ("M-g n", Command::NextError),
    ("M-g M-n", Command::NextError),
    ("C-x `", Command::NextError),
    ("M-g p", Command::PreviousError),
    ("M-g M-p", Command::PreviousError),
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
    ("C-/", Command::Undo),
    ("C-_", Command::Undo),
    ("C-x u", Command::Undo),
    ("s-z", Command::Undo),
    ("C-?", Command::Redo),
    ("C-M-_", Command::Redo),
    ("s-S-z", Command::Redo),
    ("M-x", Command::ExecuteExtendedCommand),
    ("C-h f", Command::DescribeCommand),
    ("C-s", Command::IsearchForward),
    ("C-r", Command::IsearchBackward),
    ("C-M-s", Command::IsearchForwardRegexp),
    ("C-M-r", Command::IsearchBackwardRegexp),
    ("M-%", Command::QueryReplace),
    ("C-M-%", Command::QueryReplaceRegexp),
    ("C-x C-f", Command::FindFile),
    ("C-x C-s", Command::SaveBuffer),
    ("C-x s", Command::SaveSomeBuffers),
    ("C-x C-w", Command::WriteFile),
    ("C-x b", Command::SwitchToBuffer),
    ("C-x k", Command::KillBuffer),
    ("C-x 2", Command::SplitWindowBelow),
    ("C-x 3", Command::SplitWindowRight),
    ("C-x o", Command::OtherWindow),
    ("C-x 0", Command::DeleteWindow),
    ("C-x 1", Command::DeleteOtherWindows),
    ("C-c <left>", Command::WindmoveLeft),
    ("C-c <right>", Command::WindmoveRight),
    ("C-c <up>", Command::WindmoveUp),
    ("C-c <down>", Command::WindmoveDown),
    ("C-x +", Command::BalanceWindows),
    ("C-x ^", Command::EnlargeWindow),
    ("C-x }", Command::EnlargeWindowHorizontally),
    ("C-x {", Command::ShrinkWindowHorizontally),
    ("C-x C-q", Command::ReadOnlyMode),
    ("C-x d", Command::Dired),
    ("C-x C-j", Command::DiredJump),
    ("C-g", Command::KeyboardQuit),
    ("ESC ESC", Command::KeyboardQuit),
    ("C-x C-c", Command::SaveBuffersKillOxmacs),
];

const MINIBUFFER: &[(&str, Command)] = &[
    ("RET", Command::ExitMinibuffer),
    ("C-j", Command::ExitMinibuffer),
    ("C-n", Command::MinibufferNext),
    ("<down>", Command::MinibufferNext),
    ("C-p", Command::MinibufferPrevious),
    ("<up>", Command::MinibufferPrevious),
    ("TAB", Command::MinibufferComplete),
    ("DEL", Command::MinibufferDeleteBackward),
    ("M-RET", Command::ExitMinibufferWithInput),
];

const SEARCH: &[(&str, Command)] = &[
    ("C-s", Command::IsearchRepeatForward),
    ("C-r", Command::IsearchRepeatBackward),
    ("C-M-s", Command::IsearchRepeatForward),
    ("C-M-r", Command::IsearchRepeatBackward),
    ("DEL", Command::IsearchDeleteChar),
    ("M-r", Command::IsearchToggleRegexp),
    ("C-w", Command::IsearchYankWordOrChar),
    ("C-y", Command::IsearchYankKill),
    ("s-v", Command::IsearchYankKill),
    ("M-%", Command::IsearchQueryReplace),
    ("C-M-%", Command::IsearchQueryReplaceRegexp),
    ("RET", Command::IsearchExit),
    ("C-g", Command::IsearchAbort),
];

const DIRED: &[(&str, Command)] = &[
    ("n", Command::DiredNextLine),
    ("SPC", Command::DiredNextLine),
    ("C-n", Command::DiredNextLine),
    ("<down>", Command::DiredNextLine),
    ("p", Command::DiredPreviousLine),
    ("C-p", Command::DiredPreviousLine),
    ("<up>", Command::DiredPreviousLine),
    ("RET", Command::DiredFindFile),
    ("f", Command::DiredFindFile),
    ("e", Command::DiredFindFile),
    ("^", Command::DiredUpDirectory),
    ("g", Command::RevertBuffer),
    ("s", Command::DiredCycleSort),
    ("(", Command::DiredHideDetailsMode),
    ("q", Command::QuitWindow),
    ("m", Command::DiredMark),
    ("d", Command::DiredFlagFileDeletion),
    ("u", Command::DiredUnmark),
    ("DEL", Command::DiredUnmarkBackward),
    ("U", Command::DiredUnmarkAllMarks),
    ("t", Command::DiredToggleMarks),
    ("x", Command::DiredDoFlaggedDelete),
    ("D", Command::DiredDoDelete),
    ("R", Command::DiredDoRename),
    ("C", Command::DiredDoCopy),
    ("+", Command::DiredCreateDirectory),
    ("W", Command::DiredDoOpen),
];

const SPLASH: &[(&str, Command)] = &[
    ("n", Command::ForwardButton),
    ("TAB", Command::ForwardButton),
    ("C-n", Command::ForwardButton),
    ("<down>", Command::ForwardButton),
    ("p", Command::BackwardButton),
    ("C-p", Command::BackwardButton),
    ("<up>", Command::BackwardButton),
    ("RET", Command::PushButton),
    ("q", Command::QuitWindow),
];

const DOCTOR: &[(&str, Command)] = &[("g", Command::RevertBuffer), ("q", Command::QuitWindow)];

const RUST: &[(&str, Command)] = &[
    ("C-c C-f", Command::RustFormatBuffer),
    ("C-c C-c C-k", Command::RustCheck),
    ("C-c C-c C-b", Command::RustCompile),
    ("C-c C-c C-t", Command::RustTest),
    ("C-c C-c C-r", Command::RustRun),
    ("C-c C-c C-l", Command::RustRunClippy),
    ("C-c !", Command::ListDiagnostics),
];

const R: &[(&str, Command)] = &[
    ("M--", Command::RInsertAssignment),
    ("C-S-m", Command::RInsertPipe),
    ("C-RET", Command::RSendStatement),
    ("C-c C-c", Command::RSendFunction),
    ("C-c C-z", Command::RSwitchToConsole),
    ("C-c C-v", Command::RHelp),
    ("C-c C-e", Command::REnvironment),
    ("C-c C-f", Command::RFormatBuffer),
    ("M-.", Command::RFindDefinition),
    ("TAB", Command::IndentOrComplete),
    ("C-M-i", Command::CompletionAtPoint),
];

const CONSOLE: &[(&str, Command)] = &[
    ("RET", Command::ComintSendInput),
    ("M-p", Command::ComintPreviousInput),
    ("M-n", Command::ComintNextInput),
    ("C-c C-z", Command::RSwitchToConsole),
    ("C-c C-v", Command::RHelp),
    ("C-c C-e", Command::REnvironment),
    ("M-.", Command::RFindDefinition),
    ("TAB", Command::CompletionAtPoint),
    ("C-M-i", Command::CompletionAtPoint),
];

const PREVIEW: &[(&str, Command)] = &[
    ("SPC", Command::ScrollUpCommand),
    ("DEL", Command::ScrollDownCommand),
    ("q", Command::QuitWindow),
];

const MARKDOWN: &[(&str, Command)] = &[
    ("M-RET", Command::MarkdownInsertListItem),
    ("M-<left>", Command::MarkdownPromote),
    ("C-c C--", Command::MarkdownPromote),
    ("M-<right>", Command::MarkdownDemote),
    ("C-c C-=", Command::MarkdownDemote),
    ("C-c C-c p", Command::MarkdownPreview),
];

const COMPILATION: &[(&str, Command)] = &[
    ("g", Command::Recompile),
    ("q", Command::QuitWindow),
    ("C-c C-k", Command::KillCompilation),
    ("n", Command::NextErrorNoSelect),
    ("p", Command::PreviousErrorNoSelect),
    ("M-n", Command::CompilationNextError),
    ("M-p", Command::CompilationPreviousError),
    ("RET", Command::CompileGotoError),
];

const CUSTOMIZE: &[(&str, Command)] = &[
    ("n", Command::WidgetForward),
    ("TAB", Command::WidgetForward),
    ("p", Command::WidgetBackward),
    ("S-TAB", Command::WidgetBackward),
    ("RET", Command::WidgetButtonPress),
    ("g", Command::RevertBuffer),
    ("q", Command::QuitWindow),
];

const DIAGNOSTICS: &[(&str, Command)] = &[
    ("RET", Command::GotoDiagnostic),
    ("SPC", Command::ShowDiagnostic),
    ("n", Command::NextLine),
    ("p", Command::PreviousLine),
    ("g", Command::RevertBuffer),
    ("q", Command::QuitWindow),
];

const HELP: &[(&str, Command)] = &[
    ("SPC", Command::ScrollUpCommand),
    ("DEL", Command::ScrollDownCommand),
    ("q", Command::QuitWindow),
];

const WEATHER: &[(&str, Command)] = &[("g", Command::RevertBuffer), ("q", Command::QuitWindow)];

const CHAT: &[(&str, Command)] = &[
    ("RET", Command::IrcSendInput),
    ("TAB", Command::IrcCompleteNick),
    ("C-c C-b", Command::IrcSwitch),
    ("C-c C-n", Command::IrcToggleNickList),
    ("M-p", Command::ComintPreviousInput),
    ("M-n", Command::ComintNextInput),
];

pub fn global() -> Keymap {
    build(GLOBAL)
}

pub fn minibuffer() -> Keymap {
    build(MINIBUFFER)
}

pub fn doctor() -> Keymap {
    build(DOCTOR)
}

pub fn rust() -> Keymap {
    build(RUST)
}

pub fn r() -> Keymap {
    build(R)
}

pub fn console() -> Keymap {
    build(CONSOLE)
}

pub fn preview() -> Keymap {
    build(PREVIEW)
}

pub fn markdown() -> Keymap {
    build(MARKDOWN)
}

pub fn compilation() -> Keymap {
    build(COMPILATION)
}

pub fn customize() -> Keymap {
    build(CUSTOMIZE)
}

pub fn diagnostics() -> Keymap {
    build(DIAGNOSTICS)
}

pub fn help() -> Keymap {
    build(HELP)
}

const POPUP: &[(&str, Command)] = &[
    ("C-n", Command::CompletionNext),
    ("<down>", Command::CompletionNext),
    ("C-p", Command::CompletionPrevious),
    ("<up>", Command::CompletionPrevious),
    ("TAB", Command::CompletionAccept),
    ("RET", Command::CompletionAccept),
    ("C-g", Command::CompletionQuit),
    ("ESC", Command::CompletionQuit),
];

const PLOTS: &[(&str, Command)] = &[
    ("n", Command::RPlotNext),
    ("<right>", Command::RPlotNext),
    ("p", Command::RPlotPrevious),
    ("<left>", Command::RPlotPrevious),
    ("s", Command::RPlotSave),
    ("d", Command::RPlotDelete),
    ("z", Command::RPlotZoom),
    ("v", Command::RPlotGallery),
    ("k", Command::RPlotPin),
    ("RET", Command::RPlotOpen),
    ("<up>", Command::RPlotAbove),
    ("<down>", Command::RPlotBelow),
    ("q", Command::QuitWindow),
];

const PROCESSES: &[(&str, Command)] = &[
    ("RET", Command::ProcessVisit),
    ("k", Command::ProcessStop),
    ("n", Command::NextLine),
    ("p", Command::PreviousLine),
    ("g", Command::RevertBuffer),
    ("q", Command::QuitWindow),
];

const TOOLS: &[(&str, Command)] = &[
    ("i", Command::ToolInstall),
    ("u", Command::ToolUpdate),
    ("d", Command::ToolRemove),
    ("n", Command::NextLine),
    ("p", Command::PreviousLine),
    ("g", Command::RevertBuffer),
    ("q", Command::QuitWindow),
];

const ATLAS: &[(&str, Command)] = &[
    ("C-c C-c", Command::AtlasCycleTask),
    ("C-c C-o", Command::AtlasFollowLink),
];

const ATLAS_CALENDAR: &[(&str, Command)] = &[
    ("<left>", Command::AtlasCalendarBackDay),
    ("<right>", Command::AtlasCalendarForwardDay),
    ("<up>", Command::AtlasCalendarBackWeek),
    ("<down>", Command::AtlasCalendarForwardWeek),
    ("n", Command::AtlasCalendarNext),
    ("p", Command::AtlasCalendarPrevious),
    ("w", Command::AtlasCalendarWeekView),
    (".", Command::AtlasCalendarToday),
    ("v", Command::AtlasFilterVault),
    ("RET", Command::AtlasCalendarSelect),
    ("g", Command::RevertBuffer),
    ("q", Command::QuitWindow),
];

const ATLAS_LIST: &[(&str, Command)] = &[
    ("RET", Command::AtlasVisit),
    ("t", Command::AtlasAgendaCycle),
    ("C-c C-c", Command::AtlasAgendaCycle),
    ("s", Command::AtlasAgendaPlan),
    ("d", Command::AtlasAgendaDue),
    ("v", Command::AtlasFilterVault),
    ("n", Command::NextLine),
    ("p", Command::PreviousLine),
    ("g", Command::RevertBuffer),
    ("q", Command::QuitWindow),
];

const IRC_SETUP: &[(&str, Command)] = &[
    ("TAB", Command::IrcSetupNextField),
    ("n", Command::IrcSetupNextField),
    ("S-TAB", Command::IrcSetupPreviousField),
    ("p", Command::IrcSetupPreviousField),
    ("RET", Command::IrcSetupChange),
    ("C-c C-c", Command::IrcSetupSave),
    ("C-c C-k", Command::IrcSetupCancel),
];

const ENVIRONMENT: &[(&str, Command)] = &[
    ("RET", Command::REnvironmentVisit),
    ("n", Command::NextLine),
    ("p", Command::PreviousLine),
    ("g", Command::RevertBuffer),
    ("q", Command::QuitWindow),
];

pub fn environment() -> Keymap {
    build(ENVIRONMENT)
}

const TABLE: &[(&str, Command)] = &[
    ("<right>", Command::RTableNextColumn),
    ("f", Command::RTableNextColumn),
    ("<left>", Command::RTablePreviousColumn),
    ("b", Command::RTablePreviousColumn),
    ("n", Command::NextLine),
    ("p", Command::PreviousLine),
    ("SPC", Command::ScrollUpCommand),
    ("DEL", Command::ScrollDownCommand),
    ("g", Command::RevertBuffer),
    ("q", Command::QuitWindow),
];

pub fn table() -> Keymap {
    build(TABLE)
}

pub fn processes() -> Keymap {
    build(PROCESSES)
}

pub fn tools() -> Keymap {
    build(TOOLS)
}

pub fn irc_setup() -> Keymap {
    build(IRC_SETUP)
}

pub fn atlas() -> Keymap {
    build(ATLAS)
}

pub fn atlas_calendar() -> Keymap {
    build(ATLAS_CALENDAR)
}

pub fn atlas_list() -> Keymap {
    build(ATLAS_LIST)
}

pub fn plots() -> Keymap {
    build(PLOTS)
}

pub fn popup() -> Keymap {
    build(POPUP)
}

pub fn chat() -> Keymap {
    build(CHAT)
}

pub fn weather() -> Keymap {
    build(WEATHER)
}

pub fn search() -> Keymap {
    build(SEARCH)
}

pub fn dired() -> Keymap {
    build(DIRED)
}

pub fn splash() -> Keymap {
    build(SPLASH)
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

    #[test]
    fn every_doctor_binding_resolves_to_its_command() {
        assert_resolves(&doctor(), DOCTOR);
    }

    #[test]
    fn every_rust_binding_resolves_to_its_command() {
        assert_resolves(&rust(), RUST);
    }

    #[test]
    fn every_r_binding_resolves_to_its_command() {
        assert_resolves(&r(), R);
    }

    #[test]
    fn every_console_binding_resolves_to_its_command() {
        assert_resolves(&console(), CONSOLE);
    }

    #[test]
    fn every_preview_binding_resolves_to_its_command() {
        assert_resolves(&preview(), PREVIEW);
    }

    #[test]
    fn every_markdown_binding_resolves_to_its_command() {
        assert_resolves(&markdown(), MARKDOWN);
    }

    #[test]
    fn every_compilation_binding_resolves_to_its_command() {
        assert_resolves(&compilation(), COMPILATION);
    }

    #[test]
    fn every_customize_binding_resolves_to_its_command() {
        assert_resolves(&customize(), CUSTOMIZE);
    }

    #[test]
    fn every_diagnostics_binding_resolves_to_its_command() {
        assert_resolves(&diagnostics(), DIAGNOSTICS);
    }

    #[test]
    fn every_help_binding_resolves_to_its_command() {
        assert_resolves(&help(), HELP);
    }

    #[test]
    fn every_chat_binding_resolves_to_its_command() {
        assert_resolves(&chat(), CHAT);
    }

    #[test]
    fn every_weather_binding_resolves_to_its_command() {
        assert_resolves(&weather(), WEATHER);
    }

    #[test]
    fn every_search_binding_resolves_to_its_command() {
        assert_resolves(&search(), SEARCH);
    }

    #[test]
    fn every_dired_binding_resolves_to_its_command() {
        assert_resolves(&dired(), DIRED);
    }

    #[test]
    fn every_splash_binding_resolves_to_its_command() {
        assert_resolves(&splash(), SPLASH);
    }
}
