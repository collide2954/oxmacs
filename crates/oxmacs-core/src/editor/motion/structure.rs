use crate::editor::Editor;
use crate::editor::editing::pairs::{Inert, scan_partner};
use crate::syntax::motion::PREMATURE;
use crate::text::{Buffer, motion};

impl Editor {
    fn go_to(&mut self, target: Result<usize, &'static str>) {
        match target {
            Ok(position) => self.buffers.current_mut().set_point(position),
            Err(message) => self.show_message(message),
        }
    }

    pub(in crate::editor) fn beginning_of_defun(&mut self) {
        self.sync_syntax();
        let buffer = self.buffers.current();
        let target = self
            .buffers
            .syntax(0)
            .and_then(|syntax| syntax.defun_start_before(buffer, buffer.point()))
            .unwrap_or(0);
        self.buffers.current_mut().set_point(target);
    }

    pub(in crate::editor) fn end_of_defun(&mut self) {
        self.sync_syntax();
        let buffer = self.buffers.current();
        let target = self
            .buffers
            .syntax(0)
            .and_then(|syntax| syntax.defun_end_after(buffer, buffer.point()))
            .unwrap_or(buffer.len_chars());
        let line = buffer.line_of(target);
        let rest_blank = buffer
            .text_in(target..buffer.line_end(line))
            .trim()
            .is_empty();
        let target = if rest_blank && line + 1 < buffer.len_lines() {
            buffer.line_start(line + 1)
        } else {
            target
        };
        self.buffers.current_mut().set_point(target);
    }

    pub(in crate::editor) fn forward_sexp(&mut self) {
        self.sync_syntax();
        let buffer = self.buffers.current();
        let point = buffer.point();
        let target = self.buffers.syntax(0).map_or_else(
            || plain_sexp_end(buffer, point, &mut self.inert(0)),
            |syntax| syntax.sexp_end(buffer, point),
        );
        self.go_to(target);
    }

    pub(in crate::editor) fn backward_sexp(&mut self) {
        self.sync_syntax();
        let buffer = self.buffers.current();
        let point = buffer.point();
        let target = self.buffers.syntax(0).map_or_else(
            || plain_sexp_start(buffer, point, &mut self.inert(0)),
            |syntax| syntax.sexp_start(buffer, point),
        );
        self.go_to(target);
    }

    pub(in crate::editor) fn backward_up_list(&mut self) {
        self.sync_syntax();
        let buffer = self.buffers.current();
        let point = buffer.point();
        let target = self.buffers.syntax(0).map_or_else(
            || plain_enclosing_opener(buffer, point, &mut self.inert(0)),
            |syntax| syntax.enclosing_opener(buffer, point),
        );
        self.go_to(target.ok_or("At top level"));
    }

    pub(in crate::editor) fn down_list(&mut self) {
        self.sync_syntax();
        let buffer = self.buffers.current();
        let point = buffer.point();
        let target = if let Some(syntax) = self.buffers.syntax(0) {
            syntax.next_opener(buffer, point)
        } else {
            let mut inert = self.inert(0);
            buffer
                .chars_after(point)
                .enumerate()
                .find(|(offset, c)| matches!(c, '(' | '[' | '{') && !inert.covers(point + offset))
                .map(|(offset, _)| point + offset)
        };
        self.go_to(target.map(|opener| opener + 1).ok_or(PREMATURE));
    }
}

fn plain_sexp_end(
    buffer: &Buffer,
    point: usize,
    inert: &mut Inert<'_>,
) -> Result<usize, &'static str> {
    let from = point
        + buffer
            .chars_after(point)
            .take_while(|c| c.is_whitespace())
            .count();
    match buffer.chars_after(from).next() {
        Some('(' | '[' | '{') => scan_partner(buffer, from, inert)
            .map(|end| end + 1)
            .ok_or(PREMATURE),
        Some(')' | ']' | '}') | None => Err(PREMATURE),
        Some(_) => Ok(motion::forward_word(buffer, point)),
    }
}

fn plain_sexp_start(
    buffer: &Buffer,
    point: usize,
    inert: &mut Inert<'_>,
) -> Result<usize, &'static str> {
    let from = point
        - buffer
            .chars_before(point)
            .take_while(|c| c.is_whitespace())
            .count();
    match buffer.chars_before(from).next() {
        Some(')' | ']' | '}') => scan_partner(buffer, from - 1, inert).ok_or(PREMATURE),
        Some('(' | '[' | '{') | None => Err(PREMATURE),
        Some(_) => Ok(motion::backward_word(buffer, point)),
    }
}

