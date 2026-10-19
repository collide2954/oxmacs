use std::ops::Range;

use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::syntax::Language;
use crate::text::Buffer;

const NOTHING: &str = "No R code here";

fn blank(buffer: &Buffer, line: usize) -> bool {
    buffer.line_text(line).trim().is_empty()
}

fn paragraph(buffer: &Buffer, point: usize) -> Option<Range<usize>> {
    let lines = buffer.len_lines();
    let mut first = (buffer.line_of(point)..lines).find(|line| !blank(buffer, *line))?;
    let mut last = first;
    while first > 0 && !blank(buffer, first - 1) {
        first -= 1;
    }
    while last + 1 < lines && !blank(buffer, last + 1) {
        last += 1;
    }
    Some(buffer.line_start(first)..buffer.line_end(last))
}

impl Editor {
    fn r_send(&mut self, code: &str) {
        if !self.r_running() && !self.start_r() {
            return;
        }
        self.show_console(false);
        self.r_evaluate(code);
    }

    fn r_send_selection(&mut self) -> bool {
        let buffer = self.buffers.current();
        let Some(selection) = buffer.selection() else {
            return false;
        };
        let code = buffer.text_in(selection);
        self.buffers.current_mut().deactivate_mark();
        self.r_send(&code);
        true
    }

    fn r_send_and_step(&mut self, range: Option<Range<usize>>) {
        let Some(range) = range else {
            self.show_message(NOTHING);
            return;
        };
        let buffer = self.buffers.current();
        let next = self
            .buffers
            .syntax(0)
            .and_then(|syntax| syntax.statement_after(buffer, range.end))
            .unwrap_or_else(|| buffer.len_chars());
        let code = buffer.text_in(range);
        self.buffers.current_mut().set_point(next);
        self.r_send(&code);
    }

    pub(in crate::editor) fn r_send_statement(&mut self) {
        if self.r_send_selection() {
            return;
        }
        self.sync_syntax();
        let buffer = self.buffers.current();
        let range = self
            .buffers
            .syntax(0)
            .and_then(|syntax| syntax.statement_at(buffer, buffer.point()));
        self.r_send_and_step(range);
    }

    pub(in crate::editor) fn r_send_function(&mut self) {
        if self.r_send_selection() {
            return;
        }
        self.sync_syntax();
        let buffer = self.buffers.current();
        let point = buffer.point();
        let range = self
            .buffers
            .syntax(0)
            .and_then(|syntax| syntax.defun_around(buffer, point))
            .or_else(|| paragraph(buffer, point));
        self.r_send_and_step(range);
    }

    pub(in crate::editor) fn r_switch_to_console(&mut self) {
        if matches!(self.buffers.mode(0), Some(Mode::Console(_))) {
            let script = |editor: &Self| {
                editor.buffers.find_mode(|mode| {
                    matches!(mode, Mode::Text(Some(syntax)) if syntax.language() == Language::R)
                })
            };
            if script(self).is_none() {
                self.show_message("No R script to go back to");
                return;
            }
            self.show_beside(script);
            return;
        }
        if !self.r_running() && !self.start_r() {
            return;
        }
        self.show_console(true);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::editor::test_support::{editor_with, press};
    use crate::editor::{Editor, Effect, JobId};
    use crate::external::tools::Probe;
    use crate::syntax::Language;

    const PROMPT: &str = "\u{1}>\u{2}\u{3}";

    fn found(_: Option<&str>) -> Probe {
        Probe::Found {
            path: PathBuf::from("R-bin"),
            version: "R version 4.6.1".to_owned(),
        }
    }

    fn script(text: &str, point: usize) -> Editor {
        let mut editor = editor_with(text, point);
        editor.set_major_mode(Some(Language::R));
        editor.probe_r = found;
        editor
    }

    fn started(editor: &mut Editor) -> Vec<JobId> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::StartSession(job) => Some(job.id),
                _ => None,
            })
            .collect()
    }

    fn sent(editor: &mut Editor) -> Vec<String> {
        editor
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Send(_, text) => Some(text),
                _ => None,
            })
            .collect()
    }

    fn running(editor: &mut Editor) -> JobId {
        press(editor, "C-c C-z");
        let job = started(editor)[0];
        editor.session_output(job, PROMPT);
        press(editor, "C-c C-z");
        job
    }

    #[test]
    fn control_return_starts_r_sends_the_statement_and_steps() {
        let mut editor = script("x <- 1\n\ny <- df |>\n  head()\n", 2);

        press(&mut editor, "C-RET");

        let effects = editor.take_effects();
        assert!(matches!(effects.first(), Some(Effect::StartSession(_))));
        assert!(matches!(&effects[1], Effect::Send(_, code) if code == "x <- 1\n"));
        assert_eq!(editor.buffer().name(), "t");
        assert_eq!(editor.buffer().point(), 8);
        assert_eq!(editor.tiling().windows().len(), 2);

        press(&mut editor, "C-RET");

        assert_eq!(sent(&mut editor), ["y <- df |>\n  head()\n"]);
        assert_eq!(editor.buffer().point(), editor.buffer().len_chars());
    }

    #[test]
    fn r_is_started_only_once() {
        let mut editor = script("a\nb\n", 0);

        press(&mut editor, "C-RET");
        assert_eq!(started(&mut editor).len(), 1);
        press(&mut editor, "C-RET");

        assert_eq!(started(&mut editor), Vec::new());
    }

    #[test]
    fn a_selection_is_sent_as_it_is_and_point_stays() {
        let mut editor = script("a <- 1\nb <- 2\n", 0);
        running(&mut editor);
        editor.take_effects();
        press(&mut editor, "C-SPC C-e");

        press(&mut editor, "C-RET");

        assert_eq!(sent(&mut editor), ["a <- 1\n"]);
        assert_eq!(editor.buffer().point(), 6);
    }

    #[test]
    fn control_c_control_c_sends_the_function_or_the_paragraph() {
        let text = "f <- function(x) {\n  x + 1\n}\n\na <- 1\nb <- 2\n\nc <- 3\n";
        let mut editor = script(text, 22);
        running(&mut editor);
        editor.take_effects();

        press(&mut editor, "C-c C-c");
        assert_eq!(sent(&mut editor), ["f <- function(x) {\n  x + 1\n}\n"]);
        assert_eq!(editor.buffer().point(), 30);

        press(&mut editor, "C-n C-c C-c");
        assert_eq!(sent(&mut editor), ["a <- 1\nb <- 2\n"]);
        assert_eq!(editor.buffer().point(), 45);
    }

    #[test]
    fn control_c_control_z_moves_between_script_and_console() {
        let mut editor = script("x\n", 0);

        press(&mut editor, "C-c C-z");
        assert_eq!(editor.buffer().name(), "*R*");
        assert_eq!(started(&mut editor).len(), 1);

        press(&mut editor, "C-c C-z");
        assert_eq!(editor.buffer().name(), "t");

        press(&mut editor, "C-c C-z");
        assert_eq!(editor.buffer().name(), "*R*");
        assert_eq!(started(&mut editor), Vec::new());
    }
}
