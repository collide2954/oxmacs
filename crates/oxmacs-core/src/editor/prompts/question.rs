use std::path::{Path, PathBuf};

use crate::editor::prompts::Prompts;
use crate::editor::prompts::callbacks::Answer;
use crate::editor::{Editor, is_keyboard_quit};
use crate::external::managed::Managed;
use crate::files::{DiskState, paths};
use crate::input::{Key, KeyCode};
use crate::text::{Buffer, BufferId};

const SAVE_CHOICES: &[char] = &['y', 'n', '!', 'q'];
const YES_NO: &[char] = &['y', 'n'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::editor) enum Pending {
    SaveSome {
        remaining: Vec<PathBuf>,
        then_quit: bool,
    },
    Overwrite {
        path: PathBuf,
    },
    Kill {
        buffer: BufferId,
    },
    Revert {
        buffer: BufferId,
    },
    Reread {
        buffer: BufferId,
        disk: Option<DiskState>,
    },
    SaveOver {
        buffer: BufferId,
    },
    RemoveTool(Managed),
    Callback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    prompt: String,
    choices: &'static [char],
    typed: Option<String>,
    pending: Pending,
}

impl Question {
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn answer(&self) -> &str {
        self.typed.as_deref().unwrap_or_default()
    }
}

#[derive(Debug)]
pub(in crate::editor) enum Answering {
    NotAsked,
    Quit,
    Kept,
    Told(String),
    Resolved(Pending, char),
}

impl Prompts {
    pub(in crate::editor) fn question(&self) -> Option<&Question> {
        self.question.as_ref()
    }

    fn ask(&mut self, question: Question) {
        self.question = Some(question);
    }

    pub(in crate::editor) fn answer(&mut self, key: Key) -> Answering {
        let Some(mut question) = self.question.take() else {
            return Answering::NotAsked;
        };
        if is_keyboard_quit(key) {
            self.answer = None;
            return Answering::Quit;
        }
        if let Some(answer) = &mut question.typed {
            let mut told = None;
            match key.code() {
                KeyCode::Char(c) if key.modifiers().is_empty() => answer.push(c),
                KeyCode::Backspace => {
                    answer.pop();
                }
                KeyCode::Return if matches!(answer.as_str(), "yes" | "no") => {
                    let choice = if answer == "yes" { 'y' } else { 'n' };
                    return Answering::Resolved(question.pending, choice);
                }
                KeyCode::Return => {
                    told = Some("Please answer yes or no".to_owned());
                    answer.clear();
                }
                _ => {}
            }
            self.question = Some(question);
            return told.map_or(Answering::Kept, Answering::Told);
        }
        match key.code() {
            KeyCode::Char(c) if key.modifiers().is_empty() && question.choices.contains(&c) => {
                Answering::Resolved(question.pending, c)
            }
            _ => {
                let told = format!("Please answer {}", choice_list(question.choices));
                self.question = Some(question);
                Answering::Told(told)
            }
        }
    }
}

impl Editor {
    pub fn question(&self) -> Option<&Question> {
        self.prompts.question()
    }

    pub(in crate::editor) fn answer_question(&mut self, key: Key) -> bool {
        match self.prompts.answer(key) {
            Answering::NotAsked => return false,
            Answering::Quit => self.show_message("Quit"),
            Answering::Kept => {}
            Answering::Told(told) => self.show_message(told),
            Answering::Resolved(pending, choice) => self.resolve(pending, choice),
        }
        true
    }

    fn quit_after_saves(&mut self) {
        self.quit_requested = self.finish_formatting_saves();
    }

    pub(in crate::editor) fn save_some_buffers(&mut self, then_quit: bool) {
        let remaining: Vec<PathBuf> = self
            .buffers
            .iter()
            .filter(|buffer| buffer.is_modified())
            .filter_map(|buffer| buffer.file().map(|file| file.path.clone()))
            .collect();
        if remaining.is_empty() && !then_quit {
            self.show_message("(No files need saving)");
            return;
        }
        self.ask_next_save(remaining, then_quit);
    }

    fn ask_next_save(&mut self, remaining: Vec<PathBuf>, then_quit: bool) {
        let Some(path) = remaining.first() else {
            if then_quit {
                self.quit_after_saves();
            }
            return;
        };
        let shown = paths::abbreviate(path, self.home.as_deref());
        self.prompts.ask(Question {
            prompt: format!("Save file {shown}? (y, n, !, q) "),
            choices: SAVE_CHOICES,
            typed: None,
            pending: Pending::SaveSome {
                remaining,
                then_quit,
            },
        });
    }

