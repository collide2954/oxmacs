use std::path::PathBuf;

use strum::VariantArray;

use crate::command::{Command, Scope};
use crate::editor::Editor;
use crate::editor::transfer::Kind;
use crate::files::paths::DirEntry;
use crate::text::Buffer;

const LABEL_LIMIT: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Purpose {
    RunCommand,
    PasteFromHistory,
    FindFile,
    WriteFile,
    SwitchBuffer,
    KillBuffer,
    Dired,
    Transfer(Kind),
    CreateDirectory,
}

impl Purpose {
    pub(super) fn takes_path(self) -> bool {
        matches!(
            self,
            Self::FindFile
                | Self::WriteFile
                | Self::Dired
                | Self::Transfer(_)
                | Self::CreateDirectory
        )
    }

    fn lists_only_directories(self) -> bool {
        matches!(self, Self::Dired | Self::CreateDirectory)
    }

    fn takes_buffer(self) -> bool {
        matches!(self, Self::SwitchBuffer | Self::KillBuffer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Listing {
    pub(super) directory: PathBuf,
    pub(super) entries: Vec<DirEntry>,
}

#[derive(Debug)]
pub struct Minibuffer {
    prompt: String,
    pub(super) purpose: Purpose,
    pub(super) input: Buffer,
    pub(super) note: Option<String>,
    pub(super) selected: usize,
    pub(super) listing: Option<Listing>,
    pub(super) sources: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Command(Command),
    Clip(usize),
    Entry(usize),
    Buffer(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub label: String,
    pub annotation: Option<String>,
    pub choice: Choice,
}

impl Minibuffer {
    fn new(prompt: impl Into<String>, purpose: Purpose) -> Self {
        Self {
            prompt: prompt.into(),
            purpose,
            input: Buffer::new(" *Minibuf*"),
            note: None,
            selected: 0,
            listing: None,
            sources: Vec::new(),
        }
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn input(&self) -> &Buffer {
        &self.input
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }
}

impl Editor {
    pub fn minibuffer(&self) -> Option<&Minibuffer> {
        self.minibuffer.as_ref()
    }

    pub fn completions(&self) -> Vec<Completion> {
        let Some(minibuffer) = &self.minibuffer else {
            return Vec::new();
        };
        let input = minibuffer.input.contents().to_lowercase();
        let matches = |label: &str| {
            let label = label.to_lowercase();
            input.split_whitespace().all(|term| label.contains(term))
        };
        match minibuffer.purpose {
            Purpose::RunCommand => self.command_completions(matches),
            Purpose::PasteFromHistory => self
                .clipboard_history
                .iter()
                .enumerate()
                .map(|(index, text)| Completion {
                    label: clip_label(text),
                    annotation: None,
                    choice: Choice::Clip(index),
                })
                .filter(|completion| matches(&completion.label))
                .collect(),
            Purpose::FindFile
            | Purpose::WriteFile
            | Purpose::Dired
            | Purpose::Transfer(_)
            | Purpose::CreateDirectory => file_completions(minibuffer),
            Purpose::SwitchBuffer | Purpose::KillBuffer => {
                self.buffer_completions(minibuffer.purpose, matches)
            }
        }
    }

    fn command_completions(&self, matches: impl Fn(&str) -> bool) -> Vec<Completion> {
        let mut commands: Vec<Command> = Command::VARIANTS
            .iter()
            .copied()
            .filter(|command| {
                let scope = command.scope();
                (scope == Scope::Global || scope == self.buffer_scope()) && matches(command.name())
            })
            .collect();
        commands.sort_by_key(|command| (self.recency(*command), command.name()));
        commands
            .into_iter()
            .map(|command| Completion {
                label: command.name().to_owned(),
                annotation: self.where_is(command).map(|keys| keys.to_string()),
                choice: Choice::Command(command),
            })
            .collect()
    }

    fn recency(&self, command: Command) -> usize {
        self.command_history
            .iter()
            .rev()
            .position(|used| *used == command)
            .unwrap_or(usize::MAX)
    }

    pub(super) fn selected_completion(&self) -> Option<Completion> {
        let selected = self.minibuffer.as_ref()?.selected;
        self.completions().into_iter().nth(selected)
    }

    pub(super) fn open_minibuffer(&mut self, prompt: &str, purpose: Purpose) -> bool {
        if self.minibuffer.is_some() {
            self.show_message("Command attempted to use minibuffer while in minibuffer");
            return false;
        }
        self.minibuffer = Some(Minibuffer::new(prompt, purpose));
        true
    }

    pub(super) fn set_minibuffer_input(&mut self, text: &str) {
        if let Some(minibuffer) = &mut self.minibuffer {
            let input = &mut minibuffer.input;
            input.delete_range(0..input.len_chars());
            input.insert(text);
        }
    }

    fn minibuffer_purpose(&self) -> Option<Purpose> {
        self.minibuffer
            .as_ref()
            .map(|minibuffer| minibuffer.purpose)
    }

    pub(super) fn execute_extended_command(&mut self) {
        self.open_minibuffer("M-x ", Purpose::RunCommand);
    }

    pub(super) fn paste_from_history(&mut self) {
        if self.clipboard_history.is_empty() {
            self.show_message("Clipboard history is empty");
            return;
        }
        self.open_minibuffer("Paste from history: ", Purpose::PasteFromHistory);
    }

    pub(super) fn exit_minibuffer_with_input(&mut self) {
        let input = self.minibuffer_input();
        match self.minibuffer_purpose() {
            Some(purpose) if purpose.takes_path() => self.exit_file_prompt(true),
            Some(purpose) if purpose.takes_buffer() && !input.is_empty() => {
                self.exit_buffer_prompt(purpose, &input);
            }
            _ => self.exit_minibuffer(),
        }
    }

    pub(super) fn minibuffer_input(&self) -> String {
        self.minibuffer
            .as_ref()
            .map(|minibuffer| minibuffer.input.contents())
            .unwrap_or_default()
    }

    pub(super) fn exit_minibuffer(&mut self) {
        let Some(purpose) = self.minibuffer_purpose() else {
            return;
        };
        if purpose.takes_path() {
            self.exit_file_prompt(false);
            return;
        }
        let Some(completion) = self.selected_completion() else {
            if purpose.takes_buffer() {
                let input = self.minibuffer_input();
                self.exit_buffer_prompt(purpose, &input);
            } else {
                self.note_no_match();
            }
            return;
        };
        self.minibuffer = None;
        match completion.choice {
            Choice::Command(command) => {
                self.command_history.retain(|used| *used != command);
                self.command_history.push(command);
                self.execute(command);
            }
            Choice::Clip(index) => {
                let text = self.clipboard_history.get(index).cloned();
                self.paste(text);
            }
            Choice::Buffer(index) => self.choose_buffer(purpose, index),
            Choice::Entry(_) => {}
        }
    }

    pub(super) fn note_no_match(&mut self) {
        if let Some(minibuffer) = &mut self.minibuffer {
            minibuffer.note = Some("[No match]".to_owned());
        }
    }

    pub(super) fn minibuffer_next(&mut self) {
        let last = self.completions().len().saturating_sub(1);
        if let Some(minibuffer) = &mut self.minibuffer {
            minibuffer.selected = (minibuffer.selected + 1).min(last);
        }
    }

    pub(super) fn minibuffer_previous(&mut self) {
        if let Some(minibuffer) = &mut self.minibuffer {
            minibuffer.selected = minibuffer.selected.saturating_sub(1);
        }
    }

    pub(super) fn minibuffer_complete(&mut self) {
        if self.minibuffer_purpose().is_some_and(Purpose::takes_path) {
            self.complete_file_name();
            return;
        }
        if let Some(completion) = self.selected_completion() {
            self.set_minibuffer_input(&completion.label);
        }
    }

    pub(super) fn reset_selection(&mut self, command: Command) {
        if let Some(minibuffer) = &mut self.minibuffer
            && !matches!(
                command,
                Command::MinibufferNext | Command::MinibufferPrevious
            )
        {
            minibuffer.selected = 0;
        }
    }
}

fn file_completions(minibuffer: &Minibuffer) -> Vec<Completion> {
    let Some(listing) = &minibuffer.listing else {
        return Vec::new();
    };
    let input = minibuffer.input.contents();
    let (_, name) = crate::files::paths::split(&input);
    let typed = name.to_lowercase();
    let mut completions: Vec<(bool, Completion)> = listing
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.is_dir || !minibuffer.purpose.lists_only_directories())
        .filter(|(_, entry)| {
            let lower = entry.name.to_lowercase();
            typed.split_whitespace().all(|term| lower.contains(term))
        })
        .map(|(index, entry)| {
            let mut label = entry.name.clone();
            if entry.is_dir {
                label.push(crate::files::paths::SEPARATOR);
            }
            let completion = Completion {
                label,
                annotation: None,
                choice: Choice::Entry(index),
            };
            (entry.name != name, completion)
        })
        .collect();
    completions.sort_by_key(|(inexact, _)| *inexact);
    completions
        .into_iter()
        .map(|(_, completion)| completion)
        .collect()
}

fn clip_label(text: &str) -> String {
    let line: String = text.replace('\n', "↵");
    if line.chars().count() <= LABEL_LIMIT {
        return line;
    }
    let mut short: String = line.chars().take(LABEL_LIMIT - 1).collect();
    short.push('…');
    short
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, labels, minibuffer_input, press, type_text};

    #[test]
    fn m_x_opens_the_minibuffer_with_a_prompt() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");

        let minibuffer = editor.minibuffer().unwrap();
        assert_eq!(minibuffer.prompt(), "M-x ");
        assert_eq!(minibuffer.input().contents(), "");
    }

    #[test]
    fn typing_goes_to_the_minibuffer_not_the_buffer() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "end");

        assert_eq!(minibuffer_input(&editor), "end");
        assert_eq!(editor.buffer().contents(), "");
    }

