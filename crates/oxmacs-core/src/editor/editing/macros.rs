use crate::editor::Editor;
use crate::input::{Key, KeyCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Again {
    No,
    Quietly,
    Announced,
}

#[derive(Debug)]
struct Lines {
    line: usize,
    left: usize,
    before: usize,
}

#[derive(Debug)]
struct Playing {
    keys: Vec<Key>,
    next: usize,
    lines: Option<Lines>,
    again: Again,
}

#[derive(Debug, Default)]
pub(in crate::editor) struct Macros {
    recording: Option<Vec<Key>>,
    start: usize,
    last: Option<Vec<Key>>,
    counter: u64,
    playing: Option<Playing>,
    host: bool,
    again: bool,
    pub(in crate::editor) faulted: bool,
}

impl Macros {
    pub(in crate::editor) fn recording(&self) -> bool {
        self.recording.is_some()
    }

    pub(in crate::editor) fn replaying(&self) -> bool {
        self.playing.is_some()
    }
}

fn again(key: Key) -> bool {
    key.code() == KeyCode::Char('e') && key.modifiers().is_empty()
}

impl Editor {
    pub fn let_host_play_macros(&mut self) {
        self.macros.host = true;
    }

    pub fn macro_playing(&self) -> bool {
        self.macros.replaying()
    }

    pub fn play_macro_key(&mut self) {
        if self.macros.faulted {
            self.finish_replay();
            return;
        }
        let Some(playing) = &mut self.macros.playing else {
            return;
        };
        let Some(key) = playing.keys.get(playing.next).copied() else {
            self.next_macro_line();
            return;
        };
        playing.next += 1;
        self.handle_key(key);
        if self.macros.faulted {
            self.finish_replay();
        }
    }

    pub fn stop_macro(&mut self) {
        if self.macros.replaying() {
            self.macros.faulted = true;
            self.finish_replay();
        }
    }

    pub(in crate::editor) fn note_macro_key(&mut self, key: Key) -> bool {
        if self.macros.again && !self.macros.replaying() {
            self.macros.again = false;
            if self.pending.is_empty() && again(key) {
                self.message = None;
                self.call_macro(Again::Quietly);
                return true;
            }
        }
        if !self.macros.replaying()
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
        if self.macros.replaying() {
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
        if self.macros.replaying() {
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
        if self.macros.recording.is_none() && !self.macros.replaying() {
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
            self.call_macro(Again::No);
        }
    }

    pub(in crate::editor) fn end_and_call_macro(&mut self) {
        if self.macros.recording.is_some() {
            self.end_macro();
        }
        self.call_macro(Again::Announced);
    }

    fn call_macro(&mut self, again: Again) {
        if self.macros.replaying() {
            return;
        }
        let Some(keys) = self.macros.last.clone() else {
            self.show_message("No keyboard macro defined");
            return;
        };
        self.begin_replay(keys, None, again);
    }

    fn begin_replay(&mut self, keys: Vec<Key>, lines: Option<Lines>, again: Again) {
        self.macros.faulted = false;
        self.macros.again = false;
        self.macros.playing = Some(Playing {
            keys,
            next: 0,
            lines,
            again,
        });
        self.start_macro_line();
        self.current_mut().close_undo_step();
        if !self.macros.host {
            while self.macros.replaying() {
                self.play_macro_key();
            }
        }
    }

    fn start_macro_line(&mut self) {
        let Some(line) = self
            .macros
            .playing
            .as_ref()
            .and_then(|playing| playing.lines.as_ref())
            .map(|lines| lines.line)
        else {
            return;
        };
        let before = self.buffers.current().len_lines();
        let start = self.buffers.current().line_start(line);
        self.current_mut().set_point(start);
        if let Some(lines) = self
            .macros
            .playing
            .as_mut()
            .and_then(|playing| playing.lines.as_mut())
        {
            lines.before = before;
        }
    }

    fn next_macro_line(&mut self) {
        let after = self.buffers.current().len_lines();
        let more = match &mut self.macros.playing {
            Some(Playing {
                lines: Some(lines),
                next,
                ..
            }) if lines.left > 1 => {
                lines.left -= 1;
                lines.line = (lines.line + 1 + after).saturating_sub(lines.before);
                *next = 0;
                true
            }
            _ => false,
        };
        if more {
            self.current_mut().close_undo_step();
            self.start_macro_line();
        } else {
            self.finish_replay();
        }
    }

    fn finish_replay(&mut self) {
        let Some(playing) = self.macros.playing.take() else {
            return;
        };
        let faulted = self.macros.faulted;
        if faulted && self.search.is_some() {
            self.exit_search();
        }
        self.current_mut().close_undo_step();
        self.macros.again = !faulted && playing.again != Again::No;
        if self.macros.again && playing.again == Again::Announced {
            self.show_message("Type e to play the macro again");
        }
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
        let lines = Lines {
            line: first,
            left: last - first + 1,
            before: 0,
        };
        self.begin_replay(keys, Some(lines), Again::No);
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
    fn a_host_can_play_the_macro_one_key_at_a_time_and_stop_it() {
        let mut editor = editor_with(LINES, 0);
        editor.let_host_play_macros();
        press(&mut editor, "<f3> C-a x C-n <f4>");

        press(&mut editor, "<f4>");
        assert!(editor.macro_playing());
        assert_eq!(editor.buffer().contents(), "xone\ntwo\nthree\nfour\n");
        editor.play_macro_key();
        editor.play_macro_key();
        assert_eq!(editor.buffer().contents(), "xone\nxtwo\nthree\nfour\n");
        editor.stop_macro();
        assert!(!editor.macro_playing());

        press(&mut editor, "C-n C-a C-SPC C-n C-n C-x C-k r");
        while editor.macro_playing() {
            editor.play_macro_key();
        }
        assert_eq!(editor.buffer().contents(), "xone\nxtwo\nxthree\nxfour\n");
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
