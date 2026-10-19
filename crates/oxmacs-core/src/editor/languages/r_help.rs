use crate::editor::Editor;
use crate::editor::buffers::Mode;
use crate::editor::page::Page;
use crate::editor::prompts::minibuffer::Purpose;
use crate::editor::special::help::Help;
use crate::external::r;
use crate::text::Buffer;
use crate::theme::Highlight;

const NAME: &str = "*R help*";
const BACKSPACE: char = '\u{8}';

fn symbol_at(buffer: &Buffer, point: usize) -> Option<String> {
    let part = |c: &char| c.is_alphanumeric() || matches!(c, '.' | '_');
    let before: Vec<char> = buffer.chars_before(point).take_while(part).collect();
    let mut symbol: String = before.into_iter().rev().collect();
    symbol.extend(buffer.chars_after(point).take_while(part));
    let starts_well = symbol.starts_with(|c: char| !c.is_ascii_digit() && c != '_');
    starts_well.then_some(symbol)
}

fn overstruck(line: &str) -> Vec<(String, bool)> {
    let chars: Vec<char> = line.chars().collect();
    let mut runs: Vec<(String, bool)> = Vec::new();
    let mut index = 0;
    while let Some(&c) = chars.get(index) {
        let struck = chars.get(index + 1) == Some(&BACKSPACE) && index + 2 < chars.len();
        let (shown, step) = if struck {
            (chars[index + 2], 3)
        } else {
            (c, 1)
        };
        index += step;
        if shown == BACKSPACE {
            continue;
        }
        match runs.last_mut() {
            Some((text, marked)) if *marked == struck => text.push(shown),
            _ => runs.push((shown.to_string(), struck)),
        }
    }
    runs
}

fn page(text: &str) -> Page {
    let mut page = Page::default();
    for line in text.lines() {
        let runs = overstruck(line);
        let parts: Vec<(&str, Option<Highlight>)> = runs
            .iter()
            .map(|(text, marked)| (text.as_str(), marked.then_some(Highlight::Heading)))
            .collect();
        page.line(&parts);
    }
    page
}

impl Editor {
    pub(in crate::editor) fn r_help(&mut self) {
        let buffer = self.buffers.current();
        let default = symbol_at(buffer, buffer.point());
        let prompt = match &default {
            Some(symbol) => format!("R help on (default {symbol}): "),
            None => "R help on: ".to_owned(),
        };
        if self.open_minibuffer(&prompt, Purpose::RHelp)
            && let Some(minibuffer) = &mut self.minibuffer
        {
            minibuffer.default = default;
        }
    }

    pub(in crate::editor) fn r_help_on(&mut self, input: &str, default: Option<String>) {
        let typed = input.trim();
        let Some(topic) = (!typed.is_empty()).then(|| typed.to_owned()).or(default) else {
            return;
        };
        if !self.r_running() && !self.start_r() {
            return;
        }
        self.r_request(&format!(
            "getOption(\"oxmacs\")$help({})",
            r::string(&topic)
        ));
    }

    fn r_help_index(&self) -> Option<usize> {
        self.buffers
            .find_mode(|mode| matches!(mode, Mode::Help(help) if help.is_r()))
    }

    pub(in crate::editor) fn show_r_help(&mut self, text: &str) {
        let page = page(text);
        if self.r_help_index().is_none() {
            self.beside(|editor| {
                let mut buffer = Buffer::with_text(NAME, "");
                buffer.set_read_only(true);
                editor.add_buffer_with(buffer, Mode::Help(Help::r(Vec::new())));
            });
        }
        let Some(index) = self.r_help_index() else {
            return;
        };
        if let Some(buffer) = self.buffers.get_mut(index) {
            page.fill(buffer);
        }
        if let Some(Mode::Help(help)) = self.buffers.mode_mut(index) {
            help.set_styles(page.styles);
        }
        self.show_beside(Self::r_help_index);
        self.buffers.current_mut().set_point(0);
        self.reveal_point();
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::test_support::{
        R_PROMPT, lines, press, r_running, r_script, r_sent, type_text,
    };
    use crate::theme::Highlight;

    const HELP: &str = "\u{1}help\u{2}mean   package:base\n\n_\u{8}U_\u{8}s_\u{8}a_\u{8}g_\u{8}e:\n\n     mean(x, ...)\n\u{3}";

    #[test]
    fn control_c_control_v_asks_with_the_symbol_at_point_as_default() {
        let mut editor = r_script("mean(x)\n", 1);
        r_running(&mut editor);

        press(&mut editor, "C-c C-v");
        assert_eq!(
            editor.minibuffer().unwrap().prompt(),
            "R help on (default mean): "
        );
        press(&mut editor, "RET");

        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$help(\"mean\")\n"]
        );
    }

    #[test]
    fn a_typed_topic_wins_over_the_default() {
        let mut editor = r_script("mean(x)\n", 1);
        r_running(&mut editor);

        press(&mut editor, "C-c C-v");
        type_text(&mut editor, "lm");
        press(&mut editor, "RET");

        assert_eq!(
            r_sent(&mut editor),
            ["getOption(\"oxmacs\")$help(\"lm\")\n"]
        );
    }

    #[test]
    fn without_a_symbol_the_prompt_has_no_default() {
        let mut editor = r_script("  \n", 0);
        r_running(&mut editor);

        press(&mut editor, "C-c C-v");
        assert_eq!(editor.minibuffer().unwrap().prompt(), "R help on: ");
        press(&mut editor, "RET");

        assert_eq!(r_sent(&mut editor), Vec::<String>::new());
    }

    #[test]
    fn help_from_r_opens_beside_with_its_headings() {
        let mut editor = r_script("mean(x)\n", 1);
        let job = r_running(&mut editor);

        editor.session_output(job, &format!("{HELP}{R_PROMPT}"));

        assert_eq!(editor.buffer().name(), "*R help*");
        assert!(editor.buffer().is_read_only());
        assert_eq!(
            lines(&editor),
            ["mean   package:base", "", "Usage:", "", "     mean(x, ...)"]
        );
        assert_eq!(editor.line_style(2).spans, [(0..5, Highlight::Heading)]);
        assert_eq!(editor.tiling().windows().len(), 2);
    }

    #[test]
    fn help_lookups_leave_the_console_as_it_was() {
        let mut editor = r_script("mean(x)\n", 1);
        let job = r_running(&mut editor);
        press(&mut editor, "C-c C-v RET");

        editor.session_output(job, &format!("{HELP}{R_PROMPT}"));
        press(&mut editor, "C-x b");
        type_text(&mut editor, "*R*");
        press(&mut editor, "RET");

        assert_eq!(editor.buffer().name(), "*R*");
        assert_eq!(editor.buffer().contents(), "> ");
    }

    #[test]
    fn messages_from_r_show_in_the_echo_area() {
        let mut editor = r_script("x\n", 0);
        let job = r_running(&mut editor);

        editor.session_output(job, "\u{1}message\u{2}No R help for x\u{3}");

        assert_eq!(editor.message(), Some("No R help for x"));
    }
}
