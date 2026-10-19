use crate::command::Command;
use crate::display::Layout;
use crate::editor::Editor;
use crate::text::BufferId;

const VISIBLE: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupView {
    pub items: Vec<String>,
    pub selected: usize,
    pub column: usize,
    pub row: isize,
}

#[derive(Debug)]
pub(in crate::editor) struct Popup {
    buffer: BufferId,
    start: usize,
    candidates: Vec<String>,
    matching: Vec<usize>,
    selected: usize,
}

impl Popup {
    fn filter(&mut self, typed: &str) {
        self.matching = (0..self.candidates.len())
            .filter(|index| self.candidates[*index].starts_with(typed))
            .collect();
        self.selected = 0;
    }

    fn chosen(&self) -> Option<&str> {
        let index = *self.matching.get(self.selected)?;
        Some(&self.candidates[index])
    }

    fn visible(&self) -> (Vec<String>, usize) {
        let first = (self.selected + 1).saturating_sub(VISIBLE);
        let items = self.matching[first..]
            .iter()
            .take(VISIBLE)
            .map(|index| self.candidates[*index].clone())
            .collect();
        (items, self.selected - first)
    }
}

impl Editor {
    pub(in crate::editor) fn open_popup(&mut self, start: usize, candidates: Vec<String>) {
        let buffer = self.buffers.current();
        let mut popup = Popup {
            buffer: buffer.id(),
            start,
            candidates,
            matching: Vec::new(),
            selected: 0,
        };
        popup.filter(&buffer.text_in(start..buffer.point()));
        self.popup = (!popup.matching.is_empty()).then_some(popup);
    }

    pub(in crate::editor) fn popup_step(&mut self, forward: bool) {
        let Some(popup) = &mut self.popup else {
            return;
        };
        let count = popup.matching.len();
        popup.selected = if forward {
            (popup.selected + 1) % count
        } else {
            (popup.selected + count - 1) % count
        };
    }

    pub(in crate::editor) fn popup_accept(&mut self) {
        let Some(popup) = self.popup.take() else {
            return;
        };
        let Some(chosen) = popup.chosen() else {
            return;
        };
        let buffer = self.buffers.current_mut();
        let point = buffer.point();
        buffer.delete_range(popup.start..point);
        buffer.set_point(popup.start);
        buffer.insert(chosen);
    }

    pub(in crate::editor) fn popup_quit(&mut self) {
        self.popup = None;
    }

    pub(in crate::editor) fn refresh_popup(&mut self, command: Command) {
        if matches!(
            command,
            Command::CompletionNext | Command::CompletionPrevious
        ) {
            return;
        }
        let Some(popup) = &mut self.popup else {
            return;
        };
        let buffer = self.buffers.current();
        let point = buffer.point();
        let typed = (point >= popup.start).then(|| buffer.text_in(popup.start..point));
        let still = self.minibuffer.is_none()
            && buffer.id() == popup.buffer
            && typed
                .as_ref()
                .is_some_and(|typed| !typed.contains(char::is_whitespace));
        if let Some(typed) = typed.filter(|_| still) {
            popup.filter(&typed);
        }
        if !still || popup.matching.is_empty() {
            self.popup = None;
        }
    }

