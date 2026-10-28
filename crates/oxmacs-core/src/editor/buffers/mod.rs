mod access;
pub(super) mod auto_save;
pub(super) mod disk;
pub(super) mod files;
pub(super) mod modes;
pub(super) mod read_only;

use std::path::Path;

use crate::display::{ScreenLine, Viewport};
use crate::editor::prompts::minibuffer::Purpose;
use crate::editor::special::splash::Splash;
use crate::editor::{Choice, Completion, Editor, Event, EventKind};
use crate::files::paths;
use crate::special_buffer::{self, SpecialBuffer};
use crate::syntax::{Language, Syntax};
use crate::text::Buffer;

pub(super) const SCRATCH: &str = "*scratch*";

#[derive(Debug)]
pub(super) enum Mode {
    Text(Option<Syntax>),
    Special(Box<dyn SpecialBuffer>),
}

impl Mode {
    pub(super) fn special(view: impl SpecialBuffer) -> Self {
        Self::Special(Box::new(view))
    }

    pub(super) fn view<T: SpecialBuffer>(&self) -> Option<&T> {
        match self {
            Self::Special(view) => special_buffer::downcast(view.as_ref()),
            Self::Text(_) => None,
        }
    }

    pub(super) fn view_mut<T: SpecialBuffer>(&mut self) -> Option<&mut T> {
        match self {
            Self::Special(view) => special_buffer::downcast_mut(view.as_mut()),
            Self::Text(_) => None,
        }
    }
}

#[derive(Debug)]
struct Entry {
    buffer: Buffer,
    top: ScreenLine,
    mode: Mode,
    numbers: Option<bool>,
}

#[derive(Debug)]
pub(super) struct BufferList {
    entries: Vec<Entry>,
    scratch: Option<(Language, Vec<Language>)>,
}

impl BufferList {
    pub(super) fn new(buffer: Buffer) -> Self {
        Self {
            entries: vec![Entry {
                buffer,
                top: ScreenLine::default(),
                mode: Mode::Text(None),
                numbers: None,
            }],
            scratch: None,
        }
    }

    pub(super) fn set_scratch_language(
        &mut self,
        language: Option<Language>,
        known: Vec<Language>,
    ) {
        self.scratch = language.map(|language| (language, known));
    }

    pub(super) fn mode(&self, index: usize) -> Option<&Mode> {
        self.entries.get(index).map(|entry| &entry.mode)
    }

    pub(super) fn find_mode(&self, matches: impl Fn(&Mode) -> bool) -> Option<usize> {
        self.entries.iter().position(|entry| matches(&entry.mode))
    }

    pub(super) fn entry_mut(&mut self, index: usize) -> Option<(&mut Buffer, &mut Mode)> {
        self.entries
            .get_mut(index)
            .map(|entry| (&mut entry.buffer, &mut entry.mode))
    }

    pub(super) fn special<T: SpecialBuffer>(&self, index: usize) -> Option<&T> {
        self.mode(index)?.view()
    }

    pub(super) fn special_mut<T: SpecialBuffer>(&mut self, index: usize) -> Option<&mut T> {
        self.mode_mut(index)?.view_mut()
    }

    pub(super) fn entry_special_mut<T: SpecialBuffer>(
        &mut self,
        index: usize,
    ) -> Option<(&mut Buffer, &mut T)> {
        let (buffer, mode) = self.entry_mut(index)?;
        Some((buffer, mode.view_mut()?))
    }

    pub(super) fn find_special<T: SpecialBuffer>(
        &self,
        matches: impl Fn(&T) -> bool,
    ) -> Option<usize> {
        self.find_mode(|mode| mode.view::<T>().is_some_and(&matches))
    }

    pub(super) fn mode_mut(&mut self, index: usize) -> Option<&mut Mode> {
        self.entries.get_mut(index).map(|entry| &mut entry.mode)
    }

    pub(super) fn numbers(&self, index: usize) -> Option<bool> {
        self.entries.get(index).and_then(|entry| entry.numbers)
    }

