use crate::command::Command;
use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::external::r;
use crate::syntax::Language;
use crate::text::BufferId;

const AUTO_PREFIX: usize = 3;

#[derive(Debug, Clone, Copy)]
pub(in crate::editor) struct Asked {
    buffer: BufferId,
    line_start: usize,
    explicit: bool,
}

impl Editor {
    pub(super) fn in_r(&self) -> bool {
        matches!(self.buffers.mode(0), Some(Mode::Console(_)))
            || self.buffers.language(0) == Some(Language::R)
    }

    fn r_complete(&mut self, explicit: bool) {
        if !self.in_r() {
            return;
        }
        let refusal = if !self.r_running() {
            Some("Start R with C-c C-z to complete from its session")
        } else if !self.r_waiting() {
            Some("R is busy")
        } else {
            None
        };
        if let Some(refusal) = refusal {
            if explicit {
                self.show_message(refusal);
            }
            return;
        }
        let buffer = self.buffers.current();
        let point = buffer.point();
        let input = self.console_input_start().unwrap_or(0);
        let line_start = buffer.line_start(buffer.line_of(point)).max(input);
        let line = buffer.text_in(line_start..point);
        let call = format!(
            "getOption(\"oxmacs\")$complete({}, {}L)",
            r::string(&line),
            line.chars().count()
        );
        let asked = Asked {
            buffer: buffer.id(),
            line_start,
            explicit,
        };
        if self.r_request(&call) {
            self.set_r_asked(Some(asked));
        }
    }

    pub(in crate::editor) fn completion_at_point(&mut self) {
        self.r_complete(true);
    }

    fn in_comment(&self) -> bool {
        let buffer = self.buffers.current();
        let point = buffer.point();
        let line = buffer.line_of(point);
        self.buffers
            .syntax(0)
            .and_then(|syntax| syntax.comment_start(buffer, line))
            .is_some_and(|column| buffer.line_start(line) + column < point)
    }

    fn typed_prefix(&self) -> usize {
        let buffer = self.buffers.current();
        buffer
            .chars_before(buffer.point())
            .take_while(|c| c.is_alphanumeric() || matches!(c, '.' | '_'))
            .count()
    }

    fn auto_mark(&self) -> (BufferId, u64) {
        let buffer = self.buffers.current();
        (buffer.id(), buffer.revision())
    }

    pub fn wants_auto_complete(&self) -> bool {
        self.last_command == Some(Command::SelfInsertCommand)
            && self.minibuffer.is_none()
            && self.popup.is_none()
            && self.in_r()
            && self.r_waiting()
            && !self.r_asking()
            && self.typed_prefix() >= AUTO_PREFIX
            && !self.in_comment()
            && self.r_auto_mark() != Some(self.auto_mark())
    }

    pub fn auto_complete(&mut self) {
        if self.wants_auto_complete() {
            self.set_r_auto_mark(self.auto_mark());
            self.r_complete(false);
        }
    }

    pub(in crate::editor) fn indent_or_complete(&mut self) {
        let buffer = self.buffers.current();
        let before = (buffer.revision(), buffer.point());
        let selecting = buffer.selection().is_some();
        self.indent_for_tab();
        let buffer = self.buffers.current();
        let unchanged = before == (buffer.revision(), buffer.point());
        let after_code = buffer
            .chars_before(buffer.point())
            .next()
            .is_some_and(|c| !c.is_whitespace());
        if unchanged && !selecting && after_code {
            self.r_complete(true);
        }
    }

    pub(in crate::editor) fn r_completions(&mut self, payload: &str) {
        let Some(asked) = self.take_r_asked() else {
            return;
        };
        let mut fields = payload.split('\t');
        let Some(offset) = fields.next().and_then(|start| start.parse::<usize>().ok()) else {
            return;
        };
        let buffer = self.buffers.current();
        let point = buffer.point();
        let start = asked.line_start + offset;
        let here = self.minibuffer.is_none()
            && buffer.id() == asked.buffer
            && start <= point
            && buffer.line_of(start) == buffer.line_of(point);
        if !here {
            return;
        }
        let candidates = fields.map(str::to_owned).collect();
        self.complete_at(start, candidates, asked.explicit);
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::{
        R_PROMPT, press, r_running, r_script, r_sent, r_sessions, type_text,
    };
    use crate::editor::{Editor, JobId};

    fn shown(editor: &Editor) -> Option<Vec<String>> {
        editor.pane().popup().map(|view| view.items)
    }

    fn console() -> (Editor, JobId) {
        let mut editor = r_script("x\n", 0);
        let job = r_running(&mut editor);
        press(&mut editor, "C-c C-z");
        editor.take_effects();
        (editor, job)
    }

    fn answer(editor: &mut Editor, job: JobId, payload: &str) {
        editor.session_output(job, &format!("\u{1}complete\u{2}{payload}\u{3}{R_PROMPT}"));
    }

    #[test]
    fn tab_in_the_console_asks_r_and_offers_its_answers() {
        let (mut editor, job) = console();
        type_text(&mut editor, "pr");

        press(&mut editor, "TAB");
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$complete(\"pr\", 2L)\n"]
        );
        answer(&mut editor, job, "0\tprint\tprod");

        assert_eq!(
            shown(&editor),
            Some(vec!["print".to_owned(), "prod".to_owned()])
        );
        assert_eq!(editor.buffer().contents(), "> pr");
        press(&mut editor, "TAB");
        assert_eq!(editor.buffer().contents(), "> print");
    }

