use crate::editor::Editor;
use crate::input::{Key, KeyCode};

#[derive(Debug, Default)]
pub(in crate::editor) struct Macros {
    recording: Option<Vec<Key>>,
    start: usize,
    last: Option<Vec<Key>>,
    counter: u64,
    replaying: bool,
    again: bool,
    pub(in crate::editor) faulted: bool,
}

impl Macros {
    pub(in crate::editor) fn recording(&self) -> bool {
        self.recording.is_some()
    }

    pub(in crate::editor) fn replaying(&self) -> bool {
        self.replaying
    }
}

fn again(key: Key) -> bool {
    key.code() == KeyCode::Char('e') && key.modifiers().is_empty()
}

impl Editor {
    pub(in crate::editor) fn note_macro_key(&mut self, key: Key) -> bool {
        if self.macros.again && !self.macros.replaying {
            self.macros.again = false;
            if self.pending.is_empty() && again(key) {
                self.message = None;
                self.macros.again = self.call_macro();
                return true;
            }
        }
        if !self.macros.replaying
            && let Some(recording) = &mut self.macros.recording
        {
            if self.pending.is_empty() {
                self.macros.start = recording.len();
            }
            recording.push(key);
        }
        false
    }

    pub(in crate::editor) fn cancel_macro(&mut self) {
        self.macros.recording = None;
    }

    pub(in crate::editor) fn start_macro(&mut self) {
        if self.macros.replaying {
            return;
        }
        if self.macros.recording.is_some() {
            self.show_message("Already defining a keyboard macro");
            return;
        }
        self.macros.recording = Some(Vec::new());
        self.macros.start = 0;
        self.macros.counter = 0;
        self.show_message("Defining keyboard macro...");
    }

    pub(in crate::editor) fn end_macro(&mut self) {
        if self.macros.replaying {
            return;
        }
        let Some(mut keys) = self.macros.recording.take() else {
            self.show_message("Not defining a keyboard macro");
            return;
        };
        keys.truncate(self.macros.start);
        self.macros.last = Some(keys);
        self.show_message("Keyboard macro defined");
    }

    pub(in crate::editor) fn start_macro_or_count(&mut self) {
        if self.macros.recording.is_none() && !self.macros.replaying {
            self.start_macro();
            return;
        }
        if self.refuse_read_only() {
            return;
        }
        let counter = self.macros.counter.to_string();
        self.macros.counter += 1;
        self.current_mut().insert(&counter);
    }

    pub(in crate::editor) fn end_or_call_macro(&mut self) {
        if self.macros.recording.is_some() {
            self.end_macro();
        } else {
            self.call_macro();
        }
    }

    pub(in crate::editor) fn end_and_call_macro(&mut self) {
        if self.macros.recording.is_some() {
            self.end_macro();
        }
        if self.call_macro() {
            self.macros.again = true;
            self.show_message("Type e to play the macro again");
        }
    }

    fn call_macro(&mut self) -> bool {
        if self.macros.replaying {
            return false;
        }
        let Some(keys) = self.macros.last.clone() else {
            self.show_message("No keyboard macro defined");
            return false;
        };
        self.replay(&keys)
    }

    pub(in crate::editor) fn replay(&mut self, keys: &[Key]) -> bool {
        self.current_mut().close_undo_step();
        self.macros.replaying = true;
        self.macros.faulted = false;
        for key in keys {
            self.handle_key(*key);
            if self.macros.faulted {
                break;
            }
        }
        let faulted = self.macros.faulted;
        if faulted && self.search.is_some() {
            self.exit_search();
        }
        self.macros.replaying = false;
        self.current_mut().close_undo_step();
        !faulted
    }

    pub(in crate::editor) fn apply_macro_to_lines(&mut self) {
        let Some(keys) = self.macros.last.clone() else {
            self.show_message("No keyboard macro defined");
            return;
        };
        let buffer = self.buffers.current();
        let Some(range) = buffer.selection() else {
            self.show_message("Select the lines to play the macro on");
            return;
        };
        let first = buffer.line_of(range.start);
        let mut last = buffer.line_of(range.end);
        if range.end > range.start && buffer.line_start(last) == range.end && last > first {
            last -= 1;
        }
        self.current_mut().deactivate_mark();
        let mut line = first;
        for _ in first..=last {
            let before = self.buffers.current().len_lines();
            let start = self.buffers.current().line_start(line);
            self.current_mut().set_point(start);
            if !self.replay(&keys) {
                break;
            }
            let after = self.buffers.current().len_lines();
            line = (line + 1 + after).saturating_sub(before);
        }
    }
}
#[cfg(test)]
mod tests {
    use crate::editor::Effect;
    use crate::editor::test_support::{editor_with, press};

    const LINES: &str = "one\ntwo\nthree\nfour\n";