    pub(super) fn set_numbers(&mut self, index: usize, shown: bool) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.numbers = Some(shown);
        }
    }

    pub(super) fn is_special(&self, index: usize) -> bool {
        self.mode(index)
            .is_some_and(|mode| !matches!(mode, Mode::Text(_)))
    }

    pub(super) fn splash(&self, index: usize) -> Option<&Splash> {
        self.special(index)
    }

    pub(super) fn location(&self, index: usize) -> Option<(&Path, bool)> {
        let entry = self.entries.get(index)?;
        match &entry.mode {
            Mode::Special(view) => view
                .directory()
                .map(|directory| (directory, true))
                .or_else(|| view.file().map(|file| (file, false))),
            Mode::Text(_) => entry.buffer.file().map(|file| (file.path.as_path(), false)),
        }
    }

    pub(super) fn fixed_name(&self, index: usize) -> bool {
        matches!(self.mode(index), Some(Mode::Special(view)) if view.fixed_name())
    }

    pub(super) fn watched(&self, index: usize) -> Option<&Path> {
        match self.mode(index)? {
            Mode::Special(view) => view.watched(),
            Mode::Text(_) => None,
        }
    }

    pub(super) fn language(&self, index: usize) -> Option<Language> {
        match self.mode(index)? {
            Mode::Text(syntax) => syntax.as_ref().map(Syntax::language),
            Mode::Special(_) => None,
        }
    }

    pub(super) fn syntax(&self, index: usize) -> Option<&Syntax> {
        match self.mode(index)? {
            Mode::Text(syntax) => syntax.as_ref().filter(|syntax| syntax.parses()),
            Mode::Special(_) => None,
        }
    }

    pub(super) fn sync_syntax(&mut self, index: usize) {
        if let Some(entry) = self.entries.get_mut(index)
            && let Mode::Text(Some(syntax)) = &mut entry.mode
        {
            syntax.sync(&mut entry.buffer);
        }
    }

    pub(super) fn sync_syntax_lazily(&mut self, index: usize) {
        if let Some(entry) = self.entries.get_mut(index)
            && let Mode::Text(Some(syntax)) = &mut entry.mode
        {
            syntax.sync_lazily(&mut entry.buffer);
        }
    }

    pub(super) fn syntax_stale(&self, index: usize) -> bool {
        self.syntax(index).is_some_and(Syntax::stale)
    }

    pub(super) fn set_language(
        &mut self,
        index: usize,
        language: Option<Language>,
        known: &[Language],
    ) {
        if let Some(entry) = self.entries.get_mut(index)
            && let Mode::Text(syntax) = &mut entry.mode
            && syntax.as_ref().map(Syntax::language) != language
        {
            *syntax = language.map(|language| Syntax::knowing(language, known));
        }
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn current(&self) -> &Buffer {
        &self.entries[0].buffer
    }

    pub(super) fn current_mut(&mut self) -> &mut Buffer {
        &mut self.entries[0].buffer
    }

    pub(super) fn top(&self) -> ScreenLine {
        self.entries[0].top
    }

    pub(super) fn set_top(&mut self, top: ScreenLine) {
        self.entries[0].top = top;
    }

    pub(super) fn get(&self, index: usize) -> Option<&Buffer> {
        self.entries.get(index).map(|entry| &entry.buffer)
    }

    pub(super) fn get_mut(&mut self, index: usize) -> Option<&mut Buffer> {
        self.entries.get_mut(index).map(|entry| &mut entry.buffer)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &Buffer> {
        self.entries.iter().map(|entry| &entry.buffer)
    }

    pub(super) fn position(&self, predicate: impl Fn(&Buffer) -> bool) -> Option<usize> {
        self.iter().position(predicate)
    }

    pub(super) fn push_front_with(&mut self, buffer: Buffer, mode: Mode) {
        self.entries.insert(
            0,
            Entry {
                buffer,
                top: ScreenLine::default(),
                mode,
                numbers: None,
            },
        );
    }

    pub(super) fn push_back_with(&mut self, buffer: Buffer, mode: Mode) -> usize {
        self.entries.push(Entry {
            buffer,
            top: ScreenLine::default(),
            mode,
            numbers: None,
        });
        self.entries.len() - 1
    }

    pub(super) fn set_mode(&mut self, index: usize, mode: Mode) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.mode = mode;
        }
    }

    pub(super) fn select(&mut self, index: usize) {
        if index < self.entries.len() {
            let entry = self.entries.remove(index);
            self.entries.insert(0, entry);
        }
    }

    pub(super) fn bury_current(&mut self) {
        self.entries.rotate_left(1);
    }

    pub(super) fn remove(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
        }
        if self.entries.is_empty() {
            let syntax = self
                .scratch
                .as_ref()
                .map(|(language, known)| Syntax::knowing(*language, known));
            self.push_front_with(Buffer::new(SCRATCH), Mode::Text(syntax));
        }
    }
}

