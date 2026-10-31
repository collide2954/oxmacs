use crate::editor::Editor;
use crate::text::BufferId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::editor) struct Suggestion {
    buffer: BufferId,
    at: usize,
    revision: u64,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ghost {
    pub line: usize,
    pub column: usize,
    pub lines: Vec<String>,
}

impl Editor {
    pub fn suggest(&mut self, text: Option<String>) {
        let buffer = self.buffer();
        self.suggestion = text.filter(|text| !text.is_empty()).map(|text| Suggestion {
            buffer: buffer.id(),
            at: buffer.point(),
            revision: buffer.revision(),
            text,
        });
    }

    pub fn suggestion(&self) -> Option<&str> {
        let shown = self.suggestion.as_ref()?;
        let buffer = self.buffer();
        let current = buffer.id() == shown.buffer
            && buffer.point() == shown.at
            && buffer.revision() == shown.revision
            && buffer.selection().is_none();
        current.then_some(shown.text.as_str())
    }

    pub(in crate::editor) fn ghost(&self) -> Option<Ghost> {
        let text = self.suggestion()?;
        let buffer = self.buffer();
        let line = buffer.line_of(buffer.point());
        Some(Ghost {
            line,
            column: buffer.point() - buffer.line_start(line),
            lines: text.split('\n').map(str::to_owned).collect(),
        })
    }

    pub(in crate::editor) fn forget_stale_suggestion(&mut self) {
        if self.suggestion().is_none() {
            self.suggestion = None;
        }
    }

    pub(in crate::editor) fn accept_suggestion(&mut self) {
        let Some(text) = self.suggestion().map(str::to_owned) else {
            return;
        };
        self.suggestion = None;
        self.current_mut().insert(&text);
    }

    pub(in crate::editor) fn suggestion_after(&self, typed: char) -> Option<String> {
        let mut rest = self.suggestion()?.chars();
        (rest.next() == Some(typed)).then(|| rest.as_str().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::testing::press;

    fn typed(text: &str) -> Editor {
        let mut editor = Editor::new();
        editor.buffer_mut().insert(text);
        editor
    }

    #[test]
    fn tab_takes_the_suggestion_shown_at_point() {
        let mut editor = typed("let x = ");
        editor.suggest(Some("42;\nlet y = 1;".to_owned()));
        assert_eq!(editor.suggestion(), Some("42;\nlet y = 1;"));

        press(&mut editor, "TAB");

        assert_eq!(editor.buffer().contents(), "let x = 42;\nlet y = 1;");
        assert_eq!(editor.suggestion(), None);
        press(&mut editor, "C-/");
        assert_eq!(editor.buffer().contents(), "let x = ");
    }

    #[test]
    fn typing_what_the_suggestion_says_keeps_the_rest_of_it() {
        let mut editor = typed("fn ma");
        editor.suggest(Some("in() {}".to_owned()));

        press(&mut editor, "i n");

        assert_eq!(editor.suggestion(), Some("() {}"));
        press(&mut editor, "x");
        assert_eq!(editor.suggestion(), None);
    }

    #[test]
    fn moving_away_drops_the_suggestion_and_tab_does_its_usual_job() {
        let mut editor = typed("ab");
        editor.suggest(Some("cd".to_owned()));

        press(&mut editor, "C-b");
        assert_eq!(editor.suggestion(), None);
        press(&mut editor, "C-f");
        assert_eq!(editor.suggestion(), None);
        press(&mut editor, "TAB");
        assert_ne!(editor.buffer().contents(), "abcd");
    }

    #[test]
    fn the_ghost_says_where_the_suggestion_goes_line_by_line() {
        let mut editor = typed("one\n  two");
        editor.suggest(Some("x\ny".to_owned()));

        assert_eq!(
            editor.ghost(),
            Some(super::Ghost {
                line: 1,
                column: 5,
                lines: vec!["x".to_owned(), "y".to_owned()],
            })
        );
    }
}
