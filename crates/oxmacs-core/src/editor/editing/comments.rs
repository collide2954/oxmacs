use std::ops::RangeInclusive;

use crate::editor::Editor;
use crate::syntax::Language;

impl Editor {
    fn comment_prefix(&mut self) -> Option<&'static str> {
        let prefix = self.language().and_then(Language::line_comment);
        if prefix.is_none() {
            self.show_message("No comment syntax is defined");
        }
        prefix
    }

    fn selected_lines(&self) -> Option<RangeInclusive<usize>> {
        let buffer = self.buffers.current();
        let selection = buffer.selection()?;
        let first = buffer.line_of(selection.start);
        let mut last = buffer.line_of(selection.end);
        if last > first && buffer.column(selection.end) == 0 {
            last -= 1;
        }
        Some(first..=last)
    }

    pub(in crate::editor) fn comment_line(&mut self) {
        let Some(prefix) = self.comment_prefix() else {
            return;
        };
        if let Some(lines) = self.selected_lines() {
            self.toggle_comments(lines, prefix);
            return;
        }
        let line = self.point_line();
        self.toggle_comments(line..=line, prefix);
        let buffer = self.buffers.current_mut();
        buffer.set_point(buffer.line_start(line + 1));
    }

    pub(in crate::editor) fn comment_dwim(&mut self) {
        let Some(prefix) = self.comment_prefix() else {
            return;
        };
        if let Some(lines) = self.selected_lines() {
            self.toggle_comments(lines, prefix);
            return;
        }
        let line = self.point_line();
        if self.buffers.current().line_text(line).trim().is_empty() {
            self.indent_for_tab();
            let buffer = self.buffers.current_mut();
            buffer.set_point(buffer.line_end(line));
            buffer.insert(&format!("{prefix} "));
            return;
        }
        self.sync_syntax();
        let buffer = self.buffers.current();
        let comment = self
            .buffers
            .syntax(0)
            .and_then(|syntax| syntax.comment_start(buffer, line));
        let buffer = self.buffers.current_mut();
        if let Some(start) = comment {
            let text = buffer.line_text(line);
            let after: usize = text
                .chars()
                .skip(start + prefix.len())
                .take_while(|c| *c == ' ')
                .count();
            buffer.set_point(buffer.line_start(line) + start + prefix.len() + after);
            return;
        }
        let end = buffer.line_end(line);
        let spaces = buffer
            .chars_before(end)
            .take_while(|c| matches!(c, ' ' | '\t'))
            .count();
        buffer.delete_range(end - spaces..end);
        buffer.set_point(end - spaces);
        buffer.insert(&format!(" {prefix} "));
    }

    fn toggle_comments(&mut self, lines: RangeInclusive<usize>, prefix: &str) {
        let buffer = self.buffers.current_mut();
        let filled: Vec<usize> = lines
            .filter(|line| !buffer.line_text(*line).trim().is_empty())
            .collect();
        let commented = filled
            .iter()
            .all(|line| buffer.line_text(*line).trim_start().starts_with(prefix));
        if commented {
            for line in filled {
                let text = buffer.line_text(line);
                let indent = leading(&text);
                let rest: String = text.chars().skip(indent + prefix.chars().count()).collect();
                let width = prefix.chars().count() + usize::from(rest.starts_with(' '));
                let start = buffer.line_start(line) + indent;
                buffer.delete_range(start..start + width);
            }
            return;
        }
        let column = filled
            .iter()
            .map(|line| leading(&buffer.line_text(*line)))
            .min()
            .unwrap_or(0);
        for line in filled {
            let at = buffer.line_start(line) + column;
            buffer.insert_at(at, &format!("{prefix} "));
        }
    }
}

fn leading(text: &str) -> usize {
    text.chars().take_while(|c| matches!(c, ' ' | '\t')).count()
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::syntax::Language;
    use crate::testing::{editor_with, lines, press};

    fn rust(text: &str, point: usize) -> Editor {
        let mut editor = editor_with(text, point);
        editor.set_major_mode(Some(Language::RUST));
        editor
    }

    #[test]
    fn comment_line_toggles_the_line_and_moves_down() {
        let mut editor = rust("fn a() {\n    b();\n}", 12);

        press(&mut editor, "C-x C-;");
        assert_eq!(lines(&editor), ["fn a() {", "    // b();", "}"]);
        assert_eq!(editor.buffer().point(), 21);

        press(&mut editor, "C-p C-x C-;");
        assert_eq!(lines(&editor), ["fn a() {", "    b();", "}"]);
    }

    #[test]
    fn selected_lines_are_commented_at_their_shallowest_indentation() {
        let mut editor = rust("fn a() {\n    b();\n\n}\n", 0);

        press(&mut editor, "C-x h C-x C-;");
        assert_eq!(lines(&editor), ["// fn a() {", "//     b();", "", "// }"]);
        assert!(editor.buffer().selection().is_some());

        press(&mut editor, "s-/");
        assert_eq!(lines(&editor), ["fn a() {", "    b();", "", "}"]);
    }

    #[test]
    fn meta_semicolon_adds_a_comment_at_the_end_of_the_line() {
        let mut editor = rust("let x = 1;  ", 0);

        press(&mut editor, "M-;");

        assert_eq!(editor.buffer().contents(), "let x = 1; // ");
        assert_eq!(editor.buffer().point(), 14);
    }

    #[test]
    fn meta_semicolon_on_a_blank_line_starts_an_indented_comment() {
        let mut editor = rust("fn a() {\n\n}", 9);

        press(&mut editor, "M-;");

        assert_eq!(lines(&editor), ["fn a() {", "    // ", "}"]);
        assert_eq!(editor.buffer().point(), 16);
    }

    #[test]
    fn meta_semicolon_on_a_comment_moves_to_its_text() {
        let mut editor = rust("x(); // note", 0);

        press(&mut editor, "M-;");

        assert_eq!(editor.buffer().point(), 8);
        assert!(!editor.buffer().is_modified());
    }

    #[test]
    fn meta_semicolon_finds_hash_comments_in_r_and_toml() {
        for language in [Language::R, Language::TOML] {
            let mut editor = editor_with("x = 1 # one", 0);
            editor.set_major_mode(Some(language));

            press(&mut editor, "M-;");

            assert_eq!(editor.buffer().point(), 8, "{language:?}");
            assert!(!editor.buffer().is_modified(), "{language:?}");
        }
    }

    #[test]
    fn r_comments_toggle_with_hashes() {
        let mut editor = editor_with("f <- function() {\n  x\n}\n", 20);
        editor.set_major_mode(Some(Language::R));

        press(&mut editor, "C-x C-;");

        assert_eq!(lines(&editor), ["f <- function() {", "  # x", "}"]);
    }

    #[test]
    fn plain_text_has_no_comment_syntax() {
        let mut editor = editor_with("a", 0);

        press(&mut editor, "M-;");

        assert_eq!(editor.message(), Some("No comment syntax is defined"));
        assert_eq!(editor.buffer().contents(), "a");
    }

    #[test]
    fn commenting_a_region_undoes_in_one_step() {
        let mut editor = rust("a();\nb();\n", 0);
        press(&mut editor, "C-x h C-x C-;");

        press(&mut editor, "C-/");

        assert_eq!(editor.buffer().contents(), "a();\nb();\n");
    }
}
