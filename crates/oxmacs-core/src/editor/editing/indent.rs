use crate::editor::Editor;
use crate::syntax::Indentation;

const ELECTRIC: &[char] = &['}', ')', ']'];

impl Editor {
    fn indents(&self) -> bool {
        self.minibuffer.is_none() && (self.buffers.syntax(0).is_some() || self.indenter().is_some())
    }

    fn indentation_rules(&self) -> Option<Indentation> {
        self.buffers
            .language(0)
            .map(|language| language.indentation())
    }

    pub(in crate::editor) fn indent_for_tab(&mut self) {
        if !self.indents() {
            self.insert_over_selection("\t");
            return;
        }
        if self.buffers.current().selection().is_some() {
            self.indent_region();
        } else {
            let line = self.point_line();
            self.indent_line(line);
        }
    }

    pub(in crate::editor) fn indent_region(&mut self) {
        let buffer = self.buffers.current();
        let Some(selection) = buffer.selection() else {
            self.show_message("The mark is not active now");
            return;
        };
        let first = buffer.line_of(selection.start);
        let last = buffer.line_of(selection.end.saturating_sub(1).max(selection.start));
        for line in first..=last {
            if !self.buffers.current().line_text(line).trim().is_empty() {
                self.indent_line(line);
            }
        }
        self.buffers.current_mut().deactivate_mark();
    }

    pub(in crate::editor) fn newline(&mut self) {
        if !self.indents() {
            self.insert_over_selection("\n");
            return;
        }
        self.delete_selection();
        let line = self.point_line();
        if self
            .indentation_rules()
            .is_some_and(|rules| rules.continues)
        {
            self.indent_line(line);
        }
        let buffer = self.buffers.current_mut();
        let point = buffer.point();
        let spaces = buffer
            .chars_before(point)
            .take(buffer.column(point))
            .take_while(|c| matches!(c, ' ' | '\t'))
            .count();
        buffer.delete_range(point - spaces..point);
        let point = point - spaces;
        let around = (
            buffer.chars_before(point).next(),
            buffer.chars_after(point).next(),
        );
        buffer.insert("\n");
        if matches!(
            around,
            (Some('{'), Some('}')) | (Some('('), Some(')')) | (Some('['), Some(']'))
        ) {
            let middle = buffer.point();
            buffer.insert("\n");
            buffer.set_point(middle);
            self.indent_line(line + 2);
        }
        self.indent_line(line + 1);
    }

    pub(in crate::editor) fn reindent_after(&mut self, typed: char) {
        if !self.indents()
            || !ELECTRIC.contains(&typed)
            || self
                .indentation_rules()
                .is_some_and(|rules| rules.guess.is_some())
        {
            return;
        }
        let buffer = self.buffers.current();
        let point = buffer.point();
        let line = buffer.line_of(point);
        let before = buffer.text_in(buffer.line_start(line)..point.saturating_sub(1));
        if before.trim().is_empty() {
            self.indent_line(line);
        }
    }

    fn indent_line(&mut self, line: usize) {
        let target = if let Some(indent) = self.indenter() {
            indent(self, self.buffers.current().id(), line)
        } else {
            self.sync_syntax();
            self.buffers
                .syntax(0)
                .and_then(|syntax| syntax.indentation(self.buffers.current(), line, self.tab_width))
        };
        let Some(target) = target else {
            return;
        };
        let buffer = self.buffers.current_mut();
        let start = buffer.line_start(line);
        let text = buffer.line_text(line);
        let leading = text.chars().take_while(|c| matches!(c, ' ' | '\t')).count();
        let wanted = " ".repeat(target);
        if text.chars().take(leading).eq(wanted.chars()) {
            if buffer.point() >= start && buffer.point() < start + leading {
                buffer.set_point(start + leading);
            }
            return;
        }
        let point = buffer.point();
        let moved = if point < start {
            point
        } else if point <= start + leading {
            start + target
        } else {
            point - leading + target
        };
        buffer.delete_range(start..start + leading);
        buffer.set_point(start);
        buffer.insert(&wanted);
        buffer.set_point(moved);
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, lines, press, type_text};
    use crate::syntax::{Grammar, Indentation, Language, LanguageSpec, Outline};

    fn rust(text: &str, point: usize) -> Editor {
        let mut editor = editor_with(text, point);
        editor.set_major_mode(Some(Language::RUST));
        editor
    }