    fn resolve(&mut self, pending: Pending, answer: char) {
        match pending {
            Pending::SaveSome {
                mut remaining,
                then_quit,
            } => match answer {
                'y' => {
                    let path = remaining.remove(0);
                    if self.save_file(&path) {
                        self.ask_next_save(remaining, then_quit);
                    }
                }
                'n' => {
                    remaining.remove(0);
                    self.ask_next_save(remaining, then_quit);
                }
                '!' => {
                    if remaining.iter().all(|path| self.save_file(path)) {
                        self.ask_next_save(Vec::new(), then_quit);
                    }
                }
                _ => {
                    if then_quit {
                        self.quit_after_saves();
                    }
                }
            },
            Pending::Overwrite { path } => {
                if answer == 'y' {
                    self.finish_write(path);
                }
            }
            Pending::Kill { buffer } => {
                if let (true, Some(index)) = (answer == 'y', self.index_of(buffer)) {
                    self.finish_kill(index);
                }
            }
            Pending::Revert { buffer } => {
                if let (true, Some(index)) = (answer == 'y', self.index_of(buffer)) {
                    self.revert_at(index);
                }
            }
            Pending::Reread { buffer, disk } => {
                if let Some(index) = self.index_of(buffer) {
                    if answer == 'y' {
                        self.revert_at(index);
                    } else {
                        self.note_disk_state(index, disk, true);
                    }
                }
                self.check_disk_changes();
            }
            Pending::SaveOver { buffer } => {
                if let (true, Some(index)) = (answer == 'y', self.index_of(buffer)) {
                    self.save_buffer_at(index);
                }
            }
            Pending::RemoveTool(tool) => {
                if answer == 'y' {
                    self.finish_tool_removal(tool);
                }
            }
            Pending::Callback => {
                if let Some(then) = self.prompts.answer.take() {
                    then.give(self, answer);
                }
            }
        }
    }

    fn id_at(&self, index: usize) -> Option<BufferId> {
        self.buffers.get(index).map(Buffer::id)
    }

    fn index_of(&self, buffer: BufferId) -> Option<usize> {
        self.buffers.position(|known| known.id() == buffer)
    }

    pub(in crate::editor) fn ask_to_save_over(&mut self, name: &str) {
        let Some(buffer) = self.id_at(0) else {
            return;
        };
        self.prompts.ask(Question {
            prompt: format!("{name} changed on disk; save anyway? (y or n) "),
            choices: YES_NO,
            typed: None,
            pending: Pending::SaveOver { buffer },
        });
    }

    pub(in crate::editor) fn ask_to_reread(
        &mut self,
        index: usize,
        name: &str,
        disk: Option<DiskState>,
    ) {
        let Some(buffer) = self.id_at(index) else {
            return;
        };
        self.prompts.ask(Question {
            prompt: format!("{name} changed on disk; reread it? (y or n) "),
            choices: YES_NO,
            typed: None,
            pending: Pending::Reread { buffer, disk },
        });
    }

    pub(in crate::editor) fn ask_to_revert(&mut self, index: usize, path: &Path) {
        let Some(buffer) = self.id_at(index) else {
            return;
        };
        let shown = paths::abbreviate(path, self.home.as_deref());
        self.prompts.ask(Question {
            prompt: format!("Revert buffer from file {shown}? (y or n) "),
            choices: YES_NO,
            typed: None,
            pending: Pending::Revert { buffer },
        });
    }

    pub(in crate::editor) fn ask_for_callback(
        &mut self,
        prompt: &str,
        shown: &str,
        choices: &'static [char],
        typed: bool,
        then: Answer,
    ) {
        self.prompts.answer = Some(then);
        self.prompts.ask(Question {
            prompt: format!("{prompt} ({shown}) "),
            choices,
            typed: typed.then(String::new),
            pending: Pending::Callback,
        });
    }

    pub(in crate::editor) fn ask_to_remove_tool(&mut self, tool: Managed, version: &str) {
        self.prompts.ask(Question {
            prompt: format!("Remove {} {version}? (y or n) ", tool.name),
            choices: YES_NO,
            typed: None,
            pending: Pending::RemoveTool(tool),
        });
    }

    pub(in crate::editor) fn ask_to_kill(&mut self, index: usize, name: &str) {
        let Some(buffer) = self.id_at(index) else {
            return;
        };
        self.prompts.ask(Question {
            prompt: format!("Buffer {name} modified; kill anyway? (y or n) "),
            choices: YES_NO,
            typed: None,
            pending: Pending::Kill { buffer },
        });
    }

    pub(in crate::editor) fn ask_to_overwrite(&mut self, path: PathBuf) {
        let shown = paths::abbreviate(&path, self.home.as_deref());
        self.prompts.ask(Question {
            prompt: format!("File {shown} exists; overwrite? (y or n) "),
            choices: YES_NO,
            typed: None,
            pending: Pending::Overwrite { path },
        });
    }

    fn save_file(&mut self, path: &Path) -> bool {
        match self.index_visiting(path) {
            Some(index) => self.save_buffer_at(index),
            None => true,
        }
    }
}