impl Editor {
    pub(super) fn switch_to_buffer(&mut self) {
        self.open_minibuffer("Switch to buffer: ", Purpose::SwitchBuffer);
    }

    pub(super) fn kill_buffer(&mut self) {
        self.open_minibuffer("Kill buffer: ", Purpose::KillBuffer);
    }

    pub(super) fn exit_buffer_prompt(&mut self, purpose: Purpose, name: &str) {
        match self.buffers.position(|buffer| buffer.name() == name) {
            Some(index) => self.choose_buffer(purpose, index),
            None if purpose == Purpose::SwitchBuffer => {
                self.prompts.done_reading();
                self.add_buffer(Buffer::new(name));
            }
            None => self.prompts.note_no_match(),
        }
    }

    pub(super) fn choose_buffer(&mut self, purpose: Purpose, index: usize) {
        self.prompts.done_reading();
        if purpose == Purpose::KillBuffer {
            self.kill_buffer_at(index);
        } else {
            self.select_buffer(index);
        }
    }

    fn kill_buffer_at(&mut self, index: usize) {
        let Some(buffer) = self.buffers.get(index) else {
            return;
        };
        if buffer.is_modified() && buffer.file().is_some() {
            let name = buffer.name().to_owned();
            self.ask_to_kill(index, &name);
        } else {
            self.finish_kill(index);
        }
    }

    pub(super) fn finish_kill(&mut self, index: usize) {
        if let Some(id) = self.buffers.get(index).map(Buffer::id) {
            self.record(Event::Killed(id));
            self.undecorate(|decoration, _| decoration.buffer == id);
        }
        if let Some(path) = self
            .buffers
            .get(index)
            .and_then(|buffer| buffer.file())
            .map(|file| file.path.clone())
        {
            self.forget_recovery(&path);
        }
        self.buffers.remove(index);
        self.rehome_windows();
        self.name_file_buffers();
        if index == 0 {
            self.after_buffer_switch();
        }
    }

    pub(super) fn buffer_completions(
        &self,
        purpose: Purpose,
        matches: impl Fn(&str) -> bool,
    ) -> Vec<Completion> {
        let count = self.buffers.len();
        let start = usize::from(purpose == Purpose::SwitchBuffer);
        (start..start + count)
            .map(|offset| offset % count)
            .filter_map(|index| {
                let buffer = self.buffers.get(index)?;
                matches(buffer.name()).then(|| Completion {
                    label: buffer.name().to_owned(),
                    annotation: self
                        .buffers
                        .location(index)
                        .map(|(path, _)| paths::abbreviate(path, self.home.as_deref())),
                    summary: None,
                    choice: Choice::Buffer(index),
                })
            })
            .collect()
    }

    pub(super) fn quit_window(&mut self) {
        self.buffers.set_top(self.viewport.top());
        self.buffers.bury_current();
        self.after_buffer_switch();
    }

    pub(super) fn select_buffer(&mut self, index: usize) {
        self.buffers.set_top(self.viewport.top());
        self.buffers.select(index);
        self.after_buffer_switch();
    }