    static GUESSING_GRAMMAR: Grammar =
        Grammar::new("toml", || tree_sitter_toml_ng::LANGUAGE.into(), "", "");

    static GUESSING: LanguageSpec = LanguageSpec {
        name: "Guessing",
        extensions: &[],
        file_names: &[],
        aliases: &[],
        line_comment: None,
        pairs: &[],
        indentation: Indentation {
            guess: Some(|_, _, line, _| Some(line * 2)),
            ..Indentation::KEEP
        },
        grammar: Some(&GUESSING_GRAMMAR),
        grammars: &[],
        outline: Outline::NONE,
        features: &[],
    };

    #[test]
    fn languages_can_guess_the_indentation_of_each_line() {
        let mut editor = editor_with("a\nb", 2);
        editor.set_major_mode(Some(Language::new(&GUESSING)));

        press(&mut editor, "TAB C-e RET");

        assert_eq!(lines(&editor), ["a", "  b", "    "]);
    }

    #[test]
    fn return_leaves_the_line_alone_where_indentation_is_kept() {
        let mut editor = editor_with("[a]\n  x = 1", 11);
        editor.set_major_mode(Some(Language::TOML));

        press(&mut editor, "RET");

        assert_eq!(lines(&editor), ["[a]", "  x = 1", "  "]);
    }

    #[test]
    fn tab_indents_the_line_wherever_point_is() {
        let mut editor = rust("fn a() {\nlet x = 1;\n}", 13);

        press(&mut editor, "TAB");

        assert_eq!(lines(&editor)[1], "    let x = 1;");
        assert_eq!(editor.buffer().point(), 17);
    }

    #[test]
    fn tab_in_the_indentation_moves_to_the_text() {
        let mut editor = rust("fn a() {\n    let x = 1;\n}", 10);

        press(&mut editor, "TAB");

        assert_eq!(editor.buffer().point(), 13);
        assert!(!editor.buffer().is_modified());
    }

    #[test]
    fn tab_inserts_a_tab_in_plain_text() {
        let mut editor = editor_with("ab", 1);

        press(&mut editor, "TAB");

        assert_eq!(editor.buffer().contents(), "a\tb");
    }

    #[test]
    fn return_indents_the_new_line() {
        let mut editor = rust("fn a() {   ", 11);

        press(&mut editor, "RET");

        assert_eq!(editor.buffer().contents(), "fn a() {\n    ");
        assert_eq!(editor.buffer().point(), 13);
    }

    #[test]
    fn return_reindents_the_line_it_leaves() {
        let mut editor = rust("fn a() {\n  let x = 1;", 21);

        press(&mut editor, "RET");

        assert_eq!(lines(&editor), ["fn a() {", "    let x = 1;", "    "]);
    }

    #[test]
    fn closing_brackets_reindent_their_line() {
        let mut editor = rust("fn a() {\n    x();\n    ", 22);

        type_text(&mut editor, "}");

        assert_eq!(lines(&editor)[2], "}");
        assert_eq!(editor.buffer().point(), 19);
    }

    #[test]
    fn closing_brackets_after_text_leave_the_line_alone() {
        let mut editor = rust("fn a() {\n    x(", 15);

        type_text(&mut editor, ")");

        assert_eq!(lines(&editor)[1], "    x()");
    }

    #[test]
    fn indent_region_fixes_every_selected_line() {
        let mut editor = rust("fn a() {\nif b {\nc();\n}\n}\n", 0);

        press(&mut editor, "C-x h C-M-\\");

        assert_eq!(
            lines(&editor),
            ["fn a() {", "    if b {", "        c();", "    }", "}"]
        );
        assert_eq!(editor.buffer().selection(), None);
    }

    #[test]
    fn tab_with_a_selection_indents_it() {
        let mut editor = rust("fn a() {\nb();\nc();\n}\n", 0);

        press(&mut editor, "C-x h TAB");

        assert_eq!(lines(&editor), ["fn a() {", "    b();", "    c();", "}"]);
    }

    #[test]
    fn indenting_undoes_in_one_step() {
        let mut editor = rust("fn a() {\nif b {\nc();\n}\n}\n", 0);
        press(&mut editor, "C-x h C-M-\\");

        press(&mut editor, "C-/");

        assert_eq!(editor.buffer().contents(), "fn a() {\nif b {\nc();\n}\n}\n");
    }
}
