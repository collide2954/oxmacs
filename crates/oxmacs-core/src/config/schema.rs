#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Toggle,
    Choice(&'static [&'static str]),
    Theme,
    Integer,
    Number,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setting {
    pub key: &'static str,
    pub group: &'static str,
    pub kind: Kind,
    pub unset: Option<&'static str>,
    pub hint: &'static str,
}

const fn setting(
    group: &'static str,
    key: &'static str,
    kind: Kind,
    hint: &'static str,
) -> Setting {
    Setting {
        key,
        group,
        kind,
        unset: None,
        hint,
    }
}

const fn optional(setting: Setting, unset: &'static str) -> Setting {
    Setting {
        unset: Some(unset),
        ..setting
    }
}

const APPEARANCE: &str = "Appearance";
const EDITING: &str = "Editing";
const FILES: &str = "Files";
const RUST: &str = "Rust";
const R: &str = "R";

pub const SETTINGS: &[Setting] = &[
    optional(
        setting(
            APPEARANCE,
            "theme",
            Kind::Theme,
            "Color theme; unset follows the OS light or dark preference",
        ),
        "follow OS",
    ),
    optional(
        setting(
            APPEARANCE,
            "font-family",
            Kind::Text,
            "Monospaced font name; leave empty for the bundled JetBrains Mono",
        ),
        "bundled",
    ),
    setting(APPEARANCE, "font-size", Kind::Number, "Font size in points"),
    setting(
        APPEARANCE,
        "line-numbers",
        Kind::Choice(&["absolute", "relative", "off"]),
        "Line numbers beside buffers that visit files",
    ),
    setting(
        APPEARANCE,
        "line-number-separator",
        Kind::Toggle,
        "A thin line between the line numbers and the text",
    ),
    setting(
        APPEARANCE,
        "window-mode",
        Kind::Choice(&["windowed", "maximized", "fullscreen"]),
        "How the window opens at startup",
    ),
    setting(
        APPEARANCE,
        "animations",
        Kind::Toggle,
        "Smooth scrolling, cursor glide and fades",
    ),
    setting(
        APPEARANCE,
        "cursor-blink-interval",
        Kind::Integer,
        "Cursor blink interval in milliseconds; 0 stops blinking",
    ),
    setting(
        APPEARANCE,
        "mode-line-style",
        Kind::Choice(&["classic", "modern", "minimal", "powerline", "boxed"]),
        "How the mode line under each window looks",
    ),
    setting(
        APPEARANCE,
        "clock",
        Kind::Toggle,
        "Clock at the right of the mode line",
    ),
    setting(
        APPEARANCE,
        "clock-format",
        Kind::Text,
        "strftime format for the clock, such as %H:%M or %a %H:%M",
    ),
    setting(
        APPEARANCE,
        "splash",
        Kind::Toggle,
        "Show the *oxmacs* screen when started without files",
    ),
    setting(
        EDITING,
        "tab-width",
        Kind::Integer,
        "Columns per tab character",
    ),
    setting(
        EDITING,
        "indent-tabs",
        Kind::Toggle,
        "Indent with tabs instead of spaces, unless a mode or .editorconfig chooses",
    ),
    setting(
        EDITING,
        "language-servers",
        Kind::Toggle,
        "Start a language server for files that have one installed",
    ),
    setting(
        EDITING,
        "inlay-hints",
        Kind::Toggle,
        "Types and parameter names from the language server, faint at the end of each line",
    ),
    setting(
        EDITING,
        "format-on-save",
        Kind::Toggle,
        "Format Rust with rustfmt and R with air when saving",
    ),
    setting(
        FILES,
        "on-disk-change",
        Kind::Choice(&["revert", "ask", "ignore"]),
        "What to do when a visited file changes on disk",
    ),
    setting(
        RUST,
        "rust-lint",
        Kind::Choice(&["check", "clippy", "off"]),
        "Cargo command that lints Rust projects on save",
    ),
    setting(
        FILES,
        "auto-save",
        Kind::Toggle,
        "Keep recovery copies of unsaved files in the oxmacs cache, offered back after a crash",
    ),
    setting(
        FILES,
        "auto-save-visited",
        Kind::Toggle,
        "Save modified files in place when idle",
    ),
    setting(
        FILES,
        "delete-to-trash",
        Kind::Toggle,
        "Move files and folders you delete to the system's trash instead of deleting them for good",
    ),
    optional(
        setting(
            R,
            "air-program",
            Kind::Text,
            "air executable that formats R; M-x list-tools installs one",
        ),
        "auto-detect",
    ),
];

pub fn find(key: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|setting| setting.key == key)
}