    #[test]
    fn editing_keys_work_in_the_minibuffer() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "abc");
        press(&mut editor, "DEL C-a");

        assert_eq!(minibuffer_input(&editor), "ab");
        assert_eq!(editor.minibuffer().unwrap().input().point(), 0);
    }

    #[test]
    fn return_runs_the_named_command_in_the_buffer() {
        let mut editor = editor_with("hello", 0);

        press(&mut editor, "M-x");
        type_text(&mut editor, "end-of-buffer");
        press(&mut editor, "RET");

        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.buffer().point(), 5);
    }

    #[test]
    fn c_j_also_runs_the_command() {
        let mut editor = editor_with("hello", 0);

        press(&mut editor, "M-x");
        type_text(&mut editor, "end-of-buffer");
        press(&mut editor, "C-j");

        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.buffer().point(), 5);
    }

    #[test]
    fn unknown_commands_keep_the_minibuffer_open() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "nope");
        press(&mut editor, "RET");

        let minibuffer = editor.minibuffer().unwrap();
        assert_eq!(minibuffer.input().contents(), "nope");
        assert_eq!(minibuffer.note(), Some("[No match]"));
    }

    #[test]
    fn the_next_key_clears_the_note() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "nope");
        press(&mut editor, "RET DEL");

        assert_eq!(editor.minibuffer().unwrap().note(), None);
    }

    #[test]
    fn keyboard_quit_closes_the_minibuffer() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x a C-g");

        assert!(editor.minibuffer().is_none());
        assert_eq!(editor.message(), Some("Quit"));
    }

    #[test]
    fn m_x_inside_the_minibuffer_is_refused() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x a M-x");

        assert_eq!(minibuffer_input(&editor), "a");
        assert_eq!(
            editor.message(),
            Some("Command attempted to use minibuffer while in minibuffer")
        );
    }

    fn open_with(editor: &mut Editor, text: &str) {
        press(editor, "M-x");
        type_text(editor, text);
    }

    #[test]
    fn empty_input_lists_commands_alphabetically() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");

        let names = labels(&editor);
        assert!(names.is_sorted(), "{names:?}");
        assert!(names.iter().any(|name| name == "forward-char"));
        assert!(!names.iter().any(|name| name == "exit-minibuffer"));
        assert!(!names.iter().any(|name| name == "minibuffer-next"));
    }

    #[test]
    fn every_space_separated_term_must_match() {
        let mut editor = Editor::new();

        open_with(&mut editor, "buf end");

        assert_eq!(labels(&editor), ["end-of-buffer"]);
    }

    #[test]
    fn matching_ignores_case() {
        let mut editor = Editor::new();

        open_with(&mut editor, "END-OF-B");

        assert_eq!(labels(&editor), ["end-of-buffer"]);
    }

    #[test]
    fn recently_used_commands_come_first() {
        let mut editor = Editor::new();
        open_with(&mut editor, "end-of-buffer");
        press(&mut editor, "RET");

        press(&mut editor, "M-x");

        let names = labels(&editor);
        assert_eq!(names[0], "end-of-buffer");
        assert!(names[1..].is_sorted(), "{names:?}");
    }

    #[test]
    fn completions_show_key_bindings() {
        let mut editor = Editor::new();
        open_with(&mut editor, "whole");

        let completions = editor.completions();

        assert_eq!(completions[0].annotation.as_deref(), Some("C-x h"));
    }

    #[test]
    fn next_and_previous_move_the_selection_within_bounds() {
        let mut editor = Editor::new();
        open_with(&mut editor, "forward-");
        let selected = |editor: &Editor| editor.minibuffer().unwrap().selected();

        press(&mut editor, "C-n");
        assert_eq!(selected(&editor), 1);
        press(&mut editor, "<down>");
        assert_eq!(selected(&editor), 1);
        press(&mut editor, "C-p");
        assert_eq!(selected(&editor), 0);
        press(&mut editor, "<up>");
        assert_eq!(selected(&editor), 0);
    }

    #[test]
    fn return_runs_the_selected_candidate() {
        let mut editor = editor_with("alpha beta", 0);
        open_with(&mut editor, "forward-");

        press(&mut editor, "C-n RET");

        assert_eq!(editor.buffer().point(), 5);
    }

    #[test]
    fn editing_the_input_resets_the_selection() {
        let mut editor = Editor::new();
        open_with(&mut editor, "word");

        press(&mut editor, "C-n DEL");

        assert_eq!(editor.minibuffer().unwrap().selected(), 0);
    }

    #[test]
    fn tab_inserts_the_selected_candidate() {
        let mut editor = Editor::new();
        open_with(&mut editor, "buf end");

        press(&mut editor, "TAB");

        let minibuffer = editor.minibuffer().unwrap();
        assert_eq!(minibuffer.input().contents(), "end-of-buffer");
        assert_eq!(minibuffer.input().point(), 13);
    }
}
