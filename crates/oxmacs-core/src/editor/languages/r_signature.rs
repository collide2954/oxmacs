use crate::editor::Editor;
use crate::external::r;
use crate::text::BufferId;

const WINDOW: usize = 4000;
const KEYWORDS: [&str; 4] = ["if", "for", "while", "function"];

pub(in crate::editor) type Call = (BufferId, usize, String);

fn call_name(text: &str) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut open: Vec<(char, usize)> = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut comment = false;
    for (index, c) in chars.iter().copied().enumerate() {
        if comment {
            comment = c != '\n';
            continue;
        }
        if let Some(closing) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == closing {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' | '`' => quote = Some(c),
            '#' => comment = true,
            '(' | '[' | '{' => open.push((c, index)),
            ')' | ']' | '}' => {
                open.pop();
            }
            _ => {}
        }
    }
    if quote.is_some() || comment {
        return None;
    }
    let &(bracket, at) = open.last()?;
    if bracket != '(' {
        return None;
    }
    let mut end = at;
    while end > 0 && chars[end - 1] == ' ' {
        end -= 1;
    }
    let mut start = end;
    while start > 0
        && (chars[start - 1].is_alphanumeric() || matches!(chars[start - 1], '.' | '_' | ':'))
    {
        start -= 1;
    }
    let name: String = chars[start..end].iter().collect();
    let valid = name
        .chars()
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '.')
        && !name.ends_with(':')
        && !KEYWORDS.contains(&name.as_str());
    valid.then_some((name, at))
}

impl Editor {
    fn call_at_point(&self) -> Option<Call> {
        if !self.in_r() {
            return None;
        }
        let buffer = self.buffers.current();
        let point = buffer.point();
        let floor = self.console_input_start().unwrap_or(0).min(point);
        let from = point.saturating_sub(WINDOW).max(floor);
        let from = buffer.line_start(buffer.line_of(from)).max(floor);
        let (name, at) = call_name(&buffer.text_in(from..point))?;
        Some((buffer.id(), from + at, name))
    }

    pub fn wants_signature(&self) -> bool {
        self.minibuffer.is_none()
            && self.popup.is_none()
            && self.question.is_none()
            && self.r_running()
            && self.call_at_point() != self.r_sessions.signature_shown
    }

    pub fn show_signature(&mut self) {
        if !self.wants_signature() {
            return;
        }
        let Some(call) = self.call_at_point() else {
            self.r_sessions.signature_shown = None;
            return;
        };
        if let Some(known) = self.cached_signature(&call.2) {
            if !known.is_empty() {
                self.show_message(known);
            }
            self.r_sessions.signature_shown = Some(call);
            return;
        }
        if !self.r_waiting() || self.r_asking() {
            return;
        }
        let request = format!("getOption(\"oxmacs\")$signature({})", r::string(&call.2));
        self.r_sessions.signature_shown = Some(call);
        self.r_request(&request);
    }

    pub(in crate::editor) fn r_signature_arrived(&mut self, payload: &str) {
        let Some((name, text)) = payload.split_once('\t') else {
            return;
        };
        self.cache_signature(name, text);
        let here = self
            .call_at_point()
            .is_some_and(|(_, _, current)| current == name);
        if here && !text.is_empty() {
            self.show_message(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::call_name;
    use crate::editor::Editor;
    use crate::editor::JobId;
    use crate::editor::test_support::{R_PROMPT, press, r_running, r_script, r_sent, type_text};

    fn name(text: &str) -> Option<String> {
        call_name(text).map(|(name, _)| name)
    }

    #[test]
    fn the_innermost_open_call_is_found() {
        assert_eq!(name("lm(mpg ~ wt, ").as_deref(), Some("lm"));
        assert_eq!(name("fit <- stats::lm(").as_deref(), Some("stats::lm"));
        assert_eq!(name("plot(density(x), ").as_deref(), Some("plot"));
        assert_eq!(name("plot(density(").as_deref(), Some("density"));
        assert_eq!(name("{\n  summary (").as_deref(), Some("summary"));
        assert_eq!(name(".hidden(").as_deref(), Some(".hidden"));
    }

    #[test]
    fn strings_comments_keywords_and_brackets_hide_calls() {
        assert_eq!(name("paste(\"(\", "), Some("paste".to_owned()));
        assert_eq!(name("paste(\"a, "), None);
        assert_eq!(name("# see plot("), None);
        assert_eq!(name("if (x > "), None);
        assert_eq!(name("function(a, "), None);
        assert_eq!(name("x[f(1), "), None);
        assert_eq!(name("plot(x)"), None);
        assert_eq!(name("(1 + "), None);
        assert_eq!(name("1(x"), None);
    }

    fn inside_plot() -> (Editor, JobId) {
        let mut editor = r_script("x <- 1:10\nplot(x, \n", 18);
        let job = r_running(&mut editor);
        (editor, job)
    }

    fn signature(editor: &mut Editor, job: JobId, payload: &str) {
        editor.session_output(job, &format!("\u{1}signature\u{2}{payload}\u{3}{R_PROMPT}"));
    }

    #[test]
    fn the_signature_of_the_call_at_point_is_asked_and_shown() {
        let (mut editor, job) = inside_plot();
        assert!(editor.wants_signature());

        editor.show_signature();
        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$signature(\"plot\")\n"]
        );
        assert!(!editor.wants_signature());

        signature(&mut editor, job, "plot\tplot(x, y, ...)");
        assert_eq!(editor.message(), Some("plot(x, y, ...)"));
    }

    #[test]
    fn known_signatures_show_without_asking_again() {
        let (mut editor, job) = inside_plot();
        editor.show_signature();
        r_sent(&mut editor);
        signature(&mut editor, job, "plot\tplot(x, y, ...)");

        press(&mut editor, "C-a");
        editor.show_signature();
        editor.show_message("elsewhere");
        press(&mut editor, "C-e");
        assert!(editor.wants_signature());
        editor.show_signature();

        assert_eq!(editor.message(), Some("plot(x, y, ...)"));
        assert_eq!(r_sent(&mut editor), Vec::<String>::new());
    }

    #[test]
    fn running_code_forgets_known_signatures() {
        let (mut editor, job) = inside_plot();
        editor.show_signature();
        r_sent(&mut editor);
        signature(&mut editor, job, "plot\tplot(x, y, ...)");

        press(&mut editor, "C-c C-z");
        type_text(&mut editor, "plot <- function(z) z");
        press(&mut editor, "RET");
        editor.session_output(job, R_PROMPT);
        r_sent(&mut editor);
        press(&mut editor, "C-c C-z C-a");
        press(&mut editor, "C-e");
        editor.show_signature();

        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$signature(\"plot\")\n"]
        );
    }

    #[test]
    fn nothing_is_asked_outside_a_call_or_without_r() {
        let editor = r_script("plot(x, ", 8);
        assert!(!editor.wants_signature());

        let (mut editor, _) = inside_plot();
        press(&mut editor, "M-<");
        assert!(!editor.wants_signature());
    }
}