    pub(super) fn add_buffer(&mut self, buffer: Buffer) {
        let language = buffer.file().and_then(|file| self.language_for(&file.path));
        let known = self.languages();
        self.add_buffer_with(
            buffer,
            Mode::Text(language.map(|language| Syntax::knowing(language, &known))),
        );
    }

    pub(super) fn add_buffer_with(&mut self, mut buffer: Buffer, mode: Mode) {
        let text = matches!(mode, Mode::Text(_));
        if text && self.subscribed(EventKind::Edited) {
            buffer.track_splices();
        }
        let id = buffer.id();
        self.buffers.set_top(self.viewport.top());
        self.buffers.push_front_with(buffer, mode);
        self.name_file_buffers();
        self.after_buffer_switch();
        if text {
            self.record(Event::Opened(id));
        }
    }

    pub(super) fn after_buffer_switch(&mut self) {
        self.viewport = Viewport::new(self.viewport.rows(), self.viewport.columns());
        self.viewport.set_top(self.buffers.top());
        self.reveal_point();
        self.goal_column = None;
        self.shift_selection = false;
        self.last_command = None;
        self.last_id = None;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::editor::Editor;
    use crate::testing::{labels, press, type_text};
    use crate::text::Buffer;

    fn editor_with_buffers(names: &[&str]) -> Editor {
        let (last, rest) = names.split_last().unwrap();
        let mut editor = Editor::with_buffer(Buffer::new(*last));
        for name in rest.iter().rev() {
            editor
                .buffers
                .push_front_with(Buffer::new(*name), super::Mode::Text(None));
        }
        editor
    }

    #[test]
    fn switching_lists_other_buffers_by_recency_then_the_current_one() {
        let mut editor = editor_with_buffers(&["c", "b", "a"]);

        press(&mut editor, "C-x b");

        assert_eq!(editor.minibuffer().unwrap().prompt(), "Switch to buffer: ");
        assert_eq!(labels(&editor), ["b", "a", "c"]);
    }

    #[test]
    fn return_switches_to_the_most_recent_other_buffer() {
        let mut editor = editor_with_buffers(&["c", "b", "a"]);

        press(&mut editor, "C-x b RET");
        assert_eq!(editor.buffer_names(), ["b", "c", "a"]);

        press(&mut editor, "C-x b RET");
        assert_eq!(editor.buffer_names(), ["c", "b", "a"]);
    }

    #[test]
    fn typing_filters_buffer_names() {
        let mut editor = editor_with_buffers(&["c", "b", "a"]);

        press(&mut editor, "C-x b");
        type_text(&mut editor, "a");
        press(&mut editor, "RET");

        assert_eq!(editor.buffer().name(), "a");
    }

    #[test]
    fn an_unknown_name_creates_an_empty_buffer() {
        let mut editor = editor_with_buffers(&["c", "b"]);

        press(&mut editor, "C-x b");
        type_text(&mut editor, "notes");
        press(&mut editor, "RET");

        assert_eq!(editor.buffer_names(), ["notes", "c", "b"]);
        assert_eq!(editor.buffer().contents(), "");
        assert_eq!(editor.buffer().file(), None);
    }

    #[test]
    fn meta_return_uses_the_exact_name() {
        let mut editor = editor_with_buffers(&["c", "ab"]);

        press(&mut editor, "C-x b");
        type_text(&mut editor, "a");
        press(&mut editor, "M-RET");
        assert_eq!(editor.buffer_names(), ["a", "c", "ab"]);

        press(&mut editor, "C-x b");
        type_text(&mut editor, "c");
        press(&mut editor, "M-RET");
        assert_eq!(editor.buffer_names(), ["c", "a", "ab"]);
    }

    #[test]
    fn file_buffers_are_annotated_with_their_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        fs::write(&path, "").unwrap();
        let mut editor = Editor::new();
        editor.set_home_directory(None);
        editor.open_file(&path);

        press(&mut editor, "C-x b");

        let annotations: Vec<Option<String>> = editor
            .completions()
            .into_iter()
            .map(|completion| completion.annotation)
            .collect();
        assert_eq!(annotations, [None, Some(path.display().to_string())]);
    }

