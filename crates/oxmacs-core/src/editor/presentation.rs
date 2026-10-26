use std::ops::Range;

use crate::display::Layout;
use crate::editor::windows::pane::Pane;
use crate::editor::{Editor, Effect, tab_width_in};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is its own setting"
)]
pub struct Presentation {
    pub column: Option<usize>,
    pub hide_chrome: bool,
    pub dim: bool,
    pub typewriter: bool,
    pub fullscreen: bool,
}

impl Editor {
    pub fn presentation(&self) -> Presentation {
        self.presentation
    }

    pub fn set_presentation(&mut self, presentation: Presentation) {
        if presentation.fullscreen != self.presentation.fullscreen {
            self.effects
                .push(Effect::Fullscreen(presentation.fullscreen));
        }
        self.presentation = presentation;
        self.reveal_point();
    }

    pub(super) fn center_point(&mut self) {
        let buffer = self.buffers.current();
        let layout = Layout::new(
            buffer,
            self.viewport.wrap_width(),
            tab_width_in(buffer, self.tab_width),
        );
        let spot = layout.screen_line(buffer.point());
        let half = (self.viewport.rows() / 2).cast_signed();
        self.viewport.set_top(layout.step(spot, -half));
    }
}

fn blank(pane: &Pane<'_>, line: usize) -> bool {
    pane.buffer().line_text(line).trim().is_empty()
}

impl Pane<'_> {
    pub fn focus_lines(&self) -> Option<Range<usize>> {
        if !self.editor().presentation.dim || !self.is_selected() {
            return None;
        }
        let buffer = self.buffer();
        let here = buffer.line_of(self.point());
        if blank(self, here) {
            return Some(here..here + 1);
        }
        let mut start = here;
        while start > 0 && !blank(self, start - 1) {
            start -= 1;
        }
        let mut end = here + 1;
        while end < buffer.len_lines() && !blank(self, end) {
            end += 1;
        }
        Some(start..end)
    }
}

#[cfg(test)]
mod tests {
    use super::Presentation;
    use crate::editor::{Editor, Effect};
    use crate::testing::{editor_with, press};

    fn numbered(count: usize) -> Editor {
        let text: Vec<String> = (0..count).map(|line| line.to_string()).collect();
        let mut editor = editor_with(&text.join("\n"), 0);
        editor.resize_window(10, 40);
        editor
    }

    #[test]
    fn typewriter_scrolling_keeps_point_in_the_middle() {
        let mut editor = numbered(100);
        editor.set_presentation(Presentation {
            typewriter: true,
            ..Presentation::default()
        });

        for _ in 0..20 {
            press(&mut editor, "C-n");
        }

        assert_eq!(editor.viewport().top().line, 15);
        press(&mut editor, "C-p");
        assert_eq!(editor.viewport().top().line, 14);
    }

    #[test]
    fn dimming_marks_the_paragraph_around_point_in_the_selected_window() {
        let mut editor = editor_with("a\nb\n\nc\nd\n\ne", 6);
        assert_eq!(editor.pane().focus_lines(), None);

        editor.set_presentation(Presentation {
            dim: true,
            ..Presentation::default()
        });

        assert_eq!(editor.pane().focus_lines(), Some(3..5));
        press(&mut editor, "M->");
        assert_eq!(editor.pane().focus_lines(), Some(6..7));
    }

    #[test]
    fn hidden_chrome_hides_line_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, "one\ntwo\n").unwrap();
        let mut editor = Editor::new();
        editor.open_file(&path);
        assert!(editor.pane().gutter().is_some());

        editor.set_presentation(Presentation {
            hide_chrome: true,
            ..Presentation::default()
        });

        assert!(editor.pane().gutter().is_none());
        assert!(editor.presentation().hide_chrome);
    }

    #[test]
    fn fullscreen_is_asked_for_once_each_way() {
        let mut editor = Editor::new();
        let fullscreen = Presentation {
            fullscreen: true,
            column: Some(72),
            ..Presentation::default()
        };

        editor.set_presentation(fullscreen);
        editor.set_presentation(fullscreen);
        editor.set_presentation(Presentation::default());

        let asked: Vec<Effect> = editor
            .take_effects()
            .into_iter()
            .filter(|effect| matches!(effect, Effect::Fullscreen(_)))
            .collect();
        assert_eq!(asked, [Effect::Fullscreen(true), Effect::Fullscreen(false)]);
    }
}
