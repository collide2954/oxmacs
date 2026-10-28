use crate::config::LineNumbers;
use crate::editor::Editor;

const MINIMUM_WIDTH: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gutter {
    style: LineNumbers,
    current: usize,
    width: usize,
    separator: bool,
}

impl Gutter {
    pub(in crate::editor) fn new(
        style: LineNumbers,
        current: usize,
        lines: usize,
        separator: bool,
    ) -> Self {
        Self {
            style,
            current,
            width: lines.to_string().len().max(MINIMUM_WIDTH),
            separator,
        }
    }

    pub fn has_separator(&self) -> bool {
        self.separator
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn is_current(&self, line: usize) -> bool {
        line == self.current
    }

    pub fn label(&self, line: usize) -> String {
        let number = match self.style {
            LineNumbers::Relative if line != self.current => line.abs_diff(self.current),
            LineNumbers::Absolute | LineNumbers::Relative | LineNumbers::Off => line + 1,
        };
        format!("{number:>width$}", width = self.width)
    }
}

impl Editor {
    pub(in crate::editor) fn line_numbers_at(&self, index: usize) -> Option<LineNumbers> {
        let configured = self.config.line_numbers;
        let visiting = !self.buffers.is_special(index)
            && self
                .buffers
                .get(index)
                .is_some_and(|buffer| buffer.file().is_some());
        let shown = self
            .buffers
            .numbers(index)
            .unwrap_or(visiting && configured != LineNumbers::Off);
        shown.then_some(match configured {
            LineNumbers::Off => LineNumbers::Absolute,
            style => style,
        })
    }

    pub(in crate::editor) fn display_line_numbers_mode(&mut self) {
        let on = self.line_numbers_at(0).is_none();
        self.buffers.set_numbers(0, on);
        self.show_message(if on {
            "Display-Line-Numbers mode enabled in current buffer"
        } else {
            "Display-Line-Numbers mode disabled in current buffer"
        });
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{Value, json};

    use super::Gutter;
    use crate::config::{Config, LineNumbers};
    use crate::editor::Editor;
    use crate::testing::{press, type_text};

    fn visiting(lines: usize, config: &Value) -> (tempfile::TempDir, Editor) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        fs::write(&path, "line\n".repeat(lines)).unwrap();
        let mut editor = Editor::new();
        editor.apply_config(&Config::from_settings(config).unwrap());
        editor.open_file(&path);
        (dir, editor)
    }

    #[test]
    fn file_buffers_number_their_lines() {
        let (_dir, editor) = visiting(3, &json!({}));

        let gutter = editor.pane().gutter().unwrap();

        assert_eq!(gutter.width(), 2);
        assert_eq!(gutter.label(0), " 1");
        assert!(gutter.is_current(0));
        assert!(!gutter.is_current(1));
    }

    #[test]
    fn the_separator_is_drawn_only_when_configured() {
        let (_dir, editor) = visiting(3, &json!({}));
        assert!(!editor.pane().gutter().unwrap().has_separator());

        let (_dir, editor) = visiting(3, &json!({"line-number-separator": true}));
        assert!(editor.pane().gutter().unwrap().has_separator());
    }

    #[test]
    fn the_gutter_widens_with_the_line_count() {
        let (_dir, editor) = visiting(120, &json!({}));

        assert_eq!(editor.pane().gutter().unwrap().label(4), "  5");
    }

    #[test]
    fn relative_numbers_count_from_the_current_line() {
        let (_dir, mut editor) = visiting(6, &json!({"line-numbers": "relative"}));
        press(&mut editor, "C-n C-n");

        let gutter = editor.pane().gutter().unwrap();

        assert_eq!(
            (0..5).map(|line| gutter.label(line)).collect::<Vec<_>>(),
            [" 2", " 1", " 3", " 1", " 2"]
        );
    }

    #[test]
    fn scratch_and_special_buffers_go_without() {
        let mut editor = Editor::new();
        assert_eq!(editor.pane().gutter(), None);

        editor.about();
        assert_eq!(editor.pane().gutter(), None);
    }

    #[test]
    fn the_setting_can_turn_numbers_off() {
        let (_dir, mut editor) = visiting(3, &json!({}));

        editor.reconfigure(
            &Config::default()
                .with("line-numbers", json!("off"))
                .unwrap(),
        );

        assert_eq!(editor.pane().gutter(), None);
    }

    #[test]
    fn the_mode_toggles_numbers_in_one_buffer() {
        let mut editor = Editor::new();

        press(&mut editor, "M-x");
        type_text(&mut editor, "display-line-numbers-mode");
        press(&mut editor, "RET");
        assert_eq!(
            editor.pane().gutter(),
            Some(Gutter::new(LineNumbers::Absolute, 0, 1, false))
        );
        assert_eq!(
            editor.message(),
            Some("Display-Line-Numbers mode enabled in current buffer")
        );

        press(&mut editor, "M-x");
        type_text(&mut editor, "display-line-numbers-mode");
        press(&mut editor, "RET");
        assert_eq!(editor.pane().gutter(), None);
    }
}