    #[test]
    fn switching_back_restores_the_view() {
        let mut editor = editor_with_buffers(&["a", "b"]);
        editor.resize_window(10, 0);
        editor.paste(Some("x\n".repeat(100)));
        press(&mut editor, "C-p C-p C-p");
        let top = editor.viewport().top().line;

        press(&mut editor, "C-x b RET");
        assert_eq!(editor.viewport().top().line, 0);
        press(&mut editor, "C-x b RET");

        assert_eq!(editor.viewport().top().line, top);
        assert_eq!(editor.buffer().line_of(editor.buffer().point()), 97);
    }

    #[test]
    fn kill_buffer_offers_the_current_buffer_first() {
        let mut editor = editor_with_buffers(&["c", "b", "a"]);

        press(&mut editor, "C-x k");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "Kill buffer: ");
        assert_eq!(labels(&editor), ["c", "b", "a"]);
        press(&mut editor, "RET");

        assert_eq!(editor.buffer_names(), ["b", "a"]);
    }

    #[test]
    fn killing_another_buffer_keeps_the_current_one() {
        let mut editor = editor_with_buffers(&["c", "b", "a"]);

        press(&mut editor, "C-x k");
        type_text(&mut editor, "a");
        press(&mut editor, "RET");

        assert_eq!(editor.buffer_names(), ["c", "b"]);
    }

    #[test]
    fn unknown_names_are_not_killed() {
        let mut editor = editor_with_buffers(&["c", "b"]);

        press(&mut editor, "C-x k");
        type_text(&mut editor, "zzz");
        press(&mut editor, "RET");

        assert_eq!(editor.minibuffer().unwrap().note(), Some("[No match]"));
        press(&mut editor, "M-RET");
        assert_eq!(editor.minibuffer().unwrap().note(), Some("[No match]"));
        assert_eq!(editor.buffer_names(), ["c", "b"]);
    }

    #[test]
    fn killing_the_last_buffer_leaves_an_empty_scratch() {
        let mut editor = editor_with_buffers(&["notes"]);
        type_text(&mut editor, "x");

        press(&mut editor, "C-x k RET");

        assert_eq!(editor.buffer_names(), ["*scratch*"]);
        assert_eq!(editor.buffer().contents(), "");
    }

    #[test]
    fn killing_a_modified_file_buffer_asks_first() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "").unwrap();
        let mut editor = Editor::new();
        editor.open_file(&dir.path().join("a.txt"));
        type_text(&mut editor, "x");

        press(&mut editor, "C-x k RET");
        assert_eq!(
            editor
                .question()
                .map(|question| question.prompt().to_owned()),
            Some("Buffer a.txt modified; kill anyway? (y or n) ".to_owned())
        );
        press(&mut editor, "n");
        assert_eq!(editor.buffer_names(), ["a.txt", "*scratch*"]);

        press(&mut editor, "C-x k RET y");
        assert_eq!(editor.buffer_names(), ["*scratch*"]);
    }

    #[test]
    fn killing_a_twin_restores_the_plain_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = Editor::new();
        for sub in ["x", "y"] {
            fs::create_dir(dir.path().join(sub)).unwrap();
            editor.open_file(&dir.path().join(sub).join("main.rs"));
        }

        press(&mut editor, "C-x k RET");

        assert_eq!(editor.buffer_names(), ["main.rs", "*scratch*"]);
    }

    #[test]
    fn killing_the_current_buffer_restores_the_next_ones_view() {
        let mut editor = editor_with_buffers(&["a", "b"]);
        editor.resize_window(10, 0);
        editor.paste(Some("x\n".repeat(100)));
        press(&mut editor, "C-p C-p C-p");
        let top = editor.viewport().top().line;
        press(&mut editor, "C-x b RET");

        press(&mut editor, "C-x k RET");

        assert_eq!(editor.buffer().name(), "a");
        assert_eq!(editor.viewport().top().line, top);
    }
}