    #[test]
    fn tab_in_a_script_indents_before_it_completes() {
        let mut editor = r_script("f <- function() {\npr\n}\n", 20);
        let job = r_running(&mut editor);

        press(&mut editor, "TAB");
        assert_eq!(editor.buffer().contents(), "f <- function() {\n  pr\n}\n");
        assert_eq!(r_sent(&mut editor), Vec::<String>::new());

        press(&mut editor, "TAB");
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$complete(\"  pr\", 4L)\n"]
        );
        answer(&mut editor, job, "2\tprint\tprod");
        assert_eq!(
            shown(&editor),
            Some(vec!["print".to_owned(), "prod".to_owned()])
        );
    }

    #[test]
    fn a_sole_candidate_is_inserted_at_once() {
        let (mut editor, job) = console();
        type_text(&mut editor, "mydata$a");

        press(&mut editor, "TAB");
        answer(&mut editor, job, "0\tmydata$alpha");

        assert_eq!(editor.buffer().contents(), "> mydata$alpha");
        assert_eq!(shown(&editor), None);
    }

    #[test]
    fn nothing_to_offer_is_reported() {
        let (mut editor, job) = console();
        type_text(&mut editor, "zz");

        press(&mut editor, "TAB");
        answer(&mut editor, job, "0");

        assert_eq!(editor.message(), Some("No completions"));
    }

    #[test]
    fn completion_needs_r_to_be_running() {
        let mut editor = r_script("pr", 2);

        press(&mut editor, "C-M-i");

        assert_eq!(
            editor.message(),
            Some("Start R with C-c C-z to complete from its session")
        );
        assert_eq!(r_sessions(&mut editor), Vec::new());
    }

    #[test]
    fn busy_r_is_not_asked() {
        let mut editor = r_script("Sys.sleep(5)\npr", 15);
        r_running(&mut editor);
        press(&mut editor, "C-p C-RET");
        editor.take_effects();
        press(&mut editor, "C-e");

        press(&mut editor, "C-M-i");

        assert_eq!(editor.message(), Some("R is busy"));
        assert_eq!(r_sent(&mut editor), Vec::<String>::new());
    }

    #[test]
    fn three_typed_characters_ask_for_completion_once_idle() {
        let (mut editor, job) = console();
        type_text(&mut editor, "me");
        assert!(!editor.wants_auto_complete());
        type_text(&mut editor, "a");
        assert!(editor.wants_auto_complete());

        editor.auto_complete();

        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$complete(\"mea\", 3L)\n"]
        );
        assert!(!editor.wants_auto_complete());
        answer(&mut editor, job, "0\tmean");
        assert_eq!(shown(&editor), Some(vec!["mean".to_owned()]));
        assert_eq!(editor.buffer().contents(), "> mea");
        assert!(!editor.wants_auto_complete());
    }

    #[test]
    fn automatic_answers_stay_quiet_and_skip_what_is_already_typed() {
        let (mut editor, job) = console();
        type_text(&mut editor, "mean");
        editor.auto_complete();
        answer(&mut editor, job, "0\tmean");
        assert_eq!(shown(&editor), None);
        assert_eq!(editor.message(), None);

        type_text(&mut editor, "x");
        editor.auto_complete();
        answer(&mut editor, job, "0");
        assert_eq!(editor.message(), None);
    }

    #[test]
    fn busy_r_comments_and_moves_do_not_ask() {
        let (mut editor, _) = console();
        type_text(&mut editor, "mea");
        press(&mut editor, "C-b");
        assert!(!editor.wants_auto_complete());

        let mut editor = r_script("x <- 1 # mea", 12);
        r_running(&mut editor);
        type_text(&mut editor, "n");
        assert!(!editor.wants_auto_complete());

        let mut editor = r_script("Sys.sleep(5)\nmea", 16);
        r_running(&mut editor);
        press(&mut editor, "M-< C-RET M->");
        type_text(&mut editor, "n");
        assert!(!editor.wants_auto_complete());
    }

    #[test]
    fn answers_for_another_buffer_are_dropped() {
        let (mut editor, job) = console();
        type_text(&mut editor, "pr");
        press(&mut editor, "TAB C-c C-z");

        answer(&mut editor, job, "0\tprint\tprod");

        assert_eq!(editor.buffer().name(), "t");
        assert_eq!(shown(&editor), None);
    }
}