    #[test]
    fn f3_records_and_f4_replays() {
        let mut editor = editor_with(LINES, 0);

        press(&mut editor, "<f3>");
        assert_eq!(editor.message(), Some("Defining keyboard macro..."));
        press(&mut editor, "C-a x C-n <f4>");
        assert_eq!(editor.message(), Some("Keyboard macro defined"));
        press(&mut editor, "<f4>");

        assert_eq!(editor.buffer().contents(), "xone\nxtwo\nthree\nfour\n");
    }

    #[test]
    fn c_x_e_replays_and_e_keeps_going() {
        let mut editor = editor_with(LINES, 0);
        press(&mut editor, "C-x ( C-a - C-n C-x )");

        press(&mut editor, "C-x e e e");

        assert_eq!(editor.buffer().contents(), "-one\n-two\n-three\n-four\n");
    }

    #[test]
    fn other_keys_after_c_x_e_work_as_usual() {
        let mut editor = editor_with(LINES, 0);
        press(&mut editor, "C-x ( C-a - C-n C-x )");

        press(&mut editor, "C-x e a e");

        assert_eq!(editor.buffer().contents(), "-one\n-two\ntaehree\nfour\n");
    }

    #[test]
    fn replay_stops_at_the_first_error() {
        let mut editor = editor_with(LINES, 0);
        press(&mut editor, "<f3> C-a x <f12> y <f4>");
        assert_eq!(editor.buffer().contents(), "xyone\ntwo\nthree\nfour\n");

        press(&mut editor, "C-n <f4>");

        assert_eq!(editor.buffer().contents(), "xyone\nxtwo\nthree\nfour\n");
        assert_eq!(editor.message(), Some("<f12> is undefined"));
    }

    #[test]
    fn a_failing_search_stops_the_replay() {
        let mut editor = editor_with("a.b.c", 0);
        press(&mut editor, "<f3> C-s . RET - <f4>");
        assert_eq!(editor.buffer().contents(), "a.-b.c");

        press(&mut editor, "<f4> <f4>");

        assert_eq!(editor.buffer().contents(), "a.-b.-c");
        assert!(editor.search().is_none());
    }

    #[test]
    fn a_replay_undoes_as_one_step() {
        let mut editor = editor_with(LINES, 0);
        press(&mut editor, "<f3> C-a x C-e y C-n <f4>");

        press(&mut editor, "<f4> C-/");

        assert_eq!(editor.buffer().contents(), "xoney\ntwo\nthree\nfour\n");
    }

    #[test]
    fn f3_while_recording_inserts_a_counter() {
        let mut editor = editor_with(LINES, 0);
        press(&mut editor, "<f3> C-a <f3> . SPC C-n <f4>");

        press(&mut editor, "<f4> <f4>");

        assert_eq!(
            editor.buffer().contents(),
            "0. one\n1. two\n2. three\nfour\n"
        );
    }

    #[test]
    fn the_macro_can_run_on_each_line_of_the_region() {
        let mut editor = editor_with(LINES, 0);
        press(&mut editor, "<f3> C-a * SPC <f4>");

        press(&mut editor, "C-n C-a C-SPC C-n C-n C-x C-k r");

        assert_eq!(editor.buffer().contents(), "* one\n* two\n* three\nfour\n");
    }

    #[test]
    fn pasting_in_a_replay_uses_the_kill_ring_at_once() {
        let mut editor = editor_with("ab\n", 0);
        press(&mut editor, "C-SPC C-f M-w");
        editor.take_effects();

        press(&mut editor, "<f3> C-y");
        let asked = editor
            .take_effects()
            .into_iter()
            .any(|effect| effect == Effect::ReadClipboard);
        assert!(asked);
        editor.paste(Some("a".to_owned()));
        press(&mut editor, "<f4> <f4>");

        assert_eq!(editor.buffer().contents(), "aaab\n");
    }

    #[test]
    fn misuse_is_explained() {
        let mut editor = editor_with(LINES, 0);

        press(&mut editor, "<f4>");
        assert_eq!(editor.message(), Some("No keyboard macro defined"));
        press(&mut editor, "C-x )");
        assert_eq!(editor.message(), Some("Not defining a keyboard macro"));
        press(&mut editor, "C-x ( C-x (");
        assert_eq!(editor.message(), Some("Already defining a keyboard macro"));
    }

    #[test]
    fn c_g_while_recording_cancels_the_macro() {
        let mut editor = editor_with(LINES, 0);
        press(&mut editor, "<f3> x <f4>");

        press(&mut editor, "<f3> y C-g <f4>");

        assert_eq!(editor.buffer().contents(), "xyxone\ntwo\nthree\nfour\n");
    }

    #[test]
    fn the_mode_line_says_a_macro_is_being_recorded() {
        let mut editor = editor_with(LINES, 0);

        press(&mut editor, "<f3>");
        assert!(
            editor
                .mode_line_segments()
                .contains(&"Recording macro".to_owned())
        );
        press(&mut editor, "<f4>");
        assert!(
            !editor
                .mode_line_segments()
                .contains(&"Recording macro".to_owned())
        );
    }
}