    pub(in crate::editor) fn popup_view(
        &self,
        buffer: BufferId,
        layout: &Layout<'_>,
        top: crate::display::ScreenLine,
        rows: usize,
    ) -> Option<PopupView> {
        let popup = self.popup.as_ref().filter(|popup| popup.buffer == buffer)?;
        let spot = layout.screen_line(popup.start);
        let row = layout.distance(top, spot, rows).unwrap_or(0);
        let (items, selected) = popup.visible();
        Some(PopupView {
            items,
            selected,
            column: layout.column(popup.start),
            row,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::editor::test_support::{editor_with, press, type_text};

    fn offered(text: &str, point: usize, start: usize, candidates: &[&str]) -> Editor {
        let mut editor = editor_with(text, point);
        let candidates = candidates.iter().map(|c| (*c).to_owned()).collect();
        editor.open_popup(start, candidates);
        editor
    }

    fn shown(editor: &Editor) -> Option<(Vec<String>, usize)> {
        editor
            .pane()
            .popup()
            .map(|view| (view.items, view.selected))
    }

    fn items(names: &[&str], selected: usize) -> (Vec<String>, usize) {
        (
            names.iter().map(|name| (*name).to_owned()).collect(),
            selected,
        )
    }

    #[test]
    fn the_popup_lists_the_candidates_matching_what_was_typed() {
        let editor = offered("x <- pr", 7, 5, &["print", "prod", "max"]);

        assert_eq!(shown(&editor), Some(items(&["print", "prod"], 0)));
        let view = editor.pane().popup().unwrap();
        assert_eq!((view.column, view.row), (5, 0));
    }

    #[test]
    fn keys_move_through_the_popup_and_wrap_around() {
        let mut editor = offered("pr", 2, 0, &["print", "prod"]);

        press(&mut editor, "C-n");
        assert_eq!(shown(&editor), Some(items(&["print", "prod"], 1)));
        press(&mut editor, "<down>");
        assert_eq!(shown(&editor), Some(items(&["print", "prod"], 0)));
        press(&mut editor, "C-p");
        assert_eq!(shown(&editor), Some(items(&["print", "prod"], 1)));
        press(&mut editor, "<up>");
        assert_eq!(shown(&editor), Some(items(&["print", "prod"], 0)));
        assert_eq!(editor.buffer().point(), 2);
    }

    #[test]
    fn tab_and_return_insert_the_selected_candidate() {
        let mut editor = offered("x <- pr", 7, 5, &["print", "prod"]);
        press(&mut editor, "C-n TAB");
        assert_eq!(editor.buffer().contents(), "x <- prod");
        assert_eq!(editor.buffer().point(), 9);
        assert_eq!(shown(&editor), None);

        let mut editor = offered("x <- pr", 7, 5, &["print", "prod"]);
        press(&mut editor, "RET");
        assert_eq!(editor.buffer().contents(), "x <- print");
    }

    #[test]
    fn typing_narrows_the_popup_until_nothing_matches() {
        let mut editor = offered("pr", 2, 0, &["print", "prod", "prompt"]);

        type_text(&mut editor, "o");
        assert_eq!(shown(&editor), Some(items(&["prod", "prompt"], 0)));
        type_text(&mut editor, "m");
        assert_eq!(shown(&editor), Some(items(&["prompt"], 0)));
        type_text(&mut editor, "x");
        assert_eq!(shown(&editor), None);
    }

    #[test]
    fn deleting_widens_the_popup_again() {
        let mut editor = offered("pro", 3, 0, &["print", "prod"]);
        assert_eq!(shown(&editor), Some(items(&["prod"], 0)));

        press(&mut editor, "DEL");

        assert_eq!(shown(&editor), Some(items(&["print", "prod"], 0)));
    }

    #[test]
    fn quitting_or_leaving_the_word_closes_the_popup() {
        let mut editor = offered("x pr", 4, 2, &["print"]);
        press(&mut editor, "C-g");
        assert_eq!(shown(&editor), None);
        assert_eq!(editor.buffer().contents(), "x pr");

        let mut editor = offered("x pr", 4, 2, &["print"]);
        press(&mut editor, "C-a");
        assert_eq!(shown(&editor), None);

        let mut editor = offered("x pr", 4, 2, &["print"]);
        press(&mut editor, "SPC");
        assert_eq!(shown(&editor), None);
        assert_eq!(editor.buffer().contents(), "x pr ");
    }

    #[test]
    fn ten_candidates_show_at_a_time_and_follow_the_selection() {
        let names: Vec<String> = (0..30).map(|n| format!("a{n:02}")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut editor = offered("a", 1, 0, &names);

        assert_eq!(editor.pane().popup().unwrap().items.len(), 10);
        for _ in 0..12 {
            press(&mut editor, "C-n");
        }

        let view = editor.pane().popup().unwrap();
        assert_eq!(view.items.len(), 10);
        assert_eq!(view.selected, 9);
        assert_eq!(view.items[9], "a12");
    }

    #[test]
    fn nothing_matching_opens_no_popup() {
        let editor = offered("zz", 2, 0, &["print"]);

        assert_eq!(shown(&editor), None);
    }
}