fn plain_enclosing_opener(buffer: &Buffer, point: usize, inert: &mut Inert<'_>) -> Option<usize> {
    let mut depth = 0_usize;
    for (position, c) in (0..point).rev().zip(buffer.chars_before(point)) {
        if matches!(c, '(' | ')' | '[' | ']' | '{' | '}') && inert.covers(position) {
            continue;
        }
        match c {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' if depth == 0 => return Some(position),
            '(' | '[' | '{' => depth -= 1,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::syntax::Language;
    use crate::testing::{editor_with, press};

    const SOURCE: &str = "fn a() {\n    x(1, \"b c\");\n}\n\nfn b() {}\n";

    fn rust(point: usize) -> Editor {
        let mut editor = editor_with(SOURCE, point);
        editor.set_major_mode(Some(Language::rust()));
        editor
    }

    fn after(editor: &mut Editor, keys: &str) -> usize {
        press(editor, keys);
        editor.buffer().point()
    }

    #[test]
    fn defun_motions_move_between_items() {
        let mut editor = rust(15);

        assert_eq!(after(&mut editor, "C-M-a"), 0);
        assert_eq!(after(&mut editor, "C-M-e"), 28);
        assert_eq!(after(&mut editor, "C-M-e"), 39);
        assert_eq!(after(&mut editor, "C-M-a"), 29);
        assert_eq!(after(&mut editor, "C-M-a"), 0);
    }

    #[test]
    fn sexp_motions_move_over_balanced_expressions() {
        let mut editor = rust(13);

        assert_eq!(after(&mut editor, "C-M-f"), 14);
        assert_eq!(after(&mut editor, "C-M-f"), 24);
        assert_eq!(after(&mut editor, "C-M-b"), 14);
        assert_eq!(after(&mut editor, "C-M-b"), 13);
    }

    #[test]
    fn strings_and_comments_are_whole_expressions() {
        let mut editor = rust(18);

        assert_eq!(after(&mut editor, "C-M-f"), 23);

        let mut comment = editor_with("// note\nfoo", 0);
        comment.set_major_mode(Some(Language::rust()));
        assert_eq!(after(&mut comment, "C-M-f"), 11);
    }

    #[test]
    fn sexp_motions_stop_at_the_end_of_their_list() {
        let mut editor = rust(23);

        press(&mut editor, "C-M-f");

        assert_eq!(editor.buffer().point(), 23);
        assert_eq!(
            editor.message(),
            Some("Containing expression ends prematurely")
        );
    }

    #[test]
    fn list_motions_move_out_of_and_into_brackets() {
        let mut editor = rust(19);

        assert_eq!(after(&mut editor, "C-M-u"), 14);
        assert_eq!(after(&mut editor, "C-M-u"), 7);
        assert_eq!(after(&mut editor, "C-M-u"), 7);
        assert_eq!(editor.message(), Some("At top level"));

        let mut editor = rust(0);
        assert_eq!(after(&mut editor, "C-M-d"), 5);
    }

    const R_SOURCE: &str = "f <- function(x) {\n  g(x, \"a b\")\n}\n\nh <- function() NULL\n";

    fn r(point: usize) -> Editor {
        let mut editor = editor_with(R_SOURCE, point);
        editor.set_major_mode(Some(Language::r()));
        editor
    }

    #[test]
    fn r_defun_motions_move_between_function_assignments() {
        let mut editor = r(25);

        assert_eq!(after(&mut editor, "C-M-a"), 0);
        assert_eq!(after(&mut editor, "C-M-e"), 35);
        assert_eq!(after(&mut editor, "C-M-e"), 57);
        assert_eq!(after(&mut editor, "C-M-a"), 36);
        assert_eq!(after(&mut editor, "C-M-a"), 0);
    }

    #[test]
    fn r_sexp_motions_skip_hash_comments() {
        let mut editor = editor_with("# note\nfoo", 0);
        editor.set_major_mode(Some(Language::r()));

        assert_eq!(after(&mut editor, "C-M-f"), 10);
    }

    #[test]
    fn plain_text_moves_by_brackets_and_words() {
        let mut editor = editor_with("(a b) c", 0);

        assert_eq!(after(&mut editor, "C-M-f"), 5);
        assert_eq!(after(&mut editor, "C-M-b"), 0);
        assert_eq!(after(&mut editor, "C-M-d"), 1);
        assert_eq!(after(&mut editor, "C-M-u"), 0);
    }
}