fn choice_list(choices: &[char]) -> String {
    let names: Vec<String> = choices.iter().map(char::to_string).collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{}, or {last}", rest.join(", ")),
        _ => names.concat(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{Answering, Pending, Question, YES_NO};
    use crate::editor::Editor;
    use crate::editor::prompts::Prompts;
    use crate::input::{Key, KeyCode, Modifiers};
    use crate::testing::{press, question, type_text};

    fn key(character: char) -> Key {
        Key::new(KeyCode::Char(character), Modifiers::empty())
    }

    #[test]
    fn prompts_answer_questions_and_hand_back_the_choice() {
        let mut prompts = Prompts::default();
        prompts.ask(Question {
            prompt: "Go? (y or n) ".to_owned(),
            choices: YES_NO,
            typed: None,
            pending: Pending::Callback,
        });

        assert!(
            matches!(prompts.answer(key('x')), Answering::Told(told) if told.starts_with("Please answer y"))
        );
        assert!(prompts.question().is_some());
        assert!(matches!(
            prompts.answer(key('n')),
            Answering::Resolved(Pending::Callback, 'n')
        ));
        assert!(prompts.question().is_none());
        assert!(matches!(prompts.answer(key('y')), Answering::NotAsked));
    }

    #[test]
    fn typed_answers_wait_for_yes_or_no_and_quit_forgets_the_question() {
        let mut prompts = Prompts::default();
        prompts.ask(Question {
            prompt: "Go? (yes or no) ".to_owned(),
            choices: YES_NO,
            typed: Some(String::new()),
            pending: Pending::Callback,
        });

        assert!(matches!(prompts.answer(key('y')), Answering::Kept));
        assert_eq!(prompts.question().map(Question::answer), Some("y"));
        assert!(matches!(
            prompts.answer(Key::new(KeyCode::Char('g'), Modifiers::CONTROL)),
            Answering::Quit
        ));
        assert!(prompts.question().is_none());
    }

    fn two_modified_files() -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        fs::write(dir.path().join("b.txt"), "b").unwrap();
        let mut editor = Editor::new();
        editor.set_home_directory(None);
        for name in ["a.txt", "b.txt"] {
            editor.open_file(&dir.path().join(name));
            type_text(&mut editor, "x");
        }
        (dir, editor)
    }

    fn on_disk(dir: &tempfile::TempDir, name: &str) -> String {
        fs::read_to_string(dir.path().join(name)).unwrap()
    }

    fn save_prompt(dir: &tempfile::TempDir, name: &str) -> String {
        format!(
            "Save file {}? (y, n, !, q) ",
            Path::new(dir.path()).join(name).display()
        )
    }

    #[test]
    fn quitting_asks_about_each_modified_file() {
        let (dir, mut editor) = two_modified_files();

        press(&mut editor, "C-x C-c");
        assert_eq!(question(&editor), Some(save_prompt(&dir, "b.txt")));

        press(&mut editor, "y");
        assert_eq!(on_disk(&dir, "b.txt"), "xb");
        assert_eq!(question(&editor), Some(save_prompt(&dir, "a.txt")));

        press(&mut editor, "n");
        assert_eq!(on_disk(&dir, "a.txt"), "a");
        assert!(editor.should_quit());
    }

    #[test]
    fn kill_oxmacs_quits_at_once_without_saving() {
        let (dir, mut editor) = two_modified_files();

        press(&mut editor, "M-x");
        type_text(&mut editor, "kill-oxmacs");
        press(&mut editor, "RET");

        assert!(editor.should_quit());
        assert_eq!(question(&editor), None);
        assert_eq!(on_disk(&dir, "a.txt"), "a");
    }

    #[test]
    fn bang_saves_every_remaining_file() {
        let (dir, mut editor) = two_modified_files();

        press(&mut editor, "C-x C-c !");

        assert_eq!(on_disk(&dir, "a.txt"), "xa");
        assert_eq!(on_disk(&dir, "b.txt"), "xb");
        assert!(editor.should_quit());
    }

    #[test]
    fn q_quits_without_saving_the_rest() {
        let (dir, mut editor) = two_modified_files();

        press(&mut editor, "C-x C-c q");

        assert_eq!(on_disk(&dir, "b.txt"), "b");
        assert!(editor.should_quit());
    }

    #[test]
    fn save_some_buffers_saves_without_quitting() {
        let (dir, mut editor) = two_modified_files();

        press(&mut editor, "C-x s y y");

        assert_eq!(on_disk(&dir, "a.txt"), "xa");
        assert_eq!(on_disk(&dir, "b.txt"), "xb");
        assert!(!editor.should_quit());
        assert_eq!(question(&editor), None);
    }

    #[test]
    fn save_some_buffers_reports_when_nothing_needs_saving() {
        let mut editor = Editor::new();

        press(&mut editor, "C-x s");

        assert_eq!(editor.message(), Some("(No files need saving)"));
    }

    #[test]
    fn other_keys_ask_again() {
        let (dir, mut editor) = two_modified_files();

        press(&mut editor, "C-x s z");

        assert_eq!(editor.message(), Some("Please answer y, n, !, or q"));
        assert_eq!(question(&editor), Some(save_prompt(&dir, "b.txt")));
    }

    #[test]
    fn keyboard_quit_cancels_the_question() {
        let (_dir, mut editor) = two_modified_files();

        press(&mut editor, "C-x C-c C-g");

        assert_eq!(question(&editor), None);
        assert!(!editor.should_quit());
        assert_eq!(editor.message(), Some("Quit"));
    }
}
